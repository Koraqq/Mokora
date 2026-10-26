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

import type { AccountInfo } from '@/lib/accounts'

function account(id: string, current: boolean): AccountInfo {
  return {
    id,
    config_path: `${id}/config.toml`,
    identity: { account_id: id === 'main' ? 1 : 2, nickname: id === 'main' ? 'Mine' : 'Other' },
    running: false,
    online: false,
    browser: false,
    current,
    startup: false,
    duplicate: false,
    last_login: null,
    avatar: null,
    in_game: false,
    character: null,
    locked: false,
  }
}

let accounts = [account('main', true), account('2', false)]
vi.mock('@/hooks/useAccounts', () => ({
  useAccounts: () => ({ accounts, refresh: vi.fn() }),
}))

let features: string[] = ['pro']
vi.mock('@/hooks/useLicense', () => ({
  useLicense: () => [
    {
      state: features.length ? 'active' : 'no_time',
      signed_in: true,
      email: null,
      paid_until: null,
      token_expires_at: null,
      keys: [],
      features,
    },
    vi.fn(),
  ],
}))

import { AccountAvatar } from './AccountAvatar'

/** Open the avatar menu, turn on delete mode and press the other account. */
function askToDelete() {
  const trigger = screen.getByTitle('Mine')
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: 'mouse' })
  fireEvent.click(screen.getByText('sidebar.account.delete'))
  fireEvent.click(screen.getByText('Other'))
}

describe('AccountAvatar delete', () => {
  beforeEach(() => {
    accounts = [account('main', true), account('2', false)]
    features = ['pro']
    invoke.mockReset()
    invoke.mockResolvedValue({ records: 0, records_kept: true, left: [] })
  })

  it('asks first, with "clear all game records" off by default', () => {
    render(<AccountAvatar />)
    askToDelete()
    expect(screen.getByText('sidebar.account.delete_title')).toBeTruthy()
    const box = screen.getByRole('checkbox') as HTMLInputElement
    expect(box.checked).toBe(false)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('keeps the records unless the box is ticked', async () => {
    render(<AccountAvatar />)
    askToDelete()
    await act(async () => {
      fireEvent.click(screen.getByText('sidebar.account.delete_confirm'))
    })
    expect(invoke).toHaveBeenCalledWith('delete_account', { id: '2', clearRecords: false })
  })

  it('clears them when ticked', async () => {
    render(<AccountAvatar />)
    askToDelete()
    fireEvent.click(screen.getByRole('checkbox'))
    await act(async () => {
      fireEvent.click(screen.getByText('sidebar.account.delete_confirm'))
    })
    expect(invoke).toHaveBeenCalledWith('delete_account', { id: '2', clearRecords: true })
  })
})

describe('AccountAvatar under the free limit', () => {
  const open = () => {
    render(<AccountAvatar />)
    fireEvent.pointerDown(screen.getByTitle('Mine'), { button: 0, ctrlKey: false, pointerType: 'mouse' })
  }

  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue(undefined)
    features = []
  })

  it('free with two accounts: adding a third is refused without asking the backend', () => {
    accounts = [account('main', true), account('2', false)]
    open()
    fireEvent.click(screen.getByText('sidebar.account.add'))
    expect(invoke).not.toHaveBeenCalledWith('add_account')
  })

  it('free with one account: a second may be added', () => {
    accounts = [account('main', true)]
    open()
    fireEvent.click(screen.getByText('sidebar.account.add'))
    expect(invoke).toHaveBeenCalledWith('add_account')
  })

  it('an account past the limit shows a lock and is not switched to', () => {
    accounts = [account('main', true), { ...account('3', false), locked: true }]
    open()
    expect(screen.getByLabelText('license.pro_accounts')).toBeTruthy()
    fireEvent.click(screen.getByText('Other'))
    expect(invoke).not.toHaveBeenCalledWith('switch_account', { id: '3' })
  })
})

describe('AccountAvatar account list', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue(undefined)
    features = ['pro']
    accounts = [
      account('main', true),
      account('2', false),
      account('3', false),
      account('4', false),
      account('5', false),
      account('6', false),
    ]
  })

  it('keeps at most five account rows visible and leaves management actions fixed below', () => {
    render(<AccountAvatar />)
    fireEvent.pointerDown(screen.getByTitle('Mine'), {
      button: 0,
      ctrlKey: false,
      pointerType: 'mouse',
    })

    const list = document.querySelector('[data-account-switch-list]')
    expect(list).toBeTruthy()
    expect(list?.classList.contains('max-h-40')).toBe(true)
    expect(list?.classList.contains('overflow-y-auto')).toBe(true)
    expect(list?.classList.contains('overscroll-contain')).toBe(true)
    expect(list?.classList.contains('scroll-smooth')).toBe(true)
    expect(list?.querySelectorAll('[data-slot="dropdown-menu-item"]')).toHaveLength(6)
    expect(list?.contains(screen.getByText('play.account.change_avatar'))).toBe(false)
    expect(list?.contains(screen.getByText('sidebar.account.add'))).toBe(false)
    expect(list?.contains(screen.getByText('sidebar.account.delete'))).toBe(false)
  })
})
