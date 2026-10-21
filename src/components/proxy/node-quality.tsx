import { Box, darken, lighten, Tooltip } from '@mui/material'
import { useSyncExternalStore } from 'react'

import { nodeQualityManager, type NodeQualityState } from '@/store/node-quality'
import type { ResolvedProxyMember } from '@/types/proxy-view'
import {
  broadcastLabel,
  QUALITY_BANDS,
  qualityBand,
  qualityScore,
  residentialLabel,
} from '@/utils/node-quality'

export const NodeQuality = ({ member }: { member: ResolvedProxyMember }) => {
  const snapshot = useSyncExternalStore(nodeQualityManager.subscribe, () =>
    nodeQualityManager.snapshot(member),
  )
  const state = snapshot && 'status' in snapshot ? snapshot : undefined
  const node = nodeQualityManager.node(member)
  if (!node) return null
  return <NodeQualityDetails state={state} />
}

export const NodeQualityDetails = ({ state }: { state?: NodeQualityState }) => {
  const report = state?.report
  const score = qualityScore(report?.fraudScore)
  const band = score === undefined ? undefined : qualityBand(score)
  const busy = state?.status === 'queued' || state?.status === 'checking'
  const status = busy
    ? report
      ? '刷新中 · 上次结果'
      : state.status === 'queued'
        ? '排队中'
        : '检测中'
    : state?.status === 'failed'
      ? report
        ? '检测失败 · 上次结果'
        : '检测失败，请重测'
      : ''
  const time = report ? new Date(report.checkedAt).toLocaleString() : ''
  const details = report
    ? `IP：${report.ip}\n国家/地区：${report.countryCode || '未知'}\nASN：${report.asn == null ? '未知' : `AS${report.asn}`}\n机构：${report.asOrganization || '未知'}\n检测时间：${time}\n来源：IPPure\n住宅与广播属性均为 IPPure 的独立分类；非住宅不等于机房。原生表示 isBroadcast=false，广播表示 isBroadcast=true，缺失为未知。风险分数越低越好。${status ? `\n${status}` : ''}`
    : `来源：IPPure\n${status || '点击节点测速或 IP质量 刷新检测'}\n住宅与广播属性均为 IPPure 的独立分类，缺失为未知。`

  return (
    <Tooltip title={<Box sx={{ whiteSpace: 'pre-line' }}>{details}</Box>} arrow>
      <Box
        component="span"
        sx={{
          display: 'block',
          mt: 0.5,
          minWidth: 0,
          fontSize: 11,
          lineHeight: '16px',
          color: 'text.secondary',
        }}
      >
        <Box
          component="span"
          sx={(theme) => ({
            display: 'block',
            color:
              band &&
              (theme.palette.mode === 'dark'
                ? lighten(band.color, 0.25)
                : darken(band.color, 0.3)),
            overflowWrap: 'anywhere',
          })}
        >
          IP风险{' '}
          {score === undefined ? '未知' : `${score}/100 · ${band!.label}`}
        </Box>
        <Box
          component="span"
          sx={{ display: 'block', overflowWrap: 'anywhere' }}
        >
          {residentialLabel(report?.isResidential)} ·{' '}
          {broadcastLabel(report?.isBroadcast)}
        </Box>
        {score !== undefined && (
          <Box
            component="span"
            sx={{ display: 'block', maxWidth: 240, pt: 0.5 }}
          >
            <Box
              component="span"
              role="img"
              aria-label={`IP风险 ${score}/100，越低越好`}
              sx={{
                display: 'flex',
                position: 'relative',
                height: 6,
                borderRadius: 1,
                overflow: 'hidden',
              }}
            >
              {QUALITY_BANDS.map(({ color, width }) => (
                <Box
                  component="span"
                  key={color}
                  sx={{ width: `${width}%`, bgcolor: color }}
                />
              ))}
              <Box
                component="span"
                sx={{
                  position: 'absolute',
                  left: `calc(${score}% - ${score * 0.04}px)`,
                  top: 0,
                  bottom: 0,
                  width: 4,
                  bgcolor: 'text.primary',
                  boxShadow: '0 0 0 1px white',
                }}
              />
            </Box>
            <Box
              component="span"
              sx={{
                display: 'flex',
                justifyContent: 'space-between',
                fontSize: 9,
                lineHeight: '12px',
              }}
            >
              <span>0</span>
              <span>越低越好</span>
              <span>100</span>
            </Box>
          </Box>
        )}
        {status && (
          <Box
            component="span"
            sx={{
              display: 'block',
              color:
                state?.status === 'failed' ? 'error.main' : 'text.secondary',
            }}
          >
            {status}
          </Box>
        )}
        {state?.status === 'failed' && report && (
          <Box component="span" sx={{ display: 'block', fontSize: 10 }}>
            上次：{time}
          </Box>
        )}
      </Box>
    </Tooltip>
  )
}
