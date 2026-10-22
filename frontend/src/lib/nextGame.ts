// Words for the next-game flow's status (resting, queuing) on the Game page's
// bot-action tile: the game it is about to queue for, and the rest left.

import type { NextGameTarget } from '@/types'

type T = (key: string, opts?: Record<string, unknown>) => string

/** "金之間・三人東" — the room (or "by rank" before the rank is known) and the
 *  table, or the ladder when the table follows the player counts. */
export function nextGameTargetLabel(target: NextGameTarget, t: T): string {
  const room = target.room
    ? t(`settings.autoplay.next_game_room_${target.room}`)
    : t('tile.next_game.room_by_rank')
  const table = target.rule
    ? t(`settings.autoplay.next_game_rule_${target.rule}`)
    : t(`tile.next_game.ladder_${target.players ?? 4}`)
  return `${room}・${table}`
}

/** `m:ss` of the time left until `untilMs` (never negative). */
export function restLeft(untilMs: number, nowMs: number): string {
  const secs = Math.max(0, Math.ceil((untilMs - nowMs) / 1000))
  return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, '0')}`
}
