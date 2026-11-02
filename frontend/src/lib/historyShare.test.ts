import { describe, expect, it } from 'vitest'

import { isSharedKey, recordsOfShared, sharedKey, sharedPlayers, shareErrorKey } from './historyShare'
import type { GameRecord, SharedSet } from '@/types'

function record(id: string, startedAt: string): GameRecord {
  return {
    id,
    started_at: startedAt,
    ended_at: startedAt,
    platform: 'majsoul',
    num_players: 4,
    kyoku_mode: 'east_south',
    names: ['Friend', '#2', '#3', '#4'],
    our_seat: 0,
    final_scores: [30_000, 25_000, 25_000, 20_000],
    final_ranks: [1, 2, 3, 4],
    our_rank: 1,
    our_delta: 5_000,
    stats: {} as GameRecord['stats'],
    match_info: null,
    log_path: '',
  }
}

const set = (id: string, name: string, records: GameRecord[]): SharedSet => ({
  id,
  name,
  source: id,
  imported_at: 1,
  records,
})

describe('received players', () => {
  it('get picker keys that no recorded player can have', () => {
    expect(sharedKey('AB12CD34')).toBe('shared:AB12CD34')
    expect(isSharedKey(sharedKey('AB12CD34'))).toBe(true)
    expect(isSharedKey('majsoul:7')).toBe(false)
  })

  it('are listed with their game count and last game', () => {
    const players = sharedPlayers([
      set('AAAAAAAA', 'Friend', [
        record('a', '2026-10-01T00:00:00Z'),
        record('b', '2026-10-03T00:00:00Z'),
      ]),
    ])
    expect(players).toEqual([
      {
        key: 'shared:AAAAAAAA',
        name: 'Friend',
        games: 2,
        lastPlayed: '2026-10-03T00:00:00Z',
        shared: true,
      },
    ])
  })

  it('give their own games, and nothing for a set that is gone', () => {
    const sets = [set('AAAAAAAA', 'Friend', [record('a', '2026-10-01T00:00:00Z')])]
    expect(recordsOfShared(sets, 'shared:AAAAAAAA').map((r) => r.id)).toEqual(['a'])
    expect(recordsOfShared(sets, 'shared:BBBBBBBB')).toEqual([])
  })
})

describe('shareErrorKey', () => {
  it('words the codes the backend sends', () => {
    expect(shareErrorKey('expired')).toBe('history.share.error.expired')
    expect(shareErrorKey(' wrong_key\n')).toBe('history.share.error.wrong_key')
    expect(shareErrorKey('network')).toBe('history.share.error.network')
  })

  it('reads "not found" while making a share as the server not offering it', () => {
    expect(shareErrorKey('not_found', true)).toBe('history.share.error.unavailable')
    expect(shareErrorKey('not_found')).toBe('history.share.error.not_found')
  })

  it('reads a lost session as being signed out, and an unknown code as unknown', () => {
    expect(shareErrorKey('invalid_session')).toBe('history.share.error.signed_out')
    expect(shareErrorKey('something odd')).toBe('history.share.error.unknown')
    expect(shareErrorKey(new Error('x'))).toBe('history.share.error.unknown')
  })
})
