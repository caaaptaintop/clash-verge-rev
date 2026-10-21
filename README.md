<h1 align="center">
  <img src="./src-tauri/icons/icon.png" alt="Clash" width="128" />
  <br>
  Clash Verge IP 质量增强版
  <br>
</h1>

<h3 align="center">
由 caaaptaintop 维护的 IP 质量增强 fork，基于 <a href="https://github.com/tauri-apps/tauri">Tauri</a> 与 Mihomo。
</h3>

<p align="center">
  语言参考（本 fork 的增强功能与下载说明以本页中文为准）：
  <a href="./docs/README_en.md">English</a> ·
  <a href="./docs/README_es.md">Español</a> ·
  <a href="./docs/README_fa.md">فارسی</a> ·
  <a href="./docs/README_ja.md">日本語</a> ·
  <a href="./docs/README_ko.md">한국어</a> ·
  <a href="./docs/README_pt.md">Português</a> ·
  <a href="./docs/README_ru.md">Русский</a> ·
  <a href="./README.md">简体中文</a>
</p>

本项目由 [caaaptaintop](https://github.com/caaaptaintop/clash-verge-rev) 维护，来源于 [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev)，以其稳定版 **v2.5.8** 为基线，维护分支为 **`feat/ip-quality`**。主要新增逐节点 IP 质量检测与展示；本 fork 与上游独立发布。

## 下载与安装

**增强版请从[本 fork 的 Releases](https://github.com/caaaptaintop/clash-verge-rev/releases) 下载。** 上游发布的安装包不包含本 fork 的 IP 质量增强功能。

当前增强版发行版本：[`v2.5.8-ipquality.1`](https://github.com/caaaptaintop/clash-verge-rev/releases/tag/v2.5.8-ipquality.1)，仅提供 **macOS Apple Silicon（arm64）** 安装包：

`Clash.Verge.IP.Quality_2.5.8-ipquality.1_macos_arm64.dmg`

请以发布页实际附件为准。Intel Mac、Windows 和 Linux 当前未提供、未验收增强版安装包。上游的多平台支持范围不代表本 fork 已有相应增强包。

### 安装与更新

1. 安装前自行备份已有配置。本 App 保留上游的应用标识（bundle id）和数据目录，可能覆盖已安装的上游 App，并共用原有配置。
2. 打开 DMG，将 App 拖入 **Applications（应用程序）**。
3. 本包使用本地 **ad hoc 签名**，未经 Apple Developer ID 签名或公证，macOS Gatekeeper 可能拦截。请先核对下载来源，并按系统提示决定是否允许打开。

本 fork 版本请从[本 fork 的 Releases](https://github.com/caaaptaintop/clash-verge-rev/releases) **手动下载更新**。当前内置更新入口仍指向上游；安装其提供的上游更新会覆盖本分支的定制功能。

### macOS 菜单栏图标排查

特定机器曾出现菜单栏图标隐藏，与从 AI 应用或终端启动后保留的旧宿主关联有关。若图标缺失，请保留现有代理运行，检查系统“菜单栏”中的应用及对应启动宿主显示开关，并检查 Thaw / Ice 的隐藏区与布局。日常请通过 Finder 或 Dock 启动 App。

## IP 质量增强功能

- **逐节点评分**：通过 IPPure 获取受测节点出口 IP 的 **0–100 风险分，越低越好**；普通卡片和 mini 卡片均显示分数、六档色带及属性，悬停可查看出口 IP、国家/地区、ASN、机构、检测时间与来源。
- **独立属性**：住宅 / 非住宅与原生 / 广播是 IPPure 返回的两类独立分类；非住宅不等于机房。“原生”对应 `isBroadcast=false`，“广播”对应 `isBroadcast=true`，字段缺失显示未知，不是本 App 的额外鉴定。
- **刷新入口**：点击代理组的“IP质量”按钮可批量刷新本组实际节点；单节点或批量延迟测速也会附带触发质量检测。组成员若是代理组，检测其当前选中的实际节点；直连、拒绝等预设项不参与。
- **独立检测链路**：检测启动独立的 Mihomo 子内核，不切换日常代理内核选中的节点。检测失败或缺少评分时显示失败 / 未知；若保留上次结果，会明确标注并显示上次检测时间。

| IP 风险分 | 等级 | 颜色 |
| :--- | :--- | :--- |
| 0–15 | 极度纯净 | 深绿 |
| >15–25 | 纯净 | 绿色 |
| >25–40 | 中性 | 黄绿色 |
| >40–50 | 轻度风险 | 黄色 |
| >50–70 | 中度风险 | 橙色 |
| >70–100 | 极度风险 | 红色 |

评分是第三方提供的参考信息，不是节点可用性、速度、匿名性或特定网站 / 服务可访问性的承诺。检测会经受测节点请求 `my.ippure.com`，第三方服务因此能看到该节点的出口 IP；新增检测请求不上传订阅内容或节点凭据，详见下方隐私说明。

## 上游界面预览

以下为上游界面示意，未展示本 fork 新增的 IP 质量信息。

| 深色模式 | 浅色模式 |
| --- | --- |
| ![预览](./docs/preview_dark.png) | ![预览](./docs/preview_light.png) |

通用安装说明和常见问题可参考[上游文档](https://clash-verge-rev.github.io/)，其中下载与更新说明不适用于本 fork 增强包。

### 上游 TG 频道：[@clash_verge_rev](https://t.me/clash_verge_re)

---

## Promotion

### ✈️ [AI云边 -- 全新架构机场 ClaudeBorder](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

🔥热销中使用本链接注册即送 **3 天免费试用**，每日 **1GB 流量**：👉 [点此注册](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

#### AI云边 -- 全新架构机场。

- 💻 多次**技术迭代后**全新亮相。
- 🗺 全**高速稳定**正价节点。
- 🌏 **海外团队**，不跑路
- 🚀 线路**冗余**设计，**自动化运维**对抗各类封锁
- 👨‍🦲 团队架构师为**大厂**网络架构师
- 💰 极致**稳定**，亲民价**价格**
- 🌐 全面支持**流媒体及各AI访问**
- 🙋 7*12小时真人客服。解决您的各类问题。

🌐 官网：👉 [https://www.claudeborder.com](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

### 🤖 [GPTKefu —— 与 Crisp 深度整合的 AI 智能客服平台](https://gptkefu.com)

- 🧠 深度理解完整对话上下文 + 图片识别，自动给出专业、精准的回复，告别机械式客服。
- ♾️ **不限回答数量**，无额度焦虑，区别于其他按条计费的 AI 客服产品。
- 💬 售前咨询、售后服务、复杂问题解答，全场景轻松覆盖，真实用户案例已验证效果。
- ⚡ 3 分钟极速接入，零门槛上手，即刻提升客服效率与客户满意度。
- 🎁 高级套餐免费试用 14 天，先体验后付费：👉 [立即试用](https://gptkefu.com)
- 📢 智能客服TG 频道：[@crisp_ai](https://t.me/crisp_ai)

---

## 上游基础功能

- 基于性能强劲的 Rust 和 Tauri 2 框架
- 内置[Clash.Meta(mihomo)](https://github.com/MetaCubeX/mihomo)内核，并支持切换 `Alpha` 版本内核。
- 简洁美观的用户界面，支持自定义主题颜色、代理组/托盘图标以及 `CSS Injection`。
- 配置文件管理和增强（Merge 和 Script），配置文件语法提示。
- 系统代理和守卫、`TUN(虚拟网卡)` 模式。
- 可视化节点和规则编辑
- WebDav 配置备份和同步

### 常见问题

参考[上游常见问题](https://clash-verge-rev.github.io/faq/windows.html)（该链接为 Windows 专题，不代表本 fork 提供 Windows 增强包）。

### 支持上游开发

[捐助Clash Verge Rev的开发](https://github.com/sponsors/clash-verge-rev)

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md) for more details.

To run the development server, execute the following commands after all prerequisites for **Tauri** are installed:

```shell
pnpm i
pnpm run prebuild
pnpm dev
```

`pnpm dev` preserves the Development Channel's installed service state: an
existing service is used, while a previously uninstalled service remains
uninstalled and the app starts in Sidecar mode. Use `pnpm dev:service` to
explicitly install or update the isolated development service before launch,
or `pnpm dev:sidecar` to force the unprivileged Sidecar workflow.

## Contributions

Issue and PR welcome!

## Acknowledgement

Clash Verge rev was based on or inspired by these projects and so on:

- [zzzgydi/clash-verge](https://github.com/zzzgydi/clash-verge): A Clash GUI based on tauri. Supports Windows, macOS and Linux.
- [tauri-apps/tauri](https://github.com/tauri-apps/tauri): Build smaller, faster, and more secure desktop applications with a web frontend.
- [Dreamacro/clash](https://github.com/Dreamacro/clash): A rule-based tunnel in Go.
- [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo): A rule-based tunnel in Go.
- [Fndroid/clash_for_windows_pkg](https://github.com/Fndroid/clash_for_windows_pkg): A Windows/macOS GUI based on Clash.
- [vitejs/vite](https://github.com/vitejs/vite): Next generation frontend tooling. It's fast!

## 隐私

配置与日志保存在本地；本增强版的网络请求说明见[隐私政策](./PRIVACY.md)。本 fork 新增的 IP 质量检测会在点击“IP质量”刷新或进行节点测速时，通过受测节点向 **`https://my.ippure.com/v1/info`** 发起第三方请求。IPPure 能看到受测节点的出口 IP 及请求元数据，并按其自身政策处理这些信息；本 App 无法保证第三方不记录请求。

新增检测逻辑在本地读取配置以建立独立检测链路，向 IPPure 发出的请求不上传订阅内容、订阅地址或节点凭据。检测结果中的出口 IP、风险分和属性会显示在界面上；缺失或失败不会被当成低风险。因此，上游“不收集用户数据”的描述不能被理解为本增强版不会向第三方发起网络请求。

## 许可证

沿用 GPL-3.0 许可证，详见 [LICENSE](./LICENSE)。
