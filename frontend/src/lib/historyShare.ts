// Players received from friends (the History page's "分享與匯入"), next to the
// ones this Mokora recorded. A received player is a set of games kept apart
// from the history (`history_shared_list`): it is listed in the picker under a
// key of its own, shows the same charts, and is in nobody's "all players".
//
// What the backend says when something fails is a short code (`signed_out`,
// `expired`, `wrong_key`, …); `shareErrorKey` turns one into its wording.

import type { HistoryPlayer } from '@/lib/historyPlayers'
import type { GameRecord, SharedSet } from '@/types'

const PREFIX = 'shared:'

/** The picker key of a received set. */
export const sharedKey = (id: string): string => `${PREFIX}${id}`

export const isSharedKey = (key: string): boolean => key.startsWith(PREFIX)

/** The most games one share holds (the newest of the player's). */
export const MAX_SHARED_GAMES = 500

/** The received sets as picker entries, newest import first. */
export function sharedPlayers(sets: SharedSet[]): HistoryPlayer[] {
  return sets.map((s) => ({
    key: sharedKey(s.id),
    name: s.name,
    games: s.records.length,
    lastPlayed: s.records.reduce((last, r) => (r.started_at > last ? r.started_at : last), ''),
    shared: true,
  }))
}

/** The games of the received set `key` stands for (none if it is gone). */
export function recordsOfShared(sets: SharedSet[], key: string): GameRecord[] {
  return sets.find((s) => sharedKey(s.id) === key)?.records ?? []
}

const KNOWN = new Set([
  'no_games',
  'no_name',
  'too_large',
  'too_many_shares',
  'rate_limited',
  'free_limit',
  'cooldown',
  'bad_response',
  'bad_id',
  'signed_out',
  'invalid_session',
  'locked',
  'blocked',
  'no_machine_id',
  'network',
  'not_found',
  'expired',
  'bad_code',
  'wrong_key',
  'bad_data',
  'newer_format',
  'lookup_lost',
  'unavailable',
])

/** The i18n key wording a failure of the share calls. */
export function shareErrorKey(error: unknown, making = false): string {
  const code = String(error).trim()
  // A share that was just being made cannot be "not found": the server does not
  // know the call (it answers the same way to any path it lacks).
  const known =
    code === 'invalid_session' ? 'signed_out' : making && code === 'not_found' ? 'unavailable' : code
  return `history.share.error.${KNOWN.has(known) ? known : 'unknown'}`
}
