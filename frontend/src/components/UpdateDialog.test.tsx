import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: 'en', changeLanguage: vi.fn() },
  }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

const invoke = vi.fn()
vi.mock('@/lib/tauri', () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  HAS_TAURI: true,
}))
const openExternal = vi.fn()
vi.mock('@/lib/external', () => ({ openExternal: (url: string) => openExternal(url) }))

import { UpdateDialog } from './UpdateDialog'
import { useUpdaterStore, type UpdateInfo } from '@/stores/updaterStore'
import { useAnnouncementStore } from '@/stores/announcementStore'

const INFO: UpdateInfo = {
  current: '0.3.0',
  latest_tag: 'v0.3.1',
  latest_version: '0.3.1',
  body: 'Fixed: things',
  html_url: 'https://github.com/Koraqq/Mokora/releases/latest',
}

function reset() {
  useUpdaterStore.setState({
    pendingUpdate: null,
    dialogOpen: false,
    checking: false,
    launchChecked: false,
    installing: false,
    dismissedVersion: null,
  })
  useAnnouncementStore.setState({ open: false })
}

describe('update check', () => {
  beforeEach(() => {
    reset()
    invoke.mockReset()
  })

  it('asks once per launch however often the notifier remounts', async () => {
    invoke.mockResolvedValue(INFO)
    await useUpdaterStore.getState().checkOnLaunch()
    await useUpdaterStore.getState().checkOnLaunch()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith('check_for_update', { manual: false })
    expect(useUpdaterStore.getState().dialogOpen).toBe(true)
  })

  it('stays closed when up to date', async () => {
    invoke.mockResolvedValue(null)
    await useUpdaterStore.getState().checkOnLaunch()
    expect(useUpdaterStore.getState().dialogOpen).toBe(false)
  })

  it('the Settings button asks past "don\'t remind me" and reports failures', async () => {
    invoke.mockRejectedValue('offline')
    await expect(useUpdaterStore.getState().checkNow(true)).rejects.toBe('offline')
    expect(invoke).toHaveBeenCalledWith('check_for_update', { manual: true })
  })

  it('the periodic check does not ask again about a version cancelled this run', async () => {
    invoke.mockResolvedValue(INFO)
    useUpdaterStore.setState({ dismissedVersion: '0.3.1' })
    await useUpdaterStore.getState().checkNow(false, true)
    expect(invoke).toHaveBeenCalledWith('check_for_update', { manual: false, periodic: true })
    expect(useUpdaterStore.getState().dialogOpen).toBe(false)
    useUpdaterStore.setState({ dismissedVersion: null })
    await useUpdaterStore.getState().checkNow(false, true)
    expect(useUpdaterStore.getState().dialogOpen).toBe(true)
  })

  it('a failed launch check stays quiet', async () => {
    invoke.mockRejectedValue('offline')
    await expect(useUpdaterStore.getState().checkOnLaunch()).resolves.toBeUndefined()
  })
})

describe('UpdateDialog', () => {
  beforeEach(() => {
    reset()
    invoke.mockReset()
    openExternal.mockReset()
    useUpdaterStore.setState({ pendingUpdate: INFO, dialogOpen: true })
  })

  it('offers "don\'t remind me" at the far left, then Cancel and Update', () => {
    render(<UpdateDialog />)
    expect(screen.getByText('updates.dialog.title')).toBeTruthy()
    expect(screen.getByText('updates.dialog.question')).toBeTruthy()
    expect(screen.getByText('Fixed: things')).toBeTruthy()
    const order = screen
      .getAllByRole('button')
      .map((b) => b.textContent)
      .filter((t) => t?.startsWith('updates.dialog.'))
    expect(order).toEqual([
      'updates.dialog.mute',
      'updates.dialog.dismiss',
      'updates.dialog.go',
    ])
  })

  it('Update downloads and installs', async () => {
    invoke.mockResolvedValue(undefined)
    render(<UpdateDialog />)
    await act(async () => {
      fireEvent.click(screen.getByText('updates.dialog.go'))
    })
    expect(invoke).toHaveBeenCalledWith('install_update')
    expect(openExternal).not.toHaveBeenCalled()
  })

  it('Cancel closes and remembers the version for the periodic check', () => {
    render(<UpdateDialog />)
    fireEvent.click(screen.getByText('updates.dialog.dismiss'))
    expect(useUpdaterStore.getState().dialogOpen).toBe(false)
    expect(useUpdaterStore.getState().pendingUpdate).toEqual(INFO)
    expect(useUpdaterStore.getState().dismissedVersion).toBe('0.3.1')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('"Don\'t remind me" saves the choice and drops the offer', async () => {
    invoke.mockResolvedValue(undefined)
    render(<UpdateDialog />)
    await act(async () => {
      fireEvent.click(screen.getByText('updates.dialog.mute'))
    })
    expect(invoke).toHaveBeenCalledWith('mute_update_reminders')
    expect(useUpdaterStore.getState().pendingUpdate).toBeNull()
    expect(useUpdaterStore.getState().dialogOpen).toBe(false)
  })

  it('waits while the announcements are open', () => {
    useAnnouncementStore.setState({ open: true })
    render(<UpdateDialog />)
    expect(screen.queryByText('updates.dialog.question')).toBeNull()
    act(() => useAnnouncementStore.setState({ open: false }))
    expect(screen.getByText('updates.dialog.question')).toBeTruthy()
  })
})
