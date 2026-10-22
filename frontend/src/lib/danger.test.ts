import { describe, expect, it } from 'vitest'

import type { BotResponse } from '@/types'

import {
  combinedDanger,
  currentDanger,
  dangerPercentByTile,
  dangerRows,
  latestDanger,
  suitOf,
} from './danger'
import type { MjaiEvent } from '@/types'

const reply = (meta?: Record<string, unknown>): BotResponse =>
  ({ type: 'none', meta }) as BotResponse

describe('latestDanger', () => {
  it('is the newest reply that carries it', () => {
    const a = { tiles: ['1m'], opponents: [[0.1], [0.2]] }
    const b = { tiles: ['1m'], opponents: [[0.3], [0.4]] }
    expect(latestDanger([reply({ danger: a }), reply({ danger: b }), reply()])).toEqual(b)
  })

  it('skips malformed ones and is null without any', () => {
    expect(latestDanger([reply({ danger: { tiles: ['1m'], opponents: [[0.1, 0.2]] } })])).toBeNull()
    expect(latestDanger([reply({ show: 1 }), reply()])).toBeNull()
  })
})

describe('currentDanger', () => {
  const read = (p: number) => ({ tiles: ['1m'], opponents: [[p], [0]] })
  const at = <T,>(v: T, _seq: number) => ({ ...v, _seq })
  const ev = (type: string, _seq: number) => ({ type, _seq }) as unknown as MjaiEvent & { _seq: number }

  it('takes only reads sent since this round began', () => {
    const responses = [at(reply({ danger: read(0.1) }), 2), at(reply({ danger: read(0.2) }), 6)]
    expect(currentDanger(responses, [ev('start_kyoku', 1), ev('dahai', 3)])).toEqual(read(0.2))
    expect(currentDanger(responses, [ev('start_kyoku', 7)])).toBeNull()
  })

  it('counts from the oldest event kept when the round start is gone', () => {
    const responses = [at(reply({ danger: read(0.1) }), 2), at(reply({ danger: read(0.2) }), 9)]
    expect(currentDanger(responses, [ev('dahai', 5), ev('dahai', 8)])).toEqual(read(0.2))
    expect(currentDanger([responses[0]], [ev('dahai', 5)])).toBeNull()
  })
})

describe('dangerPercentByTile', () => {
  it('is the combined chance in %, by plain tile', () => {
    const m = dangerPercentByTile({ tiles: ['5pr', 'E'], opponents: [[0.1, 0], [0.1, 0.5]] })
    expect(m.get('5p')).toBeCloseTo(19)
    expect(m.get('E')).toBeCloseTo(50)
  })
})

describe('combinedDanger', () => {
  it('is the chance anyone wins on the tile', () => {
    const d = { tiles: ['1m', '9m', 'E'], opponents: [[0.5, 0, 0.1], [0.5, 1, 0.1]] }
    const c = combinedDanger(d)
    expect(c[0]).toBeCloseTo(0.75)
    expect(c[1]).toBeCloseTo(1)
    expect(c[2]).toBeCloseTo(0.19)
  })
})

describe('dangerRows', () => {
  const sanma = ['1m', '9m', '1p', '2p', '1s', '2s', 'E', 'C']
  it('puts sanma terminals and honours on one row, then pinzu, then souzu', () => {
    const rows = dangerRows(sanma).map((r) => r.map((i) => sanma[i]))
    expect(rows).toEqual([['1m', '9m', 'E', 'C'], ['1p', '2p'], ['1s', '2s']])
  })

  it('gives yonma one row per suit', () => {
    const yonma = ['1m', '2m', '3m', '1p', '1s', 'E']
    expect(dangerRows(yonma).map((r) => r.map((i) => yonma[i]))).toEqual([
      ['1m', '2m', '3m'],
      ['1p'],
      ['1s'],
      ['E'],
    ])
  })
})

describe('suitOf', () => {
  it('reads the suit, red fives included, honours as z', () => {
    expect(suitOf('5pr')).toBe('p')
    expect(suitOf('1m')).toBe('m')
    expect(suitOf('E')).toBe('z')
    expect(suitOf('C')).toBe('z')
  })
})
