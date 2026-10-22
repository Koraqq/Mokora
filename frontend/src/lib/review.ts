// Words and numbers for the 對局覆盤 page: the kinds of action, tiles as
// text, and agreement with the model recomputed at the page's threshold.
//
// The review measures how far a recorded game is from a built-in model's own
// policy. Nothing here is a win rate or a verdict: a "low model support" play
// is one the model rarely makes, not a mistake.

import type { ReviewAction, ReviewDecision, ReviewKind } from '@/types'

/** The kinds of action, in the order the page lists them. */
export const REVIEW_KINDS: ReviewKind[] = [
  'discard',
  'riichi',
  'pon',
  'chi',
  'daiminkan',
  'ankan',
  'kakan',
  'kita',
  'hora',
  'ryukyoku',
  'pass',
]

/** Below this probability a play is flagged "low model support" (5%). */
export const LOW_SUPPORT_DEFAULT = 0.05

const HONORS: Record<string, string> = {
  E: '東',
  S: '南',
  W: '西',
  N: '北',
  P: '白',
  F: '發',
  C: '中',
}
const SUITS: Record<string, string> = { m: '萬', p: '筒', s: '索' }

/** A tile as text: `5pr` → `赤5筒`, `E` → `東`. */
export function tileText(tile: string): string {
  if (tile === '?') return '？'
  if (HONORS[tile]) return HONORS[tile]
  const red = tile.endsWith('r')
  const t = red ? tile.slice(0, -1) : tile
  const suit = SUITS[t.slice(-1)]
  return suit ? `${red ? '赤' : ''}${t.slice(0, -1)}${suit}` : tile
}

/** The tiles of an action as text (the tile and, for a call, what it takes). */
export function actionTiles(a: ReviewAction): string {
  const parts: string[] = []
  if (a.pai) parts.push(tileText(a.pai))
  if (a.consumed?.length) parts.push(`(${a.consumed.map(tileText).join(' ')})`)
  return parts.join(' ')
}

/** A play the model rarely makes: ranked, a real choice, under `threshold`. */
export function lowSupport(d: ReviewDecision, threshold: number): boolean {
  return !d.forced && d.actual_prob != null && d.actual_prob < threshold
}

export type KindRow = {
  kind: ReviewKind
  /** Decisions with a real choice that the model ranked. */
  scored: number
  matched: number
  /** Mean probability the model gave what was played. */
  meanProb: number
  low: number
  forced: number
}

/** Agreement per kind of action, over `decisions` at `threshold`. Forced
 *  decisions (one legal action) and ones the model did not rank count toward
 *  neither agreement nor probability. */
export function kindRows(decisions: ReviewDecision[], threshold: number): KindRow[] {
  return REVIEW_KINDS.flatMap((kind) => {
    const of = decisions.filter((d) => d.actual.kind === kind)
    if (of.length === 0) return []
    const scored = of.filter((d) => !d.forced && d.actual_prob != null)
    const sum = scored.reduce((acc, d) => acc + (d.actual_prob ?? 0), 0)
    return [
      {
        kind,
        scored: scored.length,
        matched: scored.filter((d) => d.matched).length,
        meanProb: scored.length ? sum / scored.length : 0,
        low: scored.filter((d) => lowSupport(d, threshold)).length,
        forced: of.filter((d) => d.forced).length,
      },
    ]
  })
}

/** `part / whole` as "63.5%", a dash with nothing to divide. */
export function rate(part: number, whole: number): string {
  return whole > 0 ? `${((part * 100) / whole).toFixed(1)}%` : '—'
}

/** A probability as a whole-or-tenth percent: 0.0312 → "3.1%". */
export function pct(p: number | null | undefined): string {
  return p == null ? '—' : `${(p * 100).toFixed(p < 0.1 ? 1 : 0)}%`
}
