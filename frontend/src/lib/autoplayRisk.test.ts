import { beforeEach, describe, expect, it, vi } from 'vitest'

let notice: 'signed_out' | 'acked' | 'open' = 'signed_out'
const calls: string[] = []
vi.mock('@/lib/tauri', () => ({
  HAS_TAURI: true,
  invoke: (cmd: string) => {
    calls.push(cmd)
    return Promise.resolve(cmd === 'autoplay_notice' ? notice : undefined)
  },
  listen: () => Promise.resolve(() => {}),
}))

import {
  acknowledgeAutoplayRisk,
  autoplayRiskAcknowledged,
  forgetAutoplayRisk,
  syncAutoplayRisk,
} from './autoplayRisk'

describe('autoplay risk notice', () => {
  beforeEach(() => {
    localStorage.clear()
    calls.length = 0
    notice = 'signed_out'
  })

  it('is taken from the membership when another account already accepted it', async () => {
    notice = 'acked'
    expect(autoplayRiskAcknowledged()).toBe(false)
    await syncAutoplayRisk()
    expect(autoplayRiskAcknowledged()).toBe(true)
  })

  it('stays unasked-for-yet while the membership has not seen it', async () => {
    notice = 'open'
    await syncAutoplayRisk()
    expect(autoplayRiskAcknowledged()).toBe(false)
    expect(calls).not.toContain('accept_autoplay_notice')
  })

  it('hands an older answer given in this window up to the membership', async () => {
    localStorage.setItem('mokora.autoplayRiskAck', '1')
    notice = 'open'
    await syncAutoplayRisk()
    expect(calls).toContain('accept_autoplay_notice')
  })

  it('leaves a window without a membership as it is', async () => {
    localStorage.setItem('mokora.autoplayRiskAck', '1')
    await syncAutoplayRisk()
    expect(autoplayRiskAcknowledged()).toBe(true)
    expect(calls).toEqual(['autoplay_notice'])
  })

  it('tells the membership when it is accepted, and is asked again once forgotten', () => {
    acknowledgeAutoplayRisk()
    expect(autoplayRiskAcknowledged()).toBe(true)
    expect(calls).toContain('accept_autoplay_notice')
    forgetAutoplayRisk()
    expect(autoplayRiskAcknowledged()).toBe(false)
  })
})
