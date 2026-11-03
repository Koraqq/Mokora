import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook } from '@testing-library/react'

const fetchAccounts = vi.fn(() => Promise.resolve([]))
vi.mock('@/lib/accounts', () => ({ fetchAccounts: () => fetchAccounts() }))
const listen = vi.fn(() => Promise.resolve(() => {}))
vi.mock('@/lib/tauri', () => ({ listen: (...a: unknown[]) => listen(...(a as [])) }))

import { REFRESH_MS, useAccounts } from './useAccounts'

describe('useAccounts', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    fetchAccounts.mockClear()
    listen.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('polls once however many components use it, and stops with the last', () => {
    const a = renderHook(() => useAccounts())
    const b = renderHook(() => useAccounts())
    const c = renderHook(() => useAccounts())
    expect(fetchAccounts).toHaveBeenCalledTimes(1)
    expect(listen).toHaveBeenCalledTimes(1)

    fetchAccounts.mockClear()
    vi.advanceTimersByTime(REFRESH_MS)
    expect(fetchAccounts).toHaveBeenCalledTimes(1)

    a.unmount()
    b.unmount()
    fetchAccounts.mockClear()
    vi.advanceTimersByTime(REFRESH_MS)
    expect(fetchAccounts).toHaveBeenCalledTimes(1)

    c.unmount()
    fetchAccounts.mockClear()
    vi.advanceTimersByTime(REFRESH_MS * 3)
    expect(fetchAccounts).not.toHaveBeenCalled()
  })

  it('starts again when something mounts after everything left', () => {
    renderHook(() => useAccounts()).unmount()
    fetchAccounts.mockClear()
    const again = renderHook(() => useAccounts())
    expect(fetchAccounts).toHaveBeenCalledTimes(1)
    again.unmount()
  })

  it('does not poll while its interface is parked, and catches up when shown', () => {
    const w = window as { __MOKORA_ACTIVE__?: boolean }
    w.__MOKORA_ACTIVE__ = false
    const hook = renderHook(() => useAccounts())
    fetchAccounts.mockClear()
    vi.advanceTimersByTime(REFRESH_MS * 3)
    expect(fetchAccounts).not.toHaveBeenCalled()

    w.__MOKORA_ACTIVE__ = true
    window.dispatchEvent(new CustomEvent('mokora-active', { detail: true }))
    expect(fetchAccounts).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(REFRESH_MS)
    expect(fetchAccounts).toHaveBeenCalledTimes(2)
    hook.unmount()
    delete w.__MOKORA_ACTIVE__
  })

  it('keeps the list it has when a read says the same', async () => {
    const account = (online: boolean) => ({
      id: '1',
      config_path: 'a',
      identity: { account_id: 7, nickname: 'x' },
      running: true,
      online,
      browser: true,
      current: true,
      startup: false,
      duplicate: false,
      last_login: null,
      avatar: 'data:image/png;base64,AAAA',
      in_game: false,
      character: null,
      locked: false,
    })
    fetchAccounts.mockImplementation(() => Promise.resolve([account(true)]) as never)
    const hook = renderHook(() => useAccounts())
    await vi.advanceTimersByTimeAsync(0)
    const first = hook.result.current.accounts
    expect(first).toHaveLength(1)

    await vi.advanceTimersByTimeAsync(REFRESH_MS)
    expect(hook.result.current.accounts).toBe(first)

    fetchAccounts.mockImplementation(() => Promise.resolve([account(false)]) as never)
    await vi.advanceTimersByTimeAsync(REFRESH_MS)
    expect(hook.result.current.accounts).not.toBe(first)
    expect(hook.result.current.accounts[0].online).toBe(false)
    hook.unmount()
    fetchAccounts.mockImplementation(() => Promise.resolve([]))
  })
})
