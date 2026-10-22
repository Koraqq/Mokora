import { describe, expect, it } from 'vitest'

import { ALL_PLAYERS, listPlayers, playerKeys, recordsOfPlayer } from './historyPlayers'
import type { GameRecord } from '@/types'

function record(
  id: string,
  startedAt: string,
  name: string,
  accountId?: number,
): GameRecord {
  return {
    id,
    started_at: startedAt,
    ended_at: startedAt,
    platform: 'majsoul',
    num_players: 3,
    kyoku_mode: 'east_only',
    names: ['x', name, 'y'],
    our_seat: 1,
    final_scores: [35_000, 35_000, 35_000],
    final_ranks: [1, 2, 3],
    our_rank: 2,
    our_delta: 0,
    stats: {} as GameRecord['stats'],
    match_info:
      accountId == null
        ? { platform: 'majsoul', mode_id: 19 }
        : { platform: 'majsoul', mode_id: 19, account_id: accountId },
    log_path: `games/${id}.mjai.jsonl`,
  }
}

describe('playerKeys', () => {
  it('keys a game by its account id when it has one', () => {
    const keys = playerKeys([record('a', '2026-10-08T00:00:00Z', 'K_o', 7)])
    expect(keys.get('a')).toBe('majsoul:7')
  })

  it('folds games from before the id was stored into the account that used the nickname', () => {
    const keys = playerKeys([
      record('old', '2026-10-07T00:00:00Z', 'K_o'),
      record('new', '2026-10-08T00:00:00Z', 'K_o', 7),
    ])
    expect(keys.get('old')).toBe('majsoul:7')
  })

  it('keeps two accounts apart even when they share a nickname', () => {
    const keys = playerKeys([
      record('a', '2026-10-08T00:00:00Z', 'same', 7),
      record('b', '2026-10-08T01:00:00Z', 'same', 8),
    ])
    expect(keys.get('a')).not.toBe(keys.get('b'))
  })

  it('falls back to the nickname when no account was ever seen', () => {
    const keys = playerKeys([record('a', '2026-10-07T00:00:00Z', 'K_o')])
    expect(keys.get('a')).toBe('majsoul:name:K_o')
  })

  it('leaves observer games without a player', () => {
    const watched = { ...record('w', '2026-10-07T00:00:00Z', 'K_o'), our_seat: null }
    expect(playerKeys([watched]).has('w')).toBe(false)
  })
})

describe('recordsOfPlayer', () => {
  const games = [
    record('a', '2026-10-07T00:00:00Z', 'K_o', 7),
    record('b', '2026-10-08T00:00:00Z', 'alt', 8),
    { ...record('w', '2026-10-08T01:00:00Z', 'K_o'), our_seat: null },
  ]
  const keys = playerKeys(games)

  it('keeps one player apart from the others', () => {
    expect(recordsOfPlayer(games, keys, 'majsoul:7').map((r) => r.id)).toEqual(['a'])
  })

  it('counts every game together for all players', () => {
    expect(recordsOfPlayer(games, keys, ALL_PLAYERS).map((r) => r.id)).toEqual([
      'a',
      'b',
      'w',
    ])
  })
})

describe('listPlayers', () => {
  it('lists the most recent player first, under their latest nickname', () => {
    const players = listPlayers([
      record('a', '2026-10-07T00:00:00Z', 'old name', 7),
      record('b', '2026-10-08T00:00:00Z', 'new name', 7),
      record('c', '2026-10-07T12:00:00Z', 'alt', 8),
    ])
    expect(players.map((p) => [p.key, p.name, p.games])).toEqual([
      ['majsoul:7', 'new name', 2],
      ['majsoul:8', 'alt', 1],
    ])
  })
})
