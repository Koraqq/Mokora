import { describe, expect, it } from 'vitest'

import { RANKED_ONLY, recordsOfKinds } from './historyKinds'
import type { GameRecord, MatchInfo } from '@/types'

function record(id: string, match_info: MatchInfo | null): GameRecord {
  return {
    id,
    started_at: '2026-10-10T00:00:00Z',
    ended_at: '2026-10-10T00:10:00Z',
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
    match_info,
    log_path: `games/${id}.mjai.jsonl`,
  }
}

const games = [
  record('ranked', { platform: 'majsoul', mode_id: 19, category: 2 }),
  record('friend', { platform: 'majsoul', room_id: 92939, category: 1 }),
  record('event', { platform: 'majsoul', mode_id: 40, category: 2, rules: ['wanxiangxiuluo_mode'] }),
  record('old', null),
]
const ids = (rs: GameRecord[]) => rs.map((r) => r.id)

describe('recordsOfKinds', () => {
  it('counts ranked games only by default', () => {
    expect(ids(recordsOfKinds(games, RANKED_ONLY))).toEqual(['ranked', 'old'])
  })

  it('adds friendly rooms and events by their switches', () => {
    expect(ids(recordsOfKinds(games, { friendly: true, event: false }))).toEqual(['ranked', 'friend', 'old'])
    expect(ids(recordsOfKinds(games, { friendly: false, event: true }))).toEqual(['ranked', 'event', 'old'])
    expect(ids(recordsOfKinds(games, { friendly: true, event: true }))).toHaveLength(4)
  })
})
