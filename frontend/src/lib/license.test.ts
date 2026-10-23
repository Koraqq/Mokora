import { describe, expect, it } from 'vitest'

import {
  cleanCode,
  commandError,
  daysLeft,
  errorKey,
  hasPro,
  isEmail,
  isKeyShape,
  keyStateAt,
  licenseLosses,
  needsAttention,
  paidAccess,
  type LicenseKey,
  type LicenseStatus,
} from './license'

const status = (state: LicenseStatus['state'], features: string[] = []): LicenseStatus => ({
  state,
  signed_in: state !== 'signed_out',
  email: null,
  paid_until: null,
  token_expires_at: null,
  keys: [],
  features,
})

describe('license form checks', () => {
  it('takes keys of the server shape only', () => {
    expect(isKeyShape('Mokora-ABCdef-23!@#$-%&*+=?')).toBe(true)
    expect(isKeyShape('  Mokora-ABCdef-23!@#$-%&*+=?  ')).toBe(true)
    expect(isKeyShape('Mokora-ABCde0-23!@#$-%&*+=?')).toBe(false)
    expect(isKeyShape('mokora-ABCdef-23!@#$-%&*+=?')).toBe(false)
    expect(isKeyShape('Mokora-ABCdef-23!@#$')).toBe(false)
  })

  it('checks emails and codes loosely', () => {
    expect(isEmail('me@example.com')).toBe(true)
    expect(isEmail('me@example')).toBe(false)
    expect(cleanCode(' 123 456 ')).toBe('123456')
  })

  it('counts days left, never below zero', () => {
    expect(daysLeft(86400 * 3, 0)).toBe(3)
    expect(daysLeft(86400 * 3 - 1, 0)).toBe(3)
    expect(daysLeft(0, 100)).toBe(0)
  })

  it('unlocks the paid features only with pro', () => {
    expect(hasPro(status('active', ['pro']))).toBe(true)
    expect(hasPro(status('no_time'))).toBe(false)
    expect(hasPro(null)).toBe(false)
  })

  it('the red dot is for problems, not for signed out or free', () => {
    expect(needsAttention(status('signed_out'))).toBe(false)
    expect(needsAttention(status('no_time'))).toBe(false)
    expect(needsAttention(status('active', ['pro']))).toBe(false)
    expect(needsAttention(status('offline'))).toBe(true)
    expect(needsAttention(status('locked'))).toBe(true)
    expect(needsAttention(status('blocked'))).toBe(true)
    expect(needsAttention(null)).toBe(false)
  })

  it('the state of a key moves on with the clock between renewals', () => {
    const key = { hint: 'k', days: 0, minutes: 5, start: 100, end: 400, state: 'queued' }
    expect(keyStateAt(key, 50)).toBe('queued')
    expect(keyStateAt(key, 200)).toBe('running')
    expect(keyStateAt({ ...key, state: 'running' }, 400)).toBe('used')
    expect(keyStateAt({ ...key, state: 'revoked' }, 200)).toBe('revoked')
  })

  it('words known server codes and falls back otherwise', () => {
    expect(errorKey('wrong_code')).toBe('license.error.wrong_code')
    expect(errorKey('pro_required')).toBe('license.error.pro_required')
    const t = (k: string) => `T:${k}`
    expect(commandError('pro_required', t)).toBe('T:license.pro_required')
    expect(commandError('disk full', t)).toBe('disk full')
    expect(commandError('pro_not_ready', t)).toBe('T:pro.not_ready')
    expect(errorKey('weird')).toBe('license.error.unknown')
  })
})

describe('paid features in the module', () => {
  it('open only when paid and the module is here', () => {
    const paid = status('active', ['pro'])
    expect(paidAccess(null, true)).toBe('unknown')
    expect(paidAccess(status('no_time'), true)).toBe('locked')
    expect(paidAccess(paid, false)).toBe('downloading')
    expect(paidAccess(paid, true)).toBe('open')
    expect(paidAccess(paid, null)).toBe('open')
  })
})

describe('what the player loses, for a notice', () => {
  const NOW = 1_000_000
  const key = (hint: string, state: string, extra: Partial<LicenseKey> = {}): LicenseKey => ({
    hint,
    days: 30,
    start: NOW - 10,
    end: NOW + 1000,
    state,
    ...extra,
  })
  const paid = (keys: LicenseKey[]): LicenseStatus => ({
    ...status('active', ['pro']),
    email: 'me@x.com',
    paid_until: NOW + 1000,
    keys,
  })
  const free = (keys: LicenseKey[]): LicenseStatus => ({ ...status('no_time'), email: 'me@x.com', keys })

  it('a running or queued key stopped or deleted on the server', () => {
    const before = paid([key('Mokora-AAA…', 'running'), key('Mokora-BBB…', 'queued', { start: NOW + 1000, end: NOW + 2000 })])
    expect(licenseLosses(before, free([key('Mokora-AAA…', 'revoked'), key('Mokora-BBB…', 'queued')]), NOW)).toEqual([
      { kind: 'revoked', hint: 'Mokora-AAA…' },
    ])
    // A queued key deleted: still paid, but that key is gone.
    expect(licenseLosses(before, paid([key('Mokora-AAA…', 'running')]), NOW)).toEqual([
      { kind: 'revoked', hint: 'Mokora-BBB…' },
    ])
  })

  it('the paid time running out', () => {
    const before = paid([key('Mokora-AAA…', 'running', { end: NOW })])
    expect(licenseLosses(before, free([key('Mokora-AAA…', 'used', { end: NOW })]), NOW)).toEqual([{ kind: 'expired' }])
  })

  it('a free key the player redeemed is not a loss', () => {
    const before = paid([key('Mokora-AAA…', 'running')])
    const after = free([
      key('Mokora-FFF…', 'running', { free: true, days: 0, minutes: 60, end: NOW + 3600 }),
      key('Mokora-AAA…', 'queued', { start: NOW + 3600, end: NOW + 4600 }),
    ])
    expect(licenseLosses(before, after, NOW)).toEqual([])
  })

  it('the account stopped', () => {
    expect(licenseLosses(paid([]), { ...status('locked'), email: 'me@x.com' }, NOW)).toEqual([
      { kind: 'stopped', state: 'locked' },
    ])
  })

  it('nothing at the first look, on signing out or in, or offline', () => {
    const before = paid([key('Mokora-AAA…', 'running')])
    expect(licenseLosses(null, before, NOW)).toEqual([])
    expect(licenseLosses(before, status('signed_out'), NOW)).toEqual([])
    expect(licenseLosses(before, { ...free([]), email: 'other@x.com' }, NOW)).toEqual([])
    expect(licenseLosses(before, { ...before, state: 'offline', features: [] }, NOW)).toEqual([])
    expect(licenseLosses(before, before, NOW)).toEqual([])
  })
})
