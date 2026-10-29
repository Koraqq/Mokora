import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { UpdatesCard } from '@/components/settings/UpdatesCard'
import { MOKORA_GITHUB_URL } from '@/lib/external'
import { useUpdaterStore, type UpdaterStore } from '@/stores/updaterStore'

const openExternal = vi.hoisted(() => vi.fn())
vi.mock('@/lib/external', async (original) => ({
  ...(await original<typeof import('@/lib/external')>()),
  openExternal,
}))

const draft = { updateOnLaunch: true, periodicUpdates: true }

function renderCard(checkNow: UpdaterStore['checkNow']) {
  useUpdaterStore.setState({ checkNow, checking: false, pendingUpdate: null })
  return render(<UpdatesCard draft={draft} setDraft={vi.fn()} />)
}

beforeEach(() => openExternal.mockClear())

describe('UpdatesCard buttons', () => {
  it('opens the project on GitHub (the repository the updater asks)', () => {
    expect(MOKORA_GITHUB_URL).toBe('https://github.com/Koraqq/Mokora')
    renderCard(vi.fn())
    fireEvent.click(screen.getByRole('button', { name: /updates.settings.go_github/ }))
    expect(openExternal).toHaveBeenCalledWith('https://github.com/Koraqq/Mokora')
  })

  it('checks by hand, saying so when there is nothing newer', async () => {
    const checkNow = vi.fn().mockResolvedValue(undefined)
    renderCard(checkNow)
    fireEvent.click(screen.getByRole('button', { name: /updates.settings.check_now/ }))
    expect(checkNow).toHaveBeenCalledWith(true)
    expect((await screen.findByRole('status')).textContent).toBe('updates.settings.up_to_date')
  })

  it('names the newer version that a check found', async () => {
    const checkNow = vi.fn(async () => {
      useUpdaterStore.setState({
        pendingUpdate: { latest_version: '0.2.0' } as UpdaterStore['pendingUpdate'],
      })
    })
    renderCard(checkNow)
    fireEvent.click(screen.getByRole('button', { name: /updates.settings.check_now/ }))
    await waitFor(() => expect(screen.getByRole('status').textContent).toContain('updates.settings.available'))
  })

  it('says why a check failed', async () => {
    renderCard(vi.fn().mockRejectedValue(new Error('offline')))
    fireEvent.click(screen.getByRole('button', { name: /updates.settings.check_now/ }))
    await waitFor(() => expect(screen.getByRole('status').textContent).toContain('updates.settings.check_failed'))
  })

  it('is out of reach while a check is under way', () => {
    useUpdaterStore.setState({ checkNow: vi.fn(), checking: true })
    render(<UpdatesCard draft={draft} setDraft={vi.fn()} />)
    const button = screen.getByRole('button', { name: /updates.settings.checking/ }) as HTMLButtonElement
    expect(button.disabled).toBe(true)
  })
})
