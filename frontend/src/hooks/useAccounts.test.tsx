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
})
