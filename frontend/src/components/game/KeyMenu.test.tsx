import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, o?: Record<string, unknown>) => (o ? `${key} ${JSON.stringify(o)}` : key),
    i18n: { language: 'en', changeLanguage: vi.fn() },
  }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

const invoke = vi.fn()
vi.mock('@/lib/tauri', () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  listen: () => Promise.resolve(() => {}),
  HAS_TAURI: true,
}))

import type { LicenseStatus } from '@/lib/license'

let status: LicenseStatus
const setStatus = vi.fn()
vi.mock('@/hooks/useLicense', () => ({ useLicense: () => [status, setStatus] }))

import { KeyMenu } from './KeyMenu'

const base: LicenseStatus = {
  state: 'signed_out',
  signed_in: false,
  email: null,
  paid_until: null,
  token_expires_at: null,
  keys: [],
  features: [],
}

function open() {
  render(<KeyMenu minLeft={0} />)
  fireEvent.click(screen.getByLabelText('license.title'))
}

describe('KeyMenu', () => {
  beforeEach(() => {
    invoke.mockReset()
    setStatus.mockReset()
  })

  it('signed out: only "not signed in" and the sign-in button, no key section', () => {
    status = base
    open()
    expect(screen.getByText('license.signed_out')).toBeTruthy()
    expect(screen.queryByText('license.redeem_title')).toBeNull()
  })

  it('verify sends a code, then asks for it in a centred six-digit field', async () => {
    status = base
    invoke.mockResolvedValue({ result: 'code_sent' })
    open()
    fireEvent.click(screen.getByText('license.login'))
    fireEvent.change(screen.getByPlaceholderText('name@example.com'), { target: { value: 'me@example.com' } })
    await act(async () => {
      fireEvent.click(screen.getByText('license.verify'))
    })
    expect(invoke).toHaveBeenCalledWith('license_start', { email: 'me@example.com' })
    const code = screen.getByPlaceholderText('license.code_placeholder')
    expect(code.className).toContain('text-center')
    expect(screen.getByText('license.confirm')).toBeTruthy()
  })

  it('a trusted computer signs in on verify without a code', async () => {
    status = base
    const signedIn = { ...base, state: 'no_time', signed_in: true, email: 'me@example.com' }
    invoke.mockResolvedValue({ result: 'signed_in', status: signedIn })
    open()
    fireEvent.click(screen.getByText('license.login'))
    fireEvent.change(screen.getByPlaceholderText('name@example.com'), { target: { value: 'me@example.com' } })
    await act(async () => {
      fireEvent.click(screen.getByText('license.verify'))
    })
    expect(setStatus).toHaveBeenCalledWith(signedIn)
    expect(screen.queryByPlaceholderText('license.code_placeholder')).toBeNull()
  })

  it('no red dot when signed out or free, one when the account is stopped', () => {
    const dot = () => screen.getByLabelText('license.title').querySelector('.bg-red-500')
    status = base
    const { unmount } = render(<KeyMenu minLeft={0} />)
    expect(dot()).toBeNull()
    unmount()
    status = { ...base, state: 'no_time', signed_in: true, email: 'me@example.com' }
    const second = render(<KeyMenu minLeft={0} />)
    expect(dot()).toBeNull()
    second.unmount()
    status = { ...base, state: 'locked', signed_in: true, email: 'me@example.com' }
    render(<KeyMenu minLeft={0} />)
    expect(dot()).not.toBeNull()
  })

  it('signed in, free: email, sign out, free status and the redeem row', () => {
    status = { ...base, state: 'no_time', signed_in: true, email: 'me@example.com' }
    open()
    expect(screen.getByText('me@example.com')).toBeTruthy()
    expect(screen.getByText('license.logout')).toBeTruthy()
    expect(screen.getByText('license.status_free')).toBeTruthy()
    fireEvent.click(screen.getByText('license.redeem_title'))
    expect(screen.getByPlaceholderText('Mokora-xxxxxx-xxxxxx-xxxxxx')).toBeTruthy()
  })

  it('signed in, paid: a 30-day key just redeemed shows 30 days left', () => {
    const now = Date.now() / 1000
    status = {
      ...base,
      state: 'active',
      signed_in: true,
      email: 'me@example.com',
      paid_until: now - 5 + 30 * 86400,
      features: ['pro'],
    }
    open()
    expect(screen.getByText(/license\.status_paid/).textContent).toContain('"days":30')
  })

  it('a test key of a few hours is told in hours and minutes, not as a day', () => {
    const now = Date.now() / 1000
    status = {
      ...base,
      state: 'active',
      signed_in: true,
      email: 'me@example.com',
      paid_until: now + 3 * 3600 + 20 * 60 - 30,
      features: ['pro'],
    }
    open()
    const text = screen.getByText(/license\.status_paid_hours/).textContent
    expect(text).toContain('"hours":3')
    expect(text).toContain('"minutes":20')
  })
})
