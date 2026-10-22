// Words and numbers for the 個人資料 page: Mahjong Soul level ids as rank
// names, the yaku ids of the game's own statistics, its recent games as a
// trend, and the player's recent games out of the history.

import type { GameRecord, MajsoulRecentGame } from '@/types'

/** A level id read as its tier (1 初心 … 6 魂天) and star (or level within
 *  魂天): `<ladder><tier><n>`, e.g. `20201` = 3-player 雀士 1. */
export function levelParts(id: number): { tier: number; star: number } {
  return { tier: Math.floor(id / 100) % 100, star: id % 100 }
}

/** Mahjong Soul's yaku ids of the yakuman (35 天和 … 50 大四喜). */
export const YAKUMAN_IDS = Array.from({ length: 16 }, (_, i) => 35 + i)

/** The other yaku, in the game's own order (1 門前清自摸和 … 34 拔北寶牌). */
export const YAKU_IDS = Array.from({ length: 34 }, (_, i) => 1 + i)

/** Players at a ranked table by its mode id: 17 and up are the 3-player
 *  tables (see `matchInfo`'s tier table), below that 4-player. */
export function playersOfMode(modeId: number): 3 | 4 {
  return modeId >= 17 ? 3 : 4
}

/** Points every seat starts a game with. */
const START_POINTS: Record<3 | 4, number> = { 3: 35000, 4: 25000 }

/** A first place that ended on more than twice the starting points — the
 *  game marks these on its 最近走勢 (as logged: every marked game is a first
 *  on 71,400 or more, an unmarked first at exactly 70,000 among them). */
export function isBigWin(game: MajsoulRecentGame): boolean {
  return game.rank === 1 && game.final_point > 2 * START_POINTS[playersOfMode(game.mode_id)]
}

/** The last `n` ranked games of a ladder, oldest first — the game's
 *  最近走勢: each one's place, and whether it was a big win. `games` is
 *  newest first, as the game sends it. */
export function trendOf(
  games: MajsoulRecentGame[],
  players: 3 | 4,
  n: number,
): { rank: number; big: boolean }[] {
  return games
    .filter((g) => g.ranked && playersOfMode(g.mode_id) === players)
    .slice(0, n)
    .map((g) => ({ rank: g.rank, big: isBigWin(g) }))
    .reverse()
}

/** Han of 13 or more is a yakuman (or a counted one). */
export function isYakumanHan(han: number | null | undefined): boolean {
  return (han ?? 0) >= 13
}

/** The newest `n` games of the player `accountId` (records from before the
 *  account was stored count for every player, as the History page does). */
export function recentGames(
  records: GameRecord[],
  accountId: number | null | undefined,
  n: number,
): GameRecord[] {
  const mine = records.filter((r) => {
    const info = r.match_info
    const id = info && info.platform === 'majsoul' ? info.account_id : null
    return accountId == null || id == null || id === accountId
  })
  return [...mine]
    .sort((a, b) => (a.started_at < b.started_at ? 1 : a.started_at > b.started_at ? -1 : 0))
    .slice(0, n)
}

/** Share of `part` in `whole`, as "12.3%"; a dash with nothing to divide. */
export function share(part: number, whole: number): string {
  return whole > 0 ? `${((part * 100) / whole).toFixed(1)}%` : '—'
}
