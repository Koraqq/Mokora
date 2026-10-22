import { describe, expect, it } from 'vitest'

import { isBigWin, levelParts, playersOfMode, recentGames, share, trendOf } from './majsoulProfile'
import type { GameRecord } from '@/types'

function record(id: string, at: string, accountId?: number): GameRecord {
  return {
    id,
    started_at: at,
    ended_at: at,
    platform: 'majsoul',
    num_players: 3,
    kyoku_mode: 'east_only',
    names: ['a', 'b', 'c'],
    our_seat: 0,
    final_scores: [35_000, 35_000, 35_000],
    final_ranks: [1, 2, 3],
    our_rank: 1,
    our_delta: 0,
    stats: {} as GameRecord['stats'],
    match_info:
      accountId === undefined
        ? undefined
        : ({ platform: 'majsoul', account_id: accountId } as GameRecord['match_info']),
    log_path: `games/${id}.mjai.jsonl`,
  }
}

describe('levelParts', () => {
  it('reads tier and star off a level id', () => {
    expect(levelParts(20201)).toEqual({ tier: 2, star: 1 })
    expect(levelParts(10103)).toEqual({ tier: 1, star: 3 })
  })
})

describe('recentGames', () => {
  it('keeps this player and older records, newest first', () => {
    const games = [
      record('a', '2026-10-01T00:00:00Z', 1),
      record('b', '2026-10-03T00:00:00Z', 2),
      record('c', '2026-10-02T00:00:00Z'),
      record('d', '2026-10-04T00:00:00Z', 1),
    ]
    expect(recentGames(games, 1, 10).map((r) => r.id)).toEqual(['d', 'c', 'a'])
    expect(recentGames(games, 1, 2).map((r) => r.id)).toEqual(['d', 'c'])
  })
})

describe('trendOf', () => {
  const g = (end: number, mode: number, rank: number, ranked = true, points = 0) => ({
    uuid: String(end), mode_id: mode, ranked, start_time: end - 1, end_time: end, rank, final_point: points,
  })

  it("is the ladder's last ranked places, oldest first", () => {
    // Newest first, as the game sends it; a 4-player game and an unranked one mixed in.
    const games = [g(9, 19, 1, true, 82400), g(8, 2, 4), g(7, 19, 2), g(6, 12, 3, false), g(5, 21, 3)]
    expect(trendOf(games, 3, 10).map((x) => x.rank)).toEqual([3, 2, 1])
    expect(trendOf(games, 3, 10).map((x) => x.big)).toEqual([false, false, true])
    expect(trendOf(games, 3, 2).map((x) => x.rank)).toEqual([2, 1])
    expect(trendOf(games, 4, 10).map((x) => x.rank)).toEqual([4])
    expect(playersOfMode(19)).toBe(3)
    expect(playersOfMode(12)).toBe(4)
  })
})

describe('isBigWin', () => {
  it('is a first on more than twice the starting points', () => {
    const g = (mode: number, rank: number, points: number) => ({
      uuid: 'x', mode_id: mode, ranked: true, start_time: 0, end_time: 0, rank, final_point: points,
    })
    // As logged: 71,400 marked, exactly 70,000 not (3-player, 35,000 start).
    expect(isBigWin(g(19, 1, 71400))).toBe(true)
    expect(isBigWin(g(19, 1, 70000))).toBe(false)
    expect(isBigWin(g(19, 2, 80000))).toBe(false)
    expect(isBigWin(g(12, 1, 50100))).toBe(true)
  })
})

describe('share', () => {
  it('is a percentage, or a dash of nothing', () => {
    expect(share(1, 4)).toBe('25.0%')
    expect(share(1, 0)).toBe('—')
  })
})
