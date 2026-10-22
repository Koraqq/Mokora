import { describe, expect, it } from 'vitest'

import { aggregateStats, formatDuration, gameDurationMs } from './historyStats'
import type { GameRecord } from '@/types'

function record(id: string, startedAt: string, endedAt: string): GameRecord {
  return {
    id,
    started_at: startedAt,
    ended_at: endedAt,
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
    log_path: `games/${id}.mjai.jsonl`,
  }
}

describe('play time', () => {
  it('measures a game from start to end', () => {
    const r = record('g', '2026-10-09T05:57:53Z', '2026-10-09T06:20:04Z')
    expect(gameDurationMs(r)).toBe((22 * 60 + 11) * 1000)
  })

  it('skips a game whose stamps are unreadable or out of order', () => {
    expect(gameDurationMs(record('x', 'not a date', '2026-10-09T06:20:04Z'))).toBeNull()
    expect(gameDurationMs(record('y', '2026-10-09T06:20:04Z', '2026-10-09T05:57:53Z'))).toBeNull()
  })

  it('totals, averages and bounds over the timed games only', () => {
    const stats = aggregateStats([
      record('a', '2026-10-09T00:00:00Z', '2026-10-09T00:10:00Z'),
      record('b', '2026-10-09T01:00:00Z', '2026-10-09T01:30:00Z'),
      record('bad', 'not a date', '2026-10-09T02:00:00Z'),
    ])
    expect(stats.games).toBe(3)
    expect(stats.timed_games).toBe(2)
    expect(stats.duration_total_ms).toBe(40 * 60 * 1000)
    expect(stats.duration_min_ms).toBe(10 * 60 * 1000)
    expect(stats.duration_max_ms).toBe(30 * 60 * 1000)
    expect(aggregateStats([]).duration_min_ms).toBeNull()
  })

  it('pools round times over the games that measured them', () => {
    const withRounds = (r: GameRecord, count: number, total: number, min: number, max: number) => ({
      ...r,
      round_times: { count, total_ms: total, min_ms: min, max_ms: max },
    })
    const stats = aggregateStats([
      withRounds(record('a', '2026-10-09T00:00:00Z', '2026-10-09T00:10:00Z'), 10, 100_000, 4_000, 20_000),
      withRounds(record('b', '2026-10-09T01:00:00Z', '2026-10-09T01:10:00Z'), 30, 360_000, 3_500, 70_000),
      record('old', '2026-10-09T02:00:00Z', '2026-10-09T02:10:00Z'),
    ])
    expect(stats.round_count).toBe(40)
    expect(stats.round_total_ms).toBe(460_000)
    expect(stats.round_min_ms).toBe(3_500)
    expect(stats.round_max_ms).toBe(70_000)
    expect(aggregateStats([record('old', '2026-10-09T02:00:00Z', '2026-10-09T02:10:00Z')]).round_min_ms).toBeNull()
  })

  it('formats as m:ss under an hour and h:mm:ss over it', () => {
    expect(formatDuration(65_000)).toBe('1:05')
    expect(formatDuration((22 * 60 + 11) * 1000)).toBe('22:11')
    expect(formatDuration((49 * 3600 + 3 * 60 + 7) * 1000)).toBe('49:03:07')
  })
})
