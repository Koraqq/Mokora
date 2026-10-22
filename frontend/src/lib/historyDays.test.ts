import { describe, expect, it } from 'vitest'

import { dayKey, gameDays, recordsOfDay } from './historyDays'
import type { GameRecord } from '@/types'

// Local-time timestamps, so the day buckets do not depend on the test
// machine's time zone.
function record(id: string, y: number, m: number, d: number, h: number): GameRecord {
  return {
    id,
    started_at: new Date(y, m - 1, d, h, 30).toISOString(),
    ended_at: new Date(y, m - 1, d, h, 45).toISOString(),
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
    match_info: null,
    log_path: `games/${id}.mjai.jsonl`,
  }
}

const games = [
  record('a', 2026, 10, 10, 1),
  record('b', 2026, 10, 9, 23),
  record('c', 2026, 10, 9, 0),
  record('d', 2026, 9, 30, 12),
  record('e', 2026, 10, 9, 15),
]

describe('gameDays', () => {
  it('pads the date and keeps local time', () => {
    expect(dayKey(new Date(2026, 0, 5, 23, 59).toISOString())).toBe('2026/01/05')
  })

  it('counts each day, newest first', () => {
    expect(gameDays(games)).toEqual([
      { day: '2026/10/10', count: 1 },
      { day: '2026/10/09', count: 3 },
      { day: '2026/09/30', count: 1 },
    ])
  })

  it('is empty without games', () => {
    expect(gameDays([])).toEqual([])
  })
})

describe('recordsOfDay', () => {
  it('keeps only that day', () => {
    expect(recordsOfDay(games, '2026/10/09').map((r) => r.id)).toEqual(['b', 'c', 'e'])
  })

  it('keeps everything without a day', () => {
    expect(recordsOfDay(games, null)).toBe(games)
  })
})
