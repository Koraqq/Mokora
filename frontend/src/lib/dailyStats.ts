// Today's games of the account a window shows, for the play screen's
// "today" panel and the daily game limit (`autoplay.next_game.
// max_games_per_day`, enforced by the backend's `history::daily`).
//
// A game is the account's when the game itself recorded the account's id
// (`match_info.account_id`, Mahjong Soul); today is the game day, which
// starts at 05:00 UTC+8, when Mahjong Soul resets its dailies, wherever the
// player is. The backend's `history::daily` uses the same moment.

import type { GameRecord } from '@/types'

/** The highest daily game limit that can be set (the backend's too). */
export const MAX_GAMES_PER_DAY = 99

/** A typed-in daily limit as it is kept: a whole number in 0..=99. */
export function clampDailyLimit(n: number): number {
  if (!Number.isFinite(n)) return 0
  return Math.min(MAX_GAMES_PER_DAY, Math.max(0, Math.floor(n)))
}

/** The Mahjong Soul account a record was played by. */
export function recordAccount(r: GameRecord): number | null {
  const info = r.match_info
  return info?.platform === 'majsoul' && info.account_id != null ? info.account_id : null
}

/** A game day starts at 05:00 UTC+8 (Mahjong Soul's daily reset), which is
 *  21:00 UTC the day before. */
export const DAY_START_UTC_HOUR = 21
const DAY_MS = 24 * 60 * 60 * 1000

/** When the game day `now` is in began: the last 05:00 UTC+8 at or before
 *  `now`. */
export function startOfDay(now: Date): Date {
  const start = Date.UTC(
    now.getUTCFullYear(),
    now.getUTCMonth(),
    now.getUTCDate(),
    DAY_START_UTC_HOUR,
  )
  return new Date(start > now.getTime() ? start - DAY_MS : start)
}

/** When the game day after the one `now` is in begins. */
export function startOfNextDay(now: Date): Date {
  return new Date(startOfDay(now).getTime() + DAY_MS)
}

/** The games account `accountId` started in the game day `now` is in. */
export function todaysGames(
  records: GameRecord[],
  accountId: number | null | undefined,
  now: Date,
): GameRecord[] {
  if (accountId == null) return []
  const since = startOfDay(now).getTime()
  return records.filter(
    (r) => recordAccount(r) === accountId && new Date(r.started_at).getTime() >= since,
  )
}

export type DailyStats = {
  games: number
  /** Share of the games finished first, percent; null without a game. */
  winRate: number | null
  /** Deal-ins (放銃) over the games. */
  dealIns: number
  /** The longest game, start to end, ms; null without a game. */
  longestMs: number | null
}

export function dailyStats(games: GameRecord[]): DailyStats {
  if (games.length === 0) return { games: 0, winRate: null, dealIns: 0, longestMs: null }
  const firsts = games.filter((r) => r.our_rank === 1).length
  const dealIns = games.reduce((n, r) => n + (r.stats?.houjuu ?? 0), 0)
  const longestMs = Math.max(
    ...games.map((r) => new Date(r.ended_at).getTime() - new Date(r.started_at).getTime()),
  )
  return {
    games: games.length,
    winRate: (firsts / games.length) * 100,
    dealIns,
    longestMs: Math.max(0, longestMs),
  }
}

/** `m:ss`, or `h:mm:ss` from an hour. */
export function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = String(total % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

/** A warning colour for how many games were played today: none up to ten,
 *  then bright yellow, orange from 16 and red from 21. */
export function gamesTone(games: number): string {
  if (games > 20) return 'text-red-500'
  if (games > 15) return 'text-orange-400'
  if (games > 10) return 'text-yellow-300'
  return ''
}

/** The colour of a win rate (percent): red from 90, orange from 75, yellow
 *  from 50, blue from 25, green below. */
export function winRateTone(rate: number): string {
  if (rate >= 90) return 'text-red-500'
  if (rate >= 75) return 'text-orange-400'
  if (rate >= 50) return 'text-yellow-300'
  if (rate >= 25) return 'text-sky-400'
  return 'text-green-400'
}
