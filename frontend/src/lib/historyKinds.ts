// Which kinds of game a History card counts. Ranked games always; friendly
// rooms and events (the 比賽場) only when their switch is on — neither earns
// rank points, so by default they stay out of the rank, PT and stats views.

import { gameKind } from '@/lib/matchInfo'
import type { GameRecord } from '@/types'

export type KindView = { friendly: boolean; event: boolean }

export const RANKED_ONLY: KindView = { friendly: false, event: false }

/** The games a card with `view` counts. */
export function recordsOfKinds(records: GameRecord[], view: KindView): GameRecord[] {
  return records.filter((r) => {
    const kind = gameKind(r.match_info)
    return kind === 'ranked' || view[kind]
  })
}
