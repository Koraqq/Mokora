import { describe, expect, it } from 'vitest'

import {
  computePt,
  majsoulDanOfLevel,
  type PtRule,
  withCurrentRank,
} from './ptCalc'
import type { GameRecord } from '@/types'

const rule: PtRule = { kind: 'majsoul', lobby: 'jade', dan: 'jakugou_1' }

function record(rank: 1 | 4, score = 25_000): GameRecord {
  return {
    id: 'g1',
    started_at: '2026-07-26T00:00:00Z',
    ended_at: '2026-07-26T01:00:00Z',
    platform: 'majsoul',
    num_players: 4,
    kyoku_mode: 'east_south',
    names: ['a', 'b', 'c', 'd'],
    our_seat: 0,
    final_scores: [score, 26_000, 25_000, 24_000],
    final_ranks: rank === 1 ? [1, 2, 3, 4] : [4, 1, 2, 3],
    our_rank: rank,
    our_delta: score - 25_000,
    stats: {} as GameRecord['stats'],
    log_path: 'games/test.mjai.jsonl',
  }
}

describe('Mahjong Soul PT', () => {
  it('applies the selected rank penalty to fourth place', () => {
    expect(computePt(record(4), rule)).toBe(-180)
  })

  it('rounds the platform PT result upward to an integer', () => {
    expect(computePt(record(1, 25_100), rule)).toBe(126)
  })
})

function played(
  r: GameRecord,
  startedAt: string,
  modeId: number,
  levelId?: number,
): GameRecord {
  return {
    ...r,
    started_at: startedAt,
    match_info: { platform: 'majsoul', mode_id: modeId, level_id: levelId },
  }
}

describe('Mahjong Soul current rank', () => {
  it('reads the dan out of a level id, 4p and 3p alike', () => {
    expect(majsoulDanOfLevel(10103)).toBe('shoshin_3')
    expect(majsoulDanOfLevel(20202)).toBe('jakushi_2')
    expect(majsoulDanOfLevel(10301)).toBe('jakketsu_1')
    expect(majsoulDanOfLevel(10701)).toBe('konten')
    expect(majsoulDanOfLevel(null)).toBeNull()
  })

  it('scores each game at its own room and rank', () => {
    const auto: PtRule = { ...rule, auto: true }
    // Silver hanchan (mode 5) at 雀士2: -15 uma, -40 dan penalty.
    expect(computePt(played(record(4), '2026-07-26T00:00:00Z', 5, 10202), auto)).toBe(-55)
    // A game without a recorded rank falls back to the rule's own.
    expect(computePt(played(record(4), '2026-07-26T00:00:00Z', 5), auto)).toBe(-180)
    // Switched off, the selected room / rank apply as before.
    expect(computePt(played(record(4), '2026-07-26T00:00:00Z', 5, 10202), rule)).toBe(-180)
  })

  it('takes the current room and rank from the newest game', () => {
    const auto: PtRule = { ...rule, auto: true }
    const games = [
      played(record(1), '2026-07-26T00:00:00Z', 5, 10202),
      played(record(1), '2026-07-27T00:00:00Z', 8, 10301),
      // Newest, but a melee table with no rank recorded: neither counts.
      played(record(1), '2026-07-28T00:00:00Z', 13),
    ]
    expect(withCurrentRank(auto, games)).toEqual({
      ...auto,
      lobby: 'gold',
      dan: 'jakketsu_1',
    })
    // Without `auto` the rule is left alone.
    expect(withCurrentRank(rule, games)).toBe(rule)
  })
})
