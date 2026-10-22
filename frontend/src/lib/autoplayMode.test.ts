import { describe, expect, it } from 'vitest'

import { modeInForce, modeOf, withMode } from './autoplayMode'
import type { AppConfig } from '@/types'

const base = {
  autoplay: { enabled: false, next_game: { enabled: false, rest_minutes: 7 }, delay: {} },
} as unknown as AppConfig

describe('autoplay mode', () => {
  it('reads the level off the two switches', () => {
    expect(modeOf(base)).toBe('off')
    expect(modeOf(withMode(base, 'auto'))).toBe('auto')
    expect(modeOf(withMode(base, 'full'))).toBe('full')
  })

  it('without the paid feature a saved full auto is in force as auto', () => {
    const full = withMode(base, 'full')
    expect(modeInForce(full, true)).toBe('auto')
    expect(modeInForce(full, false)).toBe('full')
    expect(modeInForce(withMode(base, 'off'), true)).toBe('off')
  })

  it('switching back to off also turns the next-game switch off', () => {
    const off = withMode(withMode(base, 'full'), 'off')
    expect(off.autoplay.enabled).toBe(false)
    expect(off.autoplay.next_game.enabled).toBe(false)
  })

  it('leaves the other autoplay settings alone', () => {
    const full = withMode(base, 'full')
    expect(full.autoplay.next_game.rest_minutes).toBe(7)
    expect(full.autoplay.delay).toEqual({})
  })

  it('a next-game switch left on does not make a switched-off autoplay full', () => {
    const odd = {
      autoplay: { enabled: false, next_game: { enabled: true } },
    } as unknown as AppConfig
    expect(modeOf(odd)).toBe('off')
  })
})
