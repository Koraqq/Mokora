import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render } from '@testing-library/react'

const invoke = vi.hoisted(() => vi.fn().mockResolvedValue(null))
const accountState = vi.hoisted(() => ({ inGame: false }))

vi.mock('@/lib/tauri', () => ({ HAS_TAURI: true, invoke }))
vi.mock('@/hooks/useAccounts', () => ({
  useAccounts: () => ({
    accounts: [{ in_game: accountState.inGame }],
    refresh: vi.fn(),
  }),
}))
vi.mock('@/components/UpdateDialog', () => ({ UpdateDialog: () => null }))

async function loadNotifier() {
  const [{ UpdateNotifier }, { useUpdaterStore }] = await Promise.all([
    import('./UpdateNotifier'),
    import('@/stores/updaterStore'),
  ])
  useUpdaterStore.setState({
    autoCheckEnabled: true,
    periodicCheckEnabled: true,
    lastChecked: null,
    pendingUpdate: null,
    dialogOpen: false,
    checking: false,
    installing: false,
    launchChecked: false,
    dismissedVersion: null,
  })
  return { UpdateNotifier, useUpdaterStore }
}

describe('UpdateNotifier scheduling', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-10-26T12:00:00Z'))
    invoke.mockReset().mockResolvedValue(null)
    accountState.inGame = false
    localStorage.clear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('checks once shortly after launch, then three minutes after that check', async () => {
    const { UpdateNotifier } = await loadNotifier()
    render(<UpdateNotifier />)

    await act(async () => vi.advanceTimersByTimeAsync(3000))
    expect(invoke).toHaveBeenCalledWith('check_for_update', { manual: false })

    invoke.mockClear()
    await act(async () => vi.advanceTimersByTimeAsync(3 * 60 * 1000 - 1))
    expect(invoke).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTimeAsync(1))
    expect(invoke).toHaveBeenCalledWith('check_for_update', {
      manual: false,
      periodic: true,
    })
  })

  it('waits through an active game and checks immediately when the last game ends', async () => {
    accountState.inGame = true
    const { UpdateNotifier } = await loadNotifier()
    const view = render(<UpdateNotifier />)

    await act(async () => vi.advanceTimersByTimeAsync(10 * 60 * 1000))
    expect(invoke).not.toHaveBeenCalled()

    accountState.inGame = false
    await act(async () => view.rerender(<UpdateNotifier />))
    expect(invoke).toHaveBeenCalledWith('check_for_update', { manual: false })
  })
})
