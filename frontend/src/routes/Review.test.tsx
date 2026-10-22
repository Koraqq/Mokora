import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, o?: Record<string, unknown>) => (o ? `${key} ${JSON.stringify(o)}` : key),
    i18n: { language: 'en', changeLanguage: vi.fn() },
  }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

const record = {
  id: 'g1',
  started_at: '2026-10-01T00:00:00Z',
  num_players: 4,
  our_seat: 0,
  our_rank: 1,
  bot: null,
}
const row = { id: 'g1', model: 'm', reviewed: true, stale: false, summary: { matched: 8, scored: 10 } }

vi.mock('@/lib/tauri', () => ({
  invoke: (cmd: string) =>
    Promise.resolve(
      cmd === 'list_game_history'
        ? [record]
        : cmd === 'review_list'
          ? [row]
          : cmd === 'review_progress'
            ? { state: 'idle', total: 0, done: 0 }
            : cmd === 'review_selfcheck_last'
              ? null
              : [],
    ),
  listen: () => Promise.resolve(() => {}),
  HAS_TAURI: true,
}))

let features: string[] = []
vi.mock('@/hooks/useLicense', () => ({
  useLicense: () => [
    {
      state: features.length ? 'active' : 'no_time',
      signed_in: true,
      email: 'me@example.com',
      paid_until: null,
      token_expires_at: null,
      keys: [],
      features,
    },
    vi.fn(),
  ],
}))

let proState: 'ready' | 'downloading' = 'ready'
vi.mock('@/hooks/useProStatus', () => ({
  useProStatus: () =>
    proState === 'ready'
      ? { state: 'ready', version: '0.2.0' }
      : { state: 'downloading', version: '0.2.0', received: 1, total: 2 },
}))

import { Review } from './Review'

describe('Review page', () => {
  beforeEach(() => {
    features = []
    proState = 'ready'
  })

  it('free: reviews done before are listed, starting a new one is locked', async () => {
    render(<Review />)
    expect(screen.getByText('license.pro_review')).toBeTruthy()
    expect(screen.queryByText(/review\.start_selected/)).toBeNull()
    expect(screen.queryByText('review.verify_title')).toBeNull()
    await waitFor(() => expect(screen.getByText('review.status_done')).toBeTruthy())
  })

  it('paid: the start buttons are there and no lock', () => {
    features = ['pro']
    render(<Review />)
    expect(screen.getByText(/review\.start_selected/)).toBeTruthy()
    expect(screen.queryByText('license.pro_review')).toBeNull()
  })
})

describe('Review page while the paid module downloads', () => {
  it('paid but the module not here yet: says so, and offers no start', () => {
    features = ['pro']
    proState = 'downloading'
    render(<Review />)
    expect(screen.getByText('pro.not_ready')).toBeTruthy()
    expect(screen.queryByText(/review\.start_selected/)).toBeNull()
  })
})
