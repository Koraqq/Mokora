import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) =>
      opts ? `${key} ${JSON.stringify(opts)}` : key,
    i18n: { language: 'en' },
  }),
}))

const invoke = vi.fn()
vi.mock('@/lib/tauri', () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  HAS_TAURI: true,
}))

const toast = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }))
vi.mock('@/components/ui/sonner', () => ({ toast }))

import { ShareDialog } from './ShareDialog'
import type { HistoryPlayer } from '@/lib/historyPlayers'
import type { GameRecord, MadeShare, SharePreview } from '@/types'

function record(id: string): GameRecord {
  return {
    id,
    started_at: '2026-10-01T00:00:00Z',
    ended_at: '2026-10-01T00:40:00Z',
    platform: 'majsoul',
    num_players: 4,
    kyoku_mode: 'east_south',
    names: ['Mine', 'a', 'b', 'c'],
    our_seat: 0,
    final_scores: [30_000, 25_000, 25_000, 20_000],
    final_ranks: [1, 2, 3, 4],
    our_rank: 1,
    our_delta: 5_000,
    stats: {} as GameRecord['stats'],
    match_info: { platform: 'majsoul', mode_id: 12, account_id: 7 },
    log_path: `games/${id}.mjai.jsonl`,
  }
}

const players: HistoryPlayer[] = [
  { key: 'majsoul:7', name: 'Mine', games: 2, lastPlayed: '2026-10-01T00:00:00Z' },
]
const records = [record('g1'), record('g2')]
const keys = new Map([
  ['g1', 'majsoul:7'],
  ['g2', 'majsoul:7'],
])

const share: MadeShare = {
  id: 'AAAAAAAA',
  code: 'MK-AAAA-AAAA-1111-2222',
  name: 'Mine',
  games: 2,
  made_at: 1_000,
  expires_at: 1_000 + 3 * 86_400,
}
const preview: SharePreview = {
  id: 'BBBBBBBB',
  name: 'Friend',
  games: 12,
  first: '2026-09-01T00:00:00Z',
  last: '2026-10-01T00:00:00Z',
  expires_at: 5_000,
  replaces: null,
}

function open(onImported = vi.fn()) {
  render(
    <ShareDialog
      players={players}
      records={records}
      keys={keys}
      activeKey="majsoul:7"
      onImported={onImported}
    />,
  )
  fireEvent.click(screen.getByRole('button', { name: 'history.share.button' }))
  return onImported
}

describe('ShareDialog', () => {
  beforeEach(() => {
    invoke.mockReset()
    toast.success.mockReset()
    toast.error.mockReset()
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'history_share_made') return Promise.resolve([])
      return Promise.resolve(undefined)
    })
  })

  it('shares the picked player\'s own games and shows the code it is given', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'history_share_made' ? Promise.resolve([]) : Promise.resolve(share),
    )
    open()
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.make' }))
    })
    expect(invoke).toHaveBeenCalledWith('history_share_create', {
      ids: ['g1', 'g2'],
      name: 'Mine',
      player: 'majsoul:7',
    })
    expect((screen.getByLabelText('history.share.code') as HTMLInputElement).value).toBe(share.code)
  })

  it('says so when the account is not signed in', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'history_share_made' ? Promise.resolve([]) : Promise.reject('signed_out'),
    )
    open()
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.make' }))
    })
    expect(screen.getByRole('alert').textContent).toBe('history.share.error.signed_out')
  })

  it('lists the shares in force and revokes one after asking', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'history_share_made' ? Promise.resolve([share]) : Promise.resolve(undefined),
    )
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
    open()
    await screen.findByLabelText('history.share.code')
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.revoke' }))
    })
    expect(confirm).toHaveBeenCalled()
    expect(invoke).toHaveBeenCalledWith('history_share_revoke', { id: 'AAAAAAAA' })
    expect(screen.queryByLabelText('history.share.code')).toBeNull()
    confirm.mockRestore()
  })

  it('keeps a share if the revoke is not confirmed', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'history_share_made' ? Promise.resolve([share]) : Promise.resolve(undefined),
    )
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    open()
    await screen.findByLabelText('history.share.code')
    fireEvent.click(screen.getByRole('button', { name: 'history.share.revoke' }))
    expect(invoke).not.toHaveBeenCalledWith('history_share_revoke', expect.anything())
    confirm.mockRestore()
  })

  it('looks a code up, shows what it holds, and keeps it only when confirmed', async () => {
    const kept = { id: 'BBBBBBBB', name: 'Friend', source: 's', imported_at: 1, records: [record('x')] }
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'history_share_made') return Promise.resolve([])
      if (cmd === 'history_import_lookup') return Promise.resolve(preview)
      if (cmd === 'history_import_confirm') return Promise.resolve(kept)
      return Promise.resolve(undefined)
    })
    const onImported = open()
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'history.share.tab_import' }))
    fireEvent.click(screen.getByRole('tab', { name: 'history.share.tab_import' }))
    fireEvent.change(screen.getByLabelText('history.share.import_hint'), {
      target: { value: ' MK-AAAA ' },
    })
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.lookup' }))
    })
    expect(invoke).toHaveBeenCalledWith('history_import_lookup', { code: 'MK-AAAA' })
    expect(document.querySelector('[data-share-preview]')).toBeTruthy()
    // Nothing is kept by looking.
    expect(invoke).not.toHaveBeenCalledWith('history_import_confirm', expect.anything())
    expect(screen.getByText(/history\.share\.no_reshare/)).toBeTruthy()

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.import' }))
    })
    expect(invoke).toHaveBeenCalledWith('history_import_confirm', { id: 'BBBBBBBB' })
    expect(onImported).toHaveBeenCalledWith(kept)
  })

  it('words a code that has expired', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'history_share_made' ? Promise.resolve([]) : Promise.reject('expired'),
    )
    open()
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'history.share.tab_import' }))
    fireEvent.click(screen.getByRole('tab', { name: 'history.share.tab_import' }))
    fireEvent.change(screen.getByLabelText('history.share.import_hint'), { target: { value: 'MK-X' } })
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'history.share.lookup' }))
    })
    expect(screen.getByRole('alert').textContent).toBe('history.share.error.expired')
  })
})
