// The danger head's read of the table (`meta.danger` on the built-in bot's
// replies, see `bot::native::with_danger`): for every tile, how likely each
// opponent is to win on it. Only weights trained with a danger head send it.

import type { BotResponse, MjaiEvent } from '@/types'

export type DangerMeta = {
  /** mjai tiles, one per probability column. */
  tiles: string[]
  /** One row per opponent, shimocha first; probabilities 0..1. */
  opponents: number[][]
}

function isDanger(v: unknown): v is DangerMeta {
  if (typeof v !== 'object' || v == null) return false
  const d = v as Partial<DangerMeta>
  return (
    Array.isArray(d.tiles) &&
    Array.isArray(d.opponents) &&
    d.opponents.every((row) => Array.isArray(row) && row.length === d.tiles!.length)
  )
}

/** The newest reply that carries danger, if any. */
export function latestDanger(responses: readonly BotResponse[]): DangerMeta | null {
  for (let i = responses.length - 1; i >= 0; i--) {
    const d = responses[i].meta?.danger
    if (isDanger(d)) return d
  }
  return null
}

/** A store entry's place in arrival order (`notifyStore` tags both events
 *  and replies from one counter). */
type Sequenced = { _seq: number }

/**
 * The newest danger read of *this* round: one sent after the round's
 * `start_kyoku` — or, with that already gone from the event buffer, after the
 * oldest event still in it (the round began before every one of them). A read
 * from an earlier round is stale.
 */
export function currentDanger(
  responses: readonly (BotResponse & Sequenced)[],
  events: readonly (MjaiEvent & Sequenced)[],
): DangerMeta | null {
  let since = events[0]?._seq ?? 0
  for (let i = events.length - 1; i >= 0; i--) {
    if (events[i].type === 'start_kyoku') {
      since = events[i]._seq
      break
    }
  }
  return latestDanger(responses.filter((r) => r._seq > since))
}

/** Each tile's chance (in %) that dealing it is won on, keyed by its plain
 *  mjai name (no red-five `r`), the Risk tile's unit. */
export function dangerPercentByTile(d: DangerMeta): Map<string, number> {
  const all = combinedDanger(d)
  return new Map(d.tiles.map((t, i) => [t.replace(/r$/, ''), all[i] * 100]))
}

/** Chance that dealing each tile is won on by anyone: 1 − Π(1 − pᵢ). */
export function combinedDanger(d: DangerMeta): number[] {
  return d.tiles.map((_, x) => 1 - d.opponents.reduce((safe, row) => safe * (1 - row[x]), 1))
}

/** The panel's rows: in sanma the terminals-and-honours row (1m 9m and the
 *  winds and dragons), then pinzu, then souzu; in yonma one row per suit.
 *  Each entry is an index into `tiles`. */
export function dangerRows(tiles: string[]): number[][] {
  const of = (suits: string[]) =>
    tiles.map((t, i) => [t, i] as const).filter(([t]) => suits.includes(suitOf(t))).map(([, i]) => i)
  const sanma = of(['m']).length <= 2
  const rows = sanma ? [of(['m', 'z']), of(['p']), of(['s'])] : [of(['m']), of(['p']), of(['s']), of(['z'])]
  return rows.filter((r) => r.length > 0)
}

/** A tile's suit: `m` / `p` / `s`, or `z` for the honours. */
export function suitOf(tile: string): 'm' | 'p' | 's' | 'z' {
  const t = tile.replace(/r$/, '')
  return /^[1-9][mps]$/.test(t) ? (t[1] as 'm' | 'p' | 's') : 'z'
}
