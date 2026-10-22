import { describe, expect, it } from 'vitest'
import { countChangedSettings } from './configDiff'

describe('countChangedSettings', () => {
  const saved = {
    general: { first_run_completed: true },
    bot: { enabled: true, active_3p: 'mora-3p', active_4p: 'mokora-4p' },
    network: { hosts: ['a', 'b'] },
  }

  it('is zero for an unchanged draft', () => {
    expect(countChangedSettings(structuredClone(saved), saved)).toBe(0)
  })

  it('counts each changed leaf once', () => {
    const draft = structuredClone(saved)
    draft.general.first_run_completed = false
    draft.bot.active_3p = 'yakuman-3p'
    expect(countChangedSettings(draft, saved)).toBe(2)
  })

  it('counts a changed list as one setting', () => {
    const draft = structuredClone(saved)
    draft.network.hosts = ['a', 'c', 'd']
    expect(countChangedSettings(draft, saved)).toBe(1)
  })
})
