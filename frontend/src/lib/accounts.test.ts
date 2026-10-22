import { describe, expect, it } from 'vitest'

import {
  accountLabel,
  byRecentLogin,
  canAddAccount,
  ownPlayerKey,
  type AccountInfo,
} from './accounts'

function account(
  id: string,
  current: boolean,
  account_id?: number,
  last_login: number | null = null,
): AccountInfo {
  return {
    id,
    config_path: `accounts/${id}/config.toml`,
    identity: account_id == null ? null : { account_id, nickname: `n${id}` },
    running: true,
    online: true,
    browser: true,
    current,
    startup: false,
    duplicate: false,
    last_login,
    avatar: null,
    in_game: false,
    character: null,
    locked: false,
  }
}

describe('byRecentLogin', () => {
  it('puts the most recent login first and the never-logged-in last', () => {
    const ids = byRecentLogin([
      account('main', false, 1, 100),
      account('2', true, 2, 300),
      account('3', false),
      account('4', false, 4, 200),
      account('5', false),
    ]).map((a) => a.id)
    expect(ids).toEqual(['2', '4', 'main', '3', '5'])
  })
})

describe('ownPlayerKey', () => {
  it('is the login of the current account', () => {
    expect(ownPlayerKey([account('main', false, 11), account('2', true, 22)])).toBe(
      'majsoul:22',
    )
  })

  it('is null before the current account has logged in', () => {
    expect(ownPlayerKey([account('main', true), account('2', false, 22)])).toBeNull()
    expect(ownPlayerKey([])).toBeNull()
  })
})

describe('accountLabel', () => {
  const t = (key: string, o?: Record<string, unknown>) => (o ? `${key}:${String(o.n)}` : key)

  it('is the nickname once logged in, the translated slot name before', () => {
    expect(accountLabel(account('2', false, 77), t)).toBe('n2')
    expect(accountLabel(account('3', false), t)).toBe('sidebar.account.numbered:3')
    expect(accountLabel(account('main', false), t)).toBe('sidebar.account.main')
  })

  it('falls back when the nickname is empty', () => {
    const a = account('4', false, 1)
    a.identity = { account_id: 1, nickname: '' }
    expect(accountLabel(a, t)).toBe('sidebar.account.numbered:4')
  })
})

describe('the free account limit', () => {
  it('free: a second account may be added, a third may not; paid: no limit', () => {
    const one = [account('main', true)]
    const two = [...one, account('2', false)]
    expect(canAddAccount(one, false)).toBe(true)
    expect(canAddAccount(two, false)).toBe(false)
    expect(canAddAccount(two, true)).toBe(true)
  })
})
