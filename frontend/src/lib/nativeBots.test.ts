import { describe, expect, it } from 'vitest'

import {
  botInForce,
  MOKORA_3P,
  MOKORA_4P,
  MORA_3P,
  MORA_4P,
  NATIVE_3P,
  NATIVE_4P,
  YAKUMAN_3P,
  YAKUMAN_4P,
  canonicalBotName,
  isNativeBot,
  nativeBotShortName,
  nativeModes,
} from './nativeBots'

describe('nativeBotShortName', () => {
  it('names each built-in bot, and no other', () => {
    expect([MORA_3P, MORA_4P].map(nativeBotShortName)).toEqual(['Mora', 'Mora'])
    expect([MOKORA_3P, MOKORA_4P].map(nativeBotShortName)).toEqual(['Mokora', 'Mokora'])
    expect([NATIVE_3P, NATIVE_4P].map(nativeBotShortName)).toEqual(['Base', 'Base'])
    expect([YAKUMAN_3P, YAKUMAN_4P].map(nativeBotShortName)).toEqual(['Yakuman', 'Yakuman'])
    expect(nativeBotShortName('my-bot')).toBeNull()
  })

  it('plays each built-in bot at its own player count', () => {
    expect([MORA_3P, MORA_4P, YAKUMAN_3P, YAKUMAN_4P].map(nativeModes)).toEqual([
      ['3p'],
      ['4p'],
      ['3p'],
      ['4p'],
    ])
    expect([YAKUMAN_3P, YAKUMAN_4P].every(isNativeBot)).toBe(true)
  })

  it('knows the bots by their old names too', () => {
    expect(canonicalBotName('mora-native3p')).toBe(MORA_3P)
    expect(canonicalBotName('my-bot')).toBe('my-bot')
    expect(nativeBotShortName('mokora-native4p')).toBe('Mokora')
    expect(isNativeBot('mora-native3p')).toBe(true)
    // Retired bots are still named for old records, but are no longer bots.
    expect(isNativeBot('mokora-native3p')).toBe(false)
  })
})

describe('Yakuman without the paid feature', () => {
  it('is played by its side of Mora; everything else is itself', () => {
    expect(botInForce('yakuman-3p', false)).toBe('mora-3p')
    expect(botInForce('yakuman-native3p', false)).toBe('mora-3p')
    expect(botInForce('yakuman-4p', false)).toBe('mora-4p')
    expect(botInForce('yakuman-3p', true)).toBe('yakuman-3p')
    expect(botInForce('mora-4p', false)).toBe('mora-4p')
    expect(botInForce('my-python-bot', false)).toBe('my-python-bot')
  })
})
