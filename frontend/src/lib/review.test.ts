import { describe, expect, it } from 'vitest'

import { kindRows, lowSupport, pct, rate, tileText } from './review'
import type { ReviewDecision } from '@/types'

function decision(
  kind: ReviewDecision['actual']['kind'],
  over: Partial<ReviewDecision> = {},
): ReviewDecision {
  return {
    idx: 0,
    bakaze: 'E',
    kyoku: 1,
    honba: 0,
    turn: 1,
    window: false,
    actual: { kind },
    actual_prob: 0.5,
    top: [],
    forced: false,
    matched: false,
    missed_win: false,
    obs: '',
    ...over,
  }
}

describe('tileText', () => {
  it('names tiles the way the table does', () => {
    expect(tileText('1m')).toBe('1萬')
    expect(tileText('5pr')).toBe('赤5筒')
    expect(tileText('9s')).toBe('9索')
    expect(tileText('C')).toBe('中')
  })
})

describe('lowSupport', () => {
  it('is a ranked real choice under the threshold, nothing else', () => {
    expect(lowSupport(decision('discard', { actual_prob: 0.03 }), 0.05)).toBe(true)
    expect(lowSupport(decision('discard', { actual_prob: 0.05 }), 0.05)).toBe(false)
    expect(lowSupport(decision('discard', { actual_prob: 0.01, forced: true }), 0.05)).toBe(false)
    expect(lowSupport(decision('discard', { actual_prob: undefined }), 0.05)).toBe(false)
  })
})

describe('kindRows', () => {
  it('keeps each kind apart and skips forced and unranked decisions', () => {
    const rows = kindRows(
      [
        decision('discard', { actual_prob: 0.6, matched: true }),
        decision('discard', { actual_prob: 0.02 }),
        decision('discard', { actual_prob: 1, forced: true, matched: true }),
        decision('discard', { actual_prob: undefined }),
        decision('pass', { actual_prob: 0.9, matched: true, window: true }),
      ],
      0.05,
    )
    const discard = rows.find((r) => r.kind === 'discard')!
    expect(discard).toMatchObject({ scored: 2, matched: 1, low: 1, forced: 1 })
    expect(discard.meanProb).toBeCloseTo(0.31)
    const pass = rows.find((r) => r.kind === 'pass')!
    expect(pass).toMatchObject({ scored: 1, matched: 1, low: 0 })
    expect(rows.find((r) => r.kind === 'pon')).toBeUndefined()
  })

  it('moves the low-support count with the threshold, not the rest', () => {
    const ds = [decision('discard', { actual_prob: 0.08 })]
    expect(kindRows(ds, 0.05)[0].low).toBe(0)
    expect(kindRows(ds, 0.1)[0].low).toBe(1)
    expect(kindRows(ds, 0.1)[0].scored).toBe(1)
  })
})

describe('formatting', () => {
  it('writes rates and probabilities', () => {
    expect(rate(1, 4)).toBe('25.0%')
    expect(rate(1, 0)).toBe('—')
    expect(pct(0.0312)).toBe('3.1%')
    expect(pct(0.5)).toBe('50%')
    expect(pct(undefined)).toBe('—')
  })
})
