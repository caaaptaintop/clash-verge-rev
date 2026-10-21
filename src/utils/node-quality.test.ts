import { expect, test } from 'vitest'

import {
  broadcastLabel,
  QUALITY_BANDS,
  qualityBand,
  qualityScore,
  residentialLabel,
} from './node-quality'

test('all six exact score boundaries and widths match IPPure', () => {
  for (const [score, label] of [
    [0, '极度纯净'],
    [15, '极度纯净'],
    [16, '纯净'],
    [25, '纯净'],
    [26, '中性'],
    [40, '中性'],
    [41, '轻度风险'],
    [50, '轻度风险'],
    [51, '中度风险'],
    [70, '中度风险'],
    [71, '极度风险'],
    [100, '极度风险'],
  ] as const)
    expect(qualityBand(score).label).toBe(label)
  expect(QUALITY_BANDS.map(({ width }) => width)).toEqual([
    15, 10, 15, 10, 20, 30,
  ])
  expect(QUALITY_BANDS.map(({ color }) => color)).toEqual([
    '#166534',
    '#22C55E',
    '#84CC16',
    '#EAB308',
    '#F97316',
    '#DC2626',
  ])
})

test('unknown scores do not become zero; markers clamp and classifications stay independent', () => {
  expect(qualityScore(null)).toBeUndefined()
  expect(qualityScore(NaN)).toBeUndefined()
  expect(qualityScore(-1)).toBe(0)
  expect(qualityScore(101)).toBe(100)
  expect(residentialLabel(false)).toBe('非住宅')
  expect(residentialLabel(null)).toBe('住宅未知')
  expect(broadcastLabel(false)).toBe('原生')
  expect(broadcastLabel(true)).toBe('广播')
})
