import { describe, expect, it } from 'vitest'

import { hexToRgba, pickShow, visibleItems } from './botShow'
import type { ShowItem, ShowMeta } from '@/types'

const row = (label: string, pais?: string[]): ShowItem => ({ label, pais, value: '50%' })

describe('pickShow', () => {
  it('returns the show block of a bot response meta', () => {
    const show = pickShow({ show: { title: 'Mokora', items: [row('Discard', ['1m'])] } })
    expect(show?.title).toBe('Mokora')
    expect(show?.items).toHaveLength(1)
  })

  it('rejects metas with nothing to render', () => {
    // A bare `none` reaction, a bot that emits no `show`, and a `show` with an
    // empty list all have to be indistinguishable from "no suggestion" —
    // otherwise the overlay would blank out its last real suggestion.
    expect(pickShow(undefined)).toBeNull()
    expect(pickShow(null)).toBeNull()
    expect(pickShow({})).toBeNull()
    expect(pickShow({ show: {} })).toBeNull()
    expect(pickShow({ show: { items: [] } })).toBeNull()
    expect(pickShow({ show: { items: 'nope' } })).toBeNull()
  })
})

describe('visibleItems', () => {
  const show: ShowMeta = {
    items: [
      row('Discard 1m', ['1m']),
      // A bot may emit a spacer/blank candidate. It draws as nothing.
      { value: '0%' },
      row('Discard 2m', ['2m']),
      row('Discard 3m', ['3m']),
      row('Discard 4m', ['4m']),
    ],
  }

  it('drops rows with nothing to draw', () => {
    expect(visibleItems(show).map((i) => i.label)).toEqual([
      'Discard 1m',
      'Discard 2m',
      'Discard 3m',
      'Discard 4m',
    ])
  })

  it('applies the top-N cap after filtering, not before', () => {
    // The overlay's "top 3" must mean three *visible* rows. Slicing first
    // would have spent one of the three slots on the empty candidate and
    // rendered only two.
    expect(visibleItems(show, 3).map((i) => i.label)).toEqual([
      'Discard 1m',
      'Discard 2m',
      'Discard 3m',
    ])
  })

  // The bot's "Pass" row (#190) carries a label and a probability but no tiles —
  // drawing the declined tile there would read as a discard suggestion. If the
  // empty-row filter dropped it, a declined call would render as *only* the call
  // it turned down, i.e. as a recommendation to make it.
  it('keeps a tile-less row that still has a label', () => {
    const declined: ShowMeta = {
      items: [
        { label: 'Pass', value: '87%' },
        { label: 'Pon', pais: ['2p', '2p'], value: '13%' },
      ],
    }
    expect(visibleItems(declined).map((i) => i.label)).toEqual(['Pass', 'Pon'])
  })

  it('is unbounded when no limit is given, and safe at the edges', () => {
    expect(visibleItems(show)).toHaveLength(4)
    expect(visibleItems(show, 99)).toHaveLength(4)
    expect(visibleItems(show, 0)).toHaveLength(0)
    expect(visibleItems(null, 3)).toHaveLength(0)
  })
})

describe('hexToRgba', () => {
  it('converts a six-digit hex, with or without the hash', () => {
    expect(hexToRgba('#aabbcc', 0.1)).toBe('rgba(170, 187, 204, 0.1)')
    expect(hexToRgba('AABBCC', 1)).toBe('rgba(170, 187, 204, 1)')
  })

  it('returns undefined for anything it cannot parse', () => {
    expect(hexToRgba(undefined, 0.1)).toBeUndefined()
    expect(hexToRgba('red', 0.1)).toBeUndefined()
    expect(hexToRgba('#abc', 0.1)).toBeUndefined()
  })
})
