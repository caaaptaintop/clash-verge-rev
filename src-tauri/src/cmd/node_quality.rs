use super::{CmdResult, StringifyErr as _};
use crate::{
    config::Config,
    core::{
        CoreManager,
        proxy_view::{ProxyNodeSource, ProxyViewV1},
    },
    utils::dirs,
};
use anyhow::{Context as _, Result, ensure};
use base64::Engine as _;
use reqwest::{Client, Proxy, StatusCode};
use serde::Serialize;
use serde_json::Value as Json;
use serde_yaml_ng::{Mapping, Value};
use sha2::{Digest as _, Sha256};
use std::{
    collections::HashSet,
    fs,
    net::{IpAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    sync::LazyLock,
    time::Duration,
};
use tokio::{process::Command, sync::Semaphore};

const QUALITY_URL: &str = "https://my.ippure.com/v1/info";
const MAX_RESPONSE: usize = 16 * 1024;
static PROBES: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeQualityReport {
    ip: String,
    fraud_score: Option<u64>,
    is_residential: Option<bool>,
    is_broadcast: Option<bool>,
    country_code: String,
    as_organization: String,
    asn: Option<u64>,
    checked_at: String,
    generation: String,
}

struct Snapshot {
    config: Mapping,
    view: ProxyViewV1,
    generation: String,
}

async fn snapshot() -> Result<Snapshot> {
    let epoch = CoreManager::global()
        .current_core_readiness_generation()
        .context("代理内核未连接，请先启动代理")?;
    let profiles = Config::profiles().await.latest_arc();
    ensure!(profiles.current.is_some(), "尚未启用订阅配置");
    let mut config = Config::runtime()
        .await
        .latest_arc()
        .config
        .clone()
        .context("代理配置尚未同步")?;
    let view = tokio::time::timeout(
        Duration::from_secs(4),
        super::proxy::proxy_view(Some(Duration::from_secs(2))),
    )
    .await
    .context("读取代理节点超时")?
    .map_err(|failure| anyhow::anyhow!(failure.detail))?;
    let mut hasher = Sha256::new();
    hasher.update(epoch.to_le_bytes());
    hasher.update(serde_json::to_vec(&*profiles)?);
    hasher.update(serde_yaml_ng::to_string(&config)?);
    // Freeze cached providers as inline payloads: inspection never downloads subscriptions.
    if let Some(Value::Mapping(providers)) = config.get_mut("proxy-providers") {
        let home = fs::canonicalize(dirs::app_home_dir()?)?;
        for (_, provider) in providers.iter_mut() {
            let provider = provider.as_mapping_mut().context("节点供应商配置无效")?;
            if provider.get("type").and_then(Value::as_str) != Some("inline") {
                let path = provider
                    .get("path")
                    .and_then(Value::as_str)
                    .context("节点供应商尚无本地缓存，无法安全检测")?;
                let path = fs::canonicalize(home.join(path)).context("节点供应商缓存不可用")?;
                ensure!(path.starts_with(&home), "节点供应商缓存不在应用数据目录中");
                ensure!(fs::metadata(&path)?.len() <= 32 * 1024 * 1024, "节点供应商缓存过大");
                let bytes = fs::read(path)?;
                hasher.update(&bytes);
                let cached: Value = serde_yaml_ng::from_slice(&bytes).context("节点供应商缓存格式无效")?;
                let payload = cached
                    .get("proxies")
                    .and_then(Value::as_sequence)
                    .context("节点供应商缓存缺少节点")?;
                provider.insert("type".into(), "inline".into());
                provider.insert("payload".into(), Value::Sequence(payload.clone()));
            }
            provider.remove("url");
            provider.remove("path");
            provider.remove("interval");
            provider.remove("proxy");
            provider.remove("header");
            provider.remove("health-check");
        }
    }
    for provider in &view.providers {
        hasher.update(provider.name.as_bytes());
        if let Some(updated) = &provider.updated_at {
            hasher.update(updated.as_bytes());
        }
    }
    hasher.update(route_identity(&view)?);
    ensure!(
        CoreManager::global().current_core_readiness_generation() == Some(epoch),
        "代理内核已变化，请重新检测"
    );
    Ok(Snapshot {
        config,
        view,
        generation: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()),
    })
}

#[tauri::command]
pub async fn get_node_quality_context() -> CmdResult<String> {
    snapshot().await.map(|snapshot| snapshot.generation).stringify_err()
}

fn reserved(name: &str) -> bool {
    name.trim().is_empty() || matches!(name, "DIRECT" | "REJECT" | "REJECT-DROP" | "PASS" | "COMPATIBLE")
}

fn eligible_type(kind: &str) -> bool {
    !matches!(
        kind.to_ascii_lowercase().replace('-', "").as_str(),
        "direct" | "reject" | "rejectdrop" | "pass" | "compatible"
    )
}

fn route_identity(view: &ProxyViewV1) -> Result<Vec<u8>> {
    let groups = view
        .groups
        .iter()
        .chain(view.global.iter())
        .map(|group| serde_json::json!([group.name, group.proxy_type, group.now, group.members]))
        .collect::<Vec<_>>();
    let nodes = view
        .records
        .values()
        .map(|node| serde_json::json!([node.record_id, node.name, node.proxy_type, node.source]))
        .collect::<Vec<_>>();
    Ok(serde_json::to_vec(&(groups, nodes))?)
}

fn check_dependency(snapshot: &Snapshot, name: &str, visiting: &mut HashSet<String>) -> Result<()> {
    if reserved(name) {
        return Ok(());
    }
    ensure!(
        visiting.len() < 64 && visiting.insert(name.into()),
        "代理链存在循环或过深，已停止检测"
    );
    let group = snapshot
        .view
        .groups
        .iter()
        .chain(snapshot.view.global.iter())
        .find(|group| group.name == name);
    if let Some(group) = group {
        let selected = group.now.as_deref().context("代理链依赖的组无法固定选路，已停止检测")?;
        check_dependency(snapshot, selected, visiting)?;
    } else {
        let nodes = snapshot
            .view
            .records
            .values()
            .filter(|node| node.name == name)
            .collect::<Vec<_>>();
        ensure!(nodes.len() == 1, "代理链依赖缺失或有歧义，已停止检测");
        match &nodes[0].source {
            ProxyNodeSource::Core { proxy_name } => {
                if let Some(dialer) = snapshot
                    .config
                    .get("proxies")
                    .and_then(Value::as_sequence)
                    .into_iter()
                    .flatten()
                    .find(|proxy| proxy.get("name").and_then(Value::as_str) == Some(proxy_name))
                    .and_then(|proxy| proxy.get("dialer-proxy"))
                    .and_then(Value::as_str)
                {
                    check_dependency(snapshot, dialer, visiting)?;
                }
            }
            ProxyNodeSource::Provider { provider_name, .. } => {
                let provider = snapshot
                    .config
                    .get("proxy-providers")
                    .and_then(|providers| providers.get(provider_name.as_str()));
                let has_dialer = provider.is_some_and(|provider| {
                    provider
                        .get("override")
                        .and_then(|overrides| overrides.get("dialer-proxy"))
                        .is_some()
                        || provider
                            .get("payload")
                            .and_then(Value::as_sequence)
                            .into_iter()
                            .flatten()
                            .any(|proxy| proxy.get("dialer-proxy").is_some())
                });
                ensure!(!has_dialer, "供应商代理链依赖无法唯一还原，已停止检测");
            }
        }
    }
    visiting.remove(name);
    Ok(())
}

fn check_dialer_dependencies(snapshot: &Snapshot, value: &Value) -> Result<()> {
    match value {
        Value::Mapping(mapping) => {
            if let Some(dialer) = mapping.get("dialer-proxy").and_then(Value::as_str) {
                check_dependency(snapshot, dialer, &mut HashSet::new())?;
            }
            for (_, nested) in mapping {
                check_dialer_dependencies(snapshot, nested)?;
            }
        }
        Value::Sequence(sequence) => {
            for nested in sequence {
                check_dialer_dependencies(snapshot, nested)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn probe_config(
    snapshot: &Snapshot,
    record_id: &str,
    port: u16,
    controller: u16,
    token: &str,
) -> Result<(Mapping, String)> {
    let node = snapshot
        .view
        .records
        .get(record_id)
        .context("节点已不存在，请刷新节点列表")?;
    ensure!(!reserved(&node.name), "该出口不支持 IP 质量检测");
    let kind = serde_json::to_value(&node.proxy_type)?;
    ensure!(kind.as_str().is_some_and(eligible_type), "该节点类型不支持 IP 质量检测");
    check_dialer_dependencies(snapshot, &Value::Mapping(snapshot.config.clone()))?;
    let target_group = format!("IP-Quality-{token}");
    let mut isolated = Mapping::new();
    for key in [
        "proxies",
        "proxy-providers",
        "ipv6",
        "interface-name",
        "routing-mark",
        "global-client-fingerprint",
        "tcp-concurrent",
    ] {
        if let Some(value) = snapshot.config.get(key) {
            isolated.insert(key.into(), value.clone());
        }
    }
    let mut groups = snapshot
        .config
        .get("proxy-groups")
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    for group in &mut groups {
        let group = group.as_mapping_mut().context("代理组配置无效")?;
        ensure!(
            group.get("type").and_then(Value::as_str) != Some("relay"),
            "relay 链路暂不支持隔离检测"
        );
        group.retain(|key, _| {
            matches!(
                key.as_str(),
                Some(
                    "name"
                        | "proxies"
                        | "use"
                        | "include-all"
                        | "include-all-proxies"
                        | "include-all-providers"
                        | "filter"
                        | "exclude-filter"
                        | "exclude-type"
                )
            )
        });
        group.insert("type".into(), "select".into());
    }
    let target = match &node.source {
        ProxyNodeSource::Core { proxy_name } => {
            let count = isolated
                .get("proxies")
                .and_then(Value::as_sequence)
                .into_iter()
                .flatten()
                .filter(|proxy| proxy.get("name").and_then(Value::as_str) == Some(proxy_name.as_str()))
                .count();
            ensure!(count == 1, "节点配置缺失或重名，已停止检测");
            proxy_name.clone()
        }
        ProxyNodeSource::Provider {
            provider_name,
            proxy_name,
        } => {
            let count = snapshot.view.records.values().filter(|record| matches!(&record.source,
                ProxyNodeSource::Provider {provider_name: provider, proxy_name: name} if provider == provider_name && name == proxy_name)).count();
            ensure!(count == 1, "同一供应商存在重名节点，已停止检测");
            ensure!(
                isolated
                    .get("proxy-providers")
                    .and_then(Value::as_mapping)
                    .is_some_and(|providers| providers.contains_key(provider_name.as_str())),
                "节点供应商已不存在"
            );
            groups.push(serde_yaml_ng::to_value(serde_json::json!({
                "name": target_group, "type": "select", "use": [provider_name],
                "filter": format!("^{}$", regex::escape(proxy_name))
            }))?);
            target_group
        }
    };
    isolated.insert("proxy-groups".into(), Value::Sequence(groups));
    let guard: Mapping = serde_yaml_ng::from_str(&format!(
        "mode: rule\nlog-level: silent\nallow-lan: false\nexternal-controller: '127.0.0.1:{controller}'\nsecret: '{token}'\ndns:\n  enable: false\ntun:\n  enable: false\nprofile:\n  store-selected: false\n  store-fake-ip: false\nrules:\n  - MATCH,REJECT\n"
    ))?;
    isolated.extend(guard);
    isolated.insert(
        "listeners".into(),
        serde_yaml_ng::to_value(serde_json::json!([{
            "name": "ip-quality", "type": "mixed", "listen": "127.0.0.1", "port": port,
        "proxy": target, "udp": false, "users": [{"username": token, "password": token}]
        }]))?,
    );
    Ok((isolated, target))
}

struct ProbeDirectory(PathBuf);
impl ProbeDirectory {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("verge-quality-{}", nanoid::nanoid!()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for ProbeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn controller_request(
    client: &Client,
    base: &str,
    token: &str,
    method: reqwest::Method,
    segments: &[&str],
    body: Option<Json>,
) -> Result<Json> {
    let mut url = reqwest::Url::parse(base)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid controller URL"))?
        .extend(segments);
    let mut request = client.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?.error_for_status()?;
    if response.status() == StatusCode::NO_CONTENT {
        return Ok(Json::Null);
    }
    Ok(response.json().await?)
}

fn isolate_environment(command: &mut Command) {
    command.env_clear().env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
}

fn probe_command(binary: &Path, directory: &Path, config: &Path) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = {
        // The pipe closes even when the GUI is killed. This supervisor then reaps
        // its private core and removes its configuration without touching daily cores.
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(concat!(
                "directory=$1; shift\n",
                "\"$@\" < /dev/null &\n",
                "probe=$!\n",
                "trap 'kill -KILL %1 2>/dev/null; wait \"$probe\" 2>/dev/null; /bin/rm -rf -- \"$directory\"' EXIT\n",
                "trap 'exit' HUP INT TERM\n",
                "read -r finished\n"
            ))
            .arg("verge-quality-supervisor")
            .arg(directory)
            .arg(binary);
        command.stdin(Stdio::piped());
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = {
        let mut command = Command::new(binary);
        command.stdin(Stdio::null()).kill_on_drop(true);
        command
    };
    isolate_environment(&mut command);
    command
        .arg("-d")
        .arg(directory)
        .arg("-f")
        .arg(config)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn validate_provider_target(provider: &Json, name: &str) -> Result<()> {
    let matching = provider["proxies"]
        .as_array()
        .context("供应商节点尚未就绪")?
        .iter()
        .filter(|proxy| proxy["name"].as_str() == Some(name))
        .collect::<Vec<_>>();
    ensure!(
        matching.len() == 1 && matching[0]["type"].as_str().is_some_and(eligible_type),
        "供应商节点重名或类型不支持，已停止检测"
    );
    Ok(())
}

async fn run_probe(snapshot: &Snapshot, record_id: &str) -> Result<NodeQualityReport> {
    let node = snapshot.view.records.get(record_id).context("节点已不存在")?;
    let proxy_socket = TcpListener::bind(("127.0.0.1", 0))?;
    let control_socket = TcpListener::bind(("127.0.0.1", 0))?;
    let port = proxy_socket.local_addr()?.port();
    let controller = control_socket.local_addr()?.port();
    let token = nanoid::nanoid!(32);
    let (config, target) = probe_config(snapshot, record_id, port, controller, &token)?;
    let directory = ProbeDirectory::new()?;
    let config_path = directory.0.join("config.yaml");
    fs::write(&config_path, serde_yaml_ng::to_string(&config)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))?;
    }
    let binary = std::env::current_exe()?
        .parent()
        .context("应用路径不可用")?
        .join(if cfg!(windows) {
            "verge-mihomo.exe"
        } else {
            "verge-mihomo"
        });
    ensure!(binary.is_file(), "找不到独立检测内核");
    drop(proxy_socket);
    drop(control_socket);
    let mut child = probe_command(&binary, &directory.0, &config_path)
        .spawn()
        .context("无法启动独立检测内核")?;
    let result = async {
        let api = Client::builder().no_proxy().timeout(Duration::from_secs(2)).build()?;
        let base = format!("http://127.0.0.1:{controller}");
        let ready = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                ensure!(child.try_wait()?.is_none(), "独立检测内核启动失败");
                if api
                    .get(format!("{base}/version"))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("独立检测内核启动超时")?;
        ready?;
        // Restore selections only inside the private child, including dialer-proxy dependencies.
        for group in snapshot.view.groups.iter().chain(snapshot.view.global.iter()) {
            if let Some(selected) = &group.now {
                controller_request(
                    &api,
                    &base,
                    &token,
                    reqwest::Method::PUT,
                    &["proxies", &group.name],
                    Some(serde_json::json!({"name":selected})),
                )
                .await
                .context("无法固定独立检测链路")?;
            }
        }
        let selected =
            controller_request(&api, &base, &token, reqwest::Method::GET, &["proxies", &target], None).await?;
        match &node.source {
            ProxyNodeSource::Provider {
                proxy_name,
                provider_name,
            } => {
                let provider = controller_request(
                    &api,
                    &base,
                    &token,
                    reqwest::Method::GET,
                    &["providers", "proxies", provider_name],
                    None,
                )
                .await?;
                validate_provider_target(&provider, proxy_name)?;
                ensure!(
                    selected["all"]
                        .as_array()
                        .is_some_and(|all| all.len() == 1 && all[0].as_str() == Some(proxy_name))
                        && selected["now"].as_str() == Some(proxy_name),
                    "供应商节点不唯一或已变化，已停止检测"
                );
            }
            ProxyNodeSource::Core { .. } => ensure!(
                selected["name"].as_str() == Some(node.name.as_str())
                    && selected["type"].as_str().is_some_and(eligible_type),
                "检测节点身份不匹配"
            ),
        }
        let proxy = Proxy::all(format!("http://127.0.0.1:{port}"))?.basic_auth(&token, &token);
        let client = Client::builder()
            .no_proxy()
            .proxy(proxy)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build()?;
        let mut response = client
            .get(QUALITY_URL)
            .header("Accept", "application/json")
            .send()
            .await
            .context("节点 IP 质量请求失败")?;
        ensure!(
            response.status() == StatusCode::OK,
            "IPPure 返回状态 {}",
            response.status().as_u16()
        );
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(body.len() + chunk.len() <= MAX_RESPONSE, "IPPure 响应超过 16 KiB");
            body.extend_from_slice(&chunk);
        }
        parse_report(&body, &snapshot.generation)
    }
    .await;
    // Reap before the directory is removed, also after every failure path.
    #[cfg(target_os = "macos")]
    drop(child.stdin.take());
    #[cfg(not(target_os = "macos"))]
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

fn parse_report(body: &[u8], generation: &str) -> Result<NodeQualityReport> {
    ensure!(body.len() <= MAX_RESPONSE, "IPPure 响应超过 16 KiB");
    let json: Json = serde_json::from_slice(body).context("IPPure 响应格式无效")?;
    ensure!(json.is_object(), "IPPure 响应不是对象");
    let ip = json["ip"].as_str().context("IPPure 响应缺少出口 IP")?;
    let _: IpAddr = ip.parse().context("IPPure 返回无效 IP")?;
    Ok(NodeQualityReport {
        ip: ip.into(),
        fraud_score: json["fraudScore"].as_u64().filter(|score| *score <= 100),
        is_residential: json["isResidential"].as_bool(),
        is_broadcast: json["isBroadcast"].as_bool(),
        country_code: json["countryCode"].as_str().unwrap_or_default().into(),
        as_organization: json["asOrganization"].as_str().unwrap_or_default().into(),
        asn: json["asn"].as_u64(),
        checked_at: chrono::Utc::now().to_rfc3339(),
        generation: generation.into(),
    })
}

#[tauri::command]
pub async fn check_node_quality(record_id: String, generation: String) -> CmdResult<NodeQualityReport> {
    async {
        let _permit = tokio::time::timeout(Duration::from_secs(17), PROBES.acquire())
            .await
            .context("IP 质量检测排队超时")??;
        let before = snapshot().await?;
        ensure!(before.generation == generation, "节点配置已变化，请重新检测");
        let report = run_probe(&before, &record_id).await?;
        ensure!(
            snapshot().await?.generation == generation,
            "节点配置已变化，已丢弃旧检测结果"
        );
        Ok::<_, anyhow::Error>(report)
    }
    .await
    .stringify_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<Snapshot> {
        use crate::core::proxy_view::{ProxyViewBuilder, ProxyViewInput};
        let view = ProxyViewBuilder::build(ProxyViewInput {
            runtime_group_order: vec!["Auto".into()],
            group_scopes: Default::default(),
            proxies: serde_json::from_value(serde_json::json!({"proxies": {
                "GLOBAL": {"type":"Selector", "now":"node-a", "all":["node-a","DIRECT"]},
                "Auto": {"type":"URLTest", "now":"node-a", "all":["node-a"]},
                "node-a": {"name":"node-a", "type":"Shadowsocks"},
                "local-alias": {"name":"local-alias", "type":"Direct"}
            }}))?,
            providers: None,
        });
        let config = serde_yaml_ng::from_str(
            "mixed-port: 7897\nport: 7899\nexternal-controller-unix: /daily.sock\nexternal-ui-url: https://example.invalid/ui.zip\nntp:\n  enable: true\ntun:\n  enable: true\ndns:\n  enable: true\n  listen: 0.0.0.0:53\nproxies:\n  - {name: node-a, type: ss}\n  - {name: local-alias, type: direct}\nproxy-groups:\n  - {name: Auto, type: url-test, proxies: [node-a], url: https://example.invalid, interval: 1}\n",
        )?;
        Ok(Snapshot {
            config,
            view,
            generation: "fixture".into(),
        })
    }

    #[test]
    fn isolated_config_cannot_inherit_daily_listeners_or_background_downloads() -> Result<()> {
        let snapshot = fixture()?;
        let node = snapshot
            .view
            .records
            .values()
            .find(|node| node.name == "node-a")
            .context("missing fixture node")?;
        let (config, target) = probe_config(&snapshot, &node.record_id, 30001, 30002, "fixture-token")?;
        assert_eq!(target, "node-a");
        for key in [
            "mixed-port",
            "port",
            "socks-port",
            "external-controller-unix",
            "external-ui-url",
            "ntp",
            "tunnels",
        ] {
            assert!(!config.contains_key(key), "unexpected inherited setting: {key}");
        }
        assert_eq!(config["tun"]["enable"].as_bool(), Some(false));
        assert_eq!(config["dns"]["enable"].as_bool(), Some(false));
        assert_eq!(config["listeners"][0]["listen"].as_str(), Some("127.0.0.1"));
        assert_eq!(config["listeners"][0]["proxy"].as_str(), Some("node-a"));
        assert_eq!(
            config["listeners"][0]["users"][0]["username"].as_str(),
            Some("fixture-token")
        );
        assert_eq!(config["rules"][0].as_str(), Some("MATCH,REJECT"));
        assert!(config["proxy-groups"][0].get("url").is_none());
        let alias = snapshot
            .view
            .records
            .values()
            .find(|node| node.name == "local-alias")
            .context("missing alias")?;
        assert!(probe_config(&snapshot, &alias.record_id, 30001, 30002, "t").is_err());
        Ok(())
    }

    #[test]
    fn live_global_selections_change_identity_and_unfixed_dependencies_fail() -> Result<()> {
        let mut snapshot = fixture()?;
        let before = route_identity(&snapshot.view)?;
        let dependency = serde_yaml_ng::from_str("dialer-proxy: GLOBAL")?;
        assert!(check_dialer_dependencies(&snapshot, &dependency).is_ok());
        snapshot.view.global.as_mut().context("missing GLOBAL")?.now = Some("Auto".into());
        assert!(check_dialer_dependencies(&snapshot, &dependency).is_ok());
        snapshot.view.groups[0].now = None;
        assert!(check_dialer_dependencies(&snapshot, &dependency).is_err());
        snapshot.view.groups[0].now = Some("GLOBAL".into());
        assert!(check_dialer_dependencies(&snapshot, &dependency).is_err());
        snapshot.view.groups[0].now = Some("node-a".into());
        snapshot.view.global.as_mut().context("missing GLOBAL")?.now = Some("DIRECT".into());
        assert_ne!(route_identity(&snapshot.view)?, before);
        snapshot.view.global.as_mut().context("missing GLOBAL")?.now = None;
        assert!(check_dialer_dependencies(&snapshot, &dependency).is_err());
        snapshot.view.groups[0].now = None;
        assert!(check_dialer_dependencies(&snapshot, &serde_yaml_ng::from_str("dialer-proxy: Auto")?).is_err());
        assert!(check_dialer_dependencies(&snapshot, &serde_yaml_ng::from_str("dialer-proxy: missing")?).is_err());
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn closing_parent_pipe_reaps_probe_and_private_config() -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = ProbeDirectory::new()?;
        let binary = directory.0.join("fixture core");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$2/probe.pid\"\nexec /bin/sleep 60\n",
        )?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
        let mut child = probe_command(&binary, &directory.0, &directory.0.join("config.yaml")).spawn()?;
        let pid_path = directory.0.join("probe.pid");
        let pid = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(pid) = fs::read_to_string(&pid_path)
                    && let Ok(pid) = pid.parse::<i32>()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        drop(child.stdin.take());
        tokio::time::timeout(Duration::from_secs(2), child.wait()).await??;
        // SAFETY: signal zero only checks existence of this fixture's process.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert!(!directory.0.exists());
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn supervisor_never_signals_a_completed_job_by_stale_pid() -> Result<()> {
        let output = Command::new("/bin/sh")
            .args(["-c", "true & probe=$!; sleep .1; kill -KILL %1 2>/dev/null; result=$?; wait \"$probe\"; test \"$result\" != 0"])
            .output()
            .await?;
        assert!(output.status.success());
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_child_cannot_inherit_mihomo_environment_overrides() -> Result<()> {
        let mut command = Command::new("/usr/bin/env");
        for name in [
            "CLASH_CONFIG_STRING",
            "CLASH_CONFIG_FILE",
            "CLASH_OVERRIDE_EXTERNAL_CONTROLLER",
            "CLASH_OVERRIDE_EXTERNAL_CONTROLLER_UNIX",
            "CLASH_OVERRIDE_SECRET",
            "CLASH_POST_UP",
            "HTTP_PROXY",
        ] {
            command.env(name, "unsafe-fixture-value");
        }
        isolate_environment(&mut command);
        let output = command.output().await?;
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)?.trim(),
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin"
        );
        Ok(())
    }

    #[test]
    fn duplicate_provider_names_and_direct_aliases_are_rejected_before_request() {
        let node = serde_json::json!({"name":"same-name", "type":"Shadowsocks"});
        assert!(validate_provider_target(&serde_json::json!({"proxies":[node.clone()]}), "same-name").is_ok());
        assert!(validate_provider_target(&serde_json::json!({"proxies":[node.clone(),node]}), "same-name").is_err());
        assert!(
            validate_provider_target(
                &serde_json::json!({"proxies":[{"name":"same-name","type":"Direct"}]}),
                "same-name"
            )
            .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicitly supplied local profile and bundled Mihomo binary"]
    async fn isolated_live_probe_smoke() -> Result<()> {
        use crate::core::proxy_view::{ProxyViewBuilder, ProxyViewInput};
        let path = std::env::var_os("VERGE_QUALITY_SMOKE_CONFIG").context("missing smoke fixture")?;
        let name = std::env::var("VERGE_QUALITY_SMOKE_NODE").context("missing smoke target")?;
        let config =
            serde_yaml_ng::from_slice(&fs::read(path)?).map_err(|_| anyhow::anyhow!("invalid smoke profile"))?;
        let view = ProxyViewBuilder::build(ProxyViewInput {
            runtime_group_order: vec![],
            group_scopes: Default::default(),
            providers: None,
            proxies: serde_json::from_value(serde_json::json!({"proxies": {
                (name.clone()): {"name":name, "type":"Shadowsocks"}
            }}))?,
        });
        let id = view.records.keys().next().context("missing smoke node")?.clone();
        let snapshot = Snapshot {
            config,
            view,
            generation: "live-smoke".into(),
        };
        let report = run_probe(&snapshot, &id).await?;
        assert_eq!(report.generation, "live-smoke");
        assert!(report.ip.parse::<IpAddr>().is_ok());
        println!(
            "IPPure smoke passed: country={}, score={:?}",
            report.country_code, report.fraud_score
        );
        Ok(())
    }

    #[test]
    fn report_requires_an_ip_and_keeps_unknown_classifications_unknown() -> Result<()> {
        let parsed = parse_report(
            br#"{"ip":"2001:db8::1","fraudScore":48,"isResidential":false,"isBroadcast":true}"#,
            "generation",
        )?;
        assert_eq!(parsed.fraud_score, Some(48));
        assert_eq!(parsed.is_residential, Some(false));
        assert_eq!(parsed.is_broadcast, Some(true));
        for invalid in ["", "example.com", "1.2.3", "fe80::1%en0"] {
            assert!(parse_report(serde_json::json!({"ip":invalid}).to_string().as_bytes(), "g").is_err());
        }
        for score in [
            serde_json::json!("48"),
            serde_json::json!(48.1),
            serde_json::json!(-1),
            serde_json::json!(101),
        ] {
            let parsed = parse_report(
                serde_json::json!({"ip":"192.0.2.1","fraudScore":score,"isResidential":"false"})
                    .to_string()
                    .as_bytes(),
                "g",
            )?;
            assert_eq!(parsed.fraud_score, None);
            assert_eq!(parsed.is_residential, None);
        }
        assert!(parse_report(b"[]", "g").is_err());
        assert!(parse_report(&vec![b' '; MAX_RESPONSE + 1], "g").is_err());
        Ok(())
    }
}
