import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const openExternal = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'zh-TW' } }),
}))
vi.mock('@/lib/appVersion', () => ({ getAppVersion: () => Promise.resolve('0.1.0') }))
vi.mock('@/lib/external', () => ({
  MOKORA_DISCORD_URL: 'https://discord.gg/Ta6bqH6NwN',
  openExternal: (url: string) => openExternal(url),
}))

import { AnnouncementsDialog } from './AnnouncementsDialog'
import { useAnnouncementStore } from '@/stores/announcementStore'

describe('AnnouncementsDialog', () => {
  beforeEach(() => {
    openExternal.mockReset()
    useAnnouncementStore.setState({
      lastSeenDate: '2026-10-23',
      entries: [],
      open: true,
      launchShowPending: false,
    })
  })

  it('opens the project Discord from the purchase link', () => {
    render(<AnnouncementsDialog />)
    fireEvent.click(screen.getByRole('button', { name: 'announcements.dialog.discord_purchase' }))
    expect(openExternal).toHaveBeenCalledWith('https://discord.gg/Ta6bqH6NwN')
  })
})
