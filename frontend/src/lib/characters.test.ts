import { describe, expect, it } from 'vitest'

import { characterIconUrl } from './characters'

describe('characterIconUrl', () => {
  it('names the icon file by the character id', () => {
    expect(characterIconUrl(200001)).toBe('/characters/200001.png')
    expect(characterIconUrl(20000123)).toBe('/characters/20000123.png')
  })
})
