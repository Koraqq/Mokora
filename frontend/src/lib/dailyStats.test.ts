import { describe, expect, it } from 'vitest'

import {
  clampDailyLimit,
  dailyStats,
  formatDuration,
  gamesTone,
  startOfDay,
  startOfNextDay,
  todaysGames,
  winRateTone,
} from './dailyStats'
import type { GameRecord } from '@/types'

function record(
  id: string,
  accountId: number,
  started: Date,
  minutes: number,
  rank: number,
  houjuu = 0,
): GameRecord {
  return {
    id,
    started_at: started.toISOString(),
    ended_at: new Date(started.getTime() + minutes * 60_000).toISOString(),
    platform: 'majsoul',
    num_players: 4,
    kyoku_mode: 'east_south',
    names: ['a', 'b', 'c', 'd'],
    our_seat: 0,
    final_scores: [25_000, 25_000, 25_000, 25_000],
    final_ranks: [1, 2, 3, 4],
    our_rank: rank,
    our_delta: 0,
    stats: { houjuu } as GameRecord['stats'],
    match_info: { platform: 'majsoul', account_id: accountId } as GameRecord['match_info'],
    log_path: `games/${id}.mjai.jsonl`,
  }
}

// Times are given in UTC+8, where the game day is counted.
const at = (h: number, day = 22) => new Date(Date.UTC(2026, 9, day, h - 8, 0))
const NOW = at(15)

describe('todaysGames', () => {
  it('counts only this account, from 05:00', () => {
    const records = [
      record('today', 1, at(5), 20, 1),
      record('before five', 1, at(4), 20, 1),
      record('other', 2, at(6), 20, 1),
    ]
    expect(todaysGames(records, 1, NOW).map((r) => r.id)).toEqual(['today'])
    expect(todaysGames(records, null, NOW)).toEqual([])
  })
})

describe('the game day', () => {
  it('starts at 05:00 UTC+8, the day before until five there', () => {
    expect(startOfDay(at(15))).toEqual(at(5))
    expect(startOfDay(at(5))).toEqual(at(5))
    expect(startOfDay(new Date(Date.UTC(2026, 9, 21, 20, 59)))).toEqual(at(5, 21))
    expect(startOfNextDay(at(15))).toEqual(at(5, 23))
    expect(startOfNextDay(at(2))).toEqual(at(5))
  })
})

describe('dailyStats', () => {
  it('sums the day', () => {
    const stats = dailyStats([
      record('a', 1, at(1), 20, 1, 2),
      record('b', 1, at(2), 65, 3, 1),
      record('c', 1, at(3), 30, 1),
      record('d', 1, at(4), 10, 2),
    ])
    expect(stats).toEqual({ games: 4, winRate: 50, dealIns: 3, longestMs: 65 * 60_000 })
  })

  it('has no rate or longest game without a game', () => {
    expect(dailyStats([])).toEqual({ games: 0, winRate: null, dealIns: 0, longestMs: null })
  })
})

describe('formatting', () => {
  it('clamps the limit to 0..=99', () => {
    expect(clampDailyLimit(-3)).toBe(0)
    expect(clampDailyLimit(150)).toBe(99)
    expect(clampDailyLimit(7.8)).toBe(7)
    expect(clampDailyLimit(Number.NaN)).toBe(0)
  })

  it('writes durations as m:ss or h:mm:ss', () => {
    expect(formatDuration(65_000)).toBe('1:05')
    expect(formatDuration(3_725_000)).toBe('1:02:05')
  })

  it('colours the game count past 10, 15 and 20', () => {
    expect([10, 11, 16, 21].map(gamesTone)).toEqual([
      '',
      'text-yellow-300',
      'text-orange-400',
      'text-red-500',
    ])
  })

  it('colours the win rate by band', () => {
    expect([0, 24, 25, 50, 75, 90, 100].map(winRateTone)).toEqual([
      'text-green-400',
      'text-green-400',
      'text-sky-400',
      'text-yellow-300',
      'text-orange-400',
      'text-red-500',
      'text-red-500',
    ])
  })
})
