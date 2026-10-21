export const QUALITY_BANDS = [
  { max: 15, width: 15, label: '极度纯净', color: '#166534' },
  { max: 25, width: 10, label: '纯净', color: '#22C55E' },
  { max: 40, width: 15, label: '中性', color: '#84CC16' },
  { max: 50, width: 10, label: '轻度风险', color: '#EAB308' },
  { max: 70, width: 20, label: '中度风险', color: '#F97316' },
  { max: 100, width: 30, label: '极度风险', color: '#DC2626' },
] as const

export const qualityScore = (value: number | null | undefined) =>
  typeof value === 'number' && Number.isFinite(value)
    ? Math.min(100, Math.max(0, value))
    : undefined

export const qualityBand = (score: number) =>
  QUALITY_BANDS.find(({ max }) => score <= max) ?? QUALITY_BANDS[5]

export const residentialLabel = (value: boolean | null | undefined) =>
  value === true ? '住宅' : value === false ? '非住宅' : '住宅未知'

export const broadcastLabel = (value: boolean | null | undefined) =>
  value === true ? '广播' : value === false ? '原生' : '原生/广播未知'
