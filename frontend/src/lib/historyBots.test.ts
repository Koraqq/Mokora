import { describe, expect, it } from 'vitest'

import { botFamilies, botFamily, botOf, recordsOfBot } from './historyBots'
import type { BotInfo, GameRecord } from '@/types'

function record(id: string, bot?: string | null): GameRecord {
  return {
    id,
    started_at: '2026-10-08T00:00:00Z',
    ended_at: '2026-10-08T00:00:00Z',
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
    bot,
    log_path: `games/${id}.mjai.jsonl`,
  }
}

describe('botOf', () => {
  it('names Mokora from the recorded bot', () => {
    expect(botOf(record('m', 'mokora-3p'))).toBe('mokora')
    expect(botOf(record('m4', 'mokora-4p'))).toBe('mokora')
    expect(botOf(record('r', 'mora-3p'))).toBe('mora')
    expect(botOf(record('r4', 'mora-4p'))).toBe('mora')
    expect(botOf(record('y', 'yakuman-3p'))).toBe('yakuman')
    expect(botOf(record('y4', 'yakuman-4p'))).toBe('yakuman')
  })

  it('reads records saved under the old names', () => {
    expect(botOf(record('m', 'mokora-native3p'))).toBe('mokora')
    expect(botOf(record('m4', 'mokora-native4p'))).toBe('mokora')
    expect(botOf(record('r', 'mora-native3p'))).toBe('mora')
  })

  it('counts the original bot, and records from before the field, as the base bot', () => {
    expect(botOf(record('a', 'base-3p'))).toBe('base')
    expect(botOf(record('a4', 'base-4p'))).toBe('base')
    expect(botOf(record('old'))).toBe('base')
    expect(botOf(record('null', null))).toBe('base')
  })
})

describe('recordsOfBot', () => {
  const games = [record('a'), record('m', 'mokora-3p'), record('b', 'base-3p')]

  it('keeps every game for the combined view', () => {
    expect(recordsOfBot(games, 'all').map((r) => r.id)).toEqual(['a', 'm', 'b'])
  })

  it('splits the games by bot', () => {
    expect(recordsOfBot(games, 'base').map((r) => r.id)).toEqual(['a', 'b'])
    expect(recordsOfBot(games, 'mokora').map((r) => r.id)).toEqual(['m'])
  })
})

function bot(name: string, display?: string): BotInfo {
  return {
    name,
    dir: name,
    has_pyproject: false,
    env_ready: true,
    manifest: display ? ({ bot: { display } } as BotInfo['manifest']) : undefined,
  }
}

describe('botFamily', () => {
  it('puts both player counts of a built-in bot together', () => {
    expect(botFamily('mora-3p')).toBe('mora')
    expect(botFamily('mora-4p')).toBe('mora')
    expect(botFamily('yakuman-native3p')).toBe('yakuman')
  })

  it('keeps any other bot as its own name, and no bot as base', () => {
    expect(botFamily('my-bot')).toBe('my-bot')
    expect(botFamily(null)).toBe('base')
  })
})

describe('botFamilies', () => {
  it('lists the installed families once each, by short name', () => {
    const families = botFamilies([
      bot('mora-3p'),
      bot('mora-4p'),
      bot('yakuman-4p'),
      bot('my-bot', 'My Bot'),
    ])
    expect(families).toEqual([
      { key: 'mora', label: 'Mora' },
      { key: 'yakuman', label: 'Yakuman' },
      { key: 'my-bot', label: 'My Bot' },
    ])
  })
})
