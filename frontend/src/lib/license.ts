// The app's license, as the Rust side reports it (`crate::license`), and the
// small checks the key panel makes before asking the server.

/** One redeemed key of the account. */
export type LicenseKey = {
  hint: string
  days: number
  /** A free key's length in minutes, whole hours (`days` is then 0). */
  minutes?: number | null
  /** A free key: the account is free while it runs, its paid time waiting. */
  free?: boolean
  start: number | null
  end: number | null
  /** queued | running | used | revoked */
  state: string
}

/** Mirrors `crate::license::LicenseStatus`. */
export type LicenseStatus = {
  /** signed_out | active | no_time | offline | locked | blocked */
  state: 'signed_out' | 'active' | 'no_time' | 'offline' | 'locked' | 'blocked'
  signed_in: boolean
  email: string | null
  /** When the account's time ends (unix seconds). */
  paid_until: number | null
  /** Until when it works without the server (unix seconds). */
  token_expires_at: number | null
  keys: LicenseKey[]
  features: string[]
}

/** Sent by the Rust side whenever the status changes. */
export const LICENSE_CHANGED = 'license-changed'

/** Seconds before another sign-in code may be sent (the server's limit). */
export const RESEND_SECONDS = 100

/** The characters a key's groups are made of (the server's KEY_ALPHABET). */
const KEY_CHARS = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789!@#$%&*+=?'
const escaped = KEY_CHARS.replace(/[\\\]^-]/g, '\\$&')
const KEY_RE = new RegExp(`^Mokora(?:-[${escaped}]{6}){3}$`)

export function isKeyShape(key: string): boolean {
  return KEY_RE.test(key.trim())
}

export function isEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())
}

/** Six digits, as typed (spaces dropped). */
export function cleanCode(code: string): string {
  return code.replace(/\s/g, '')
}

/** Whole days from `now` to `until` (unix seconds), never below 0. A key just
 *  redeemed shows its own length (30), so `now` must be the current time. */
export function daysLeft(until: number, now: number): number {
  return Math.max(0, Math.ceil((until - now) / 86400))
}

/** Whole minutes from `now` to `until` (unix seconds), never below 0. */
export function minutesLeft(until: number, now: number): number {
  return Math.max(0, Math.ceil((until - now) / 60))
}

/** Whether the time left is under a day: then it is told in hours and
 *  minutes (a test key's few hours would otherwise read "1 day left"). */
export function underADay(until: number, now: number): boolean {
  return until - now < 86400
}

/** A key's state now: the server's word, moved on by the clock. The list is
 *  what the server said at the last renewal, up to an hour ago, and a test
 *  key ends in minutes. */
export function keyStateAt(k: LicenseKey, now: number): string {
  if (k.state !== 'running' && k.state !== 'queued') return k.state
  if (k.end != null && k.end <= now) return 'used'
  if (k.start != null && k.start <= now) return 'running'
  return k.state
}

/** `until` as HH:MM. */
export function hm(until: number): string {
  const d = new Date(until * 1000)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

/** `until` as YYYY/MM/DD. */
export function ymd(until: number): string {
  const d = new Date(until * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())}`
}

/** The feature a paid account unlocks (full auto and the game review). */
export const PRO = 'pro'

/** Whether the license unlocks the paid features right now. */
export function hasPro(status: LicenseStatus | null): boolean {
  return status?.features.includes(PRO) ?? false
}

/** Whether the key button carries its red dot: something is wrong that the
 *  player should look at (paid time that cannot be confirmed with the server,
 *  or a stopped account). Signed out and free are ordinary states. */
export function needsAttention(status: LicenseStatus | null): boolean {
  return status?.state === 'offline' || status?.state === 'locked' || status?.state === 'blocked'
}

/** Whether the account has paid time left. */
export function isPaid(status: LicenseStatus, now: number): boolean {
  return status.paid_until != null && status.paid_until > now
}

/** Something the player loses that the server decided, for a notice:
 *  a key stopped (or deleted), the paid time over, the account stopped. */
export type LicenseLoss =
  | { kind: 'revoked'; hint: string }
  | { kind: 'expired' }
  | { kind: 'stopped'; state: 'locked' | 'blocked' }

/** What was lost between status `prev` and `next` (the same account signed
 *  in both times). A free key starting is the player's own doing, and a
 *  signed-out or offline status says nothing new, so neither is a loss. */
export function licenseLosses(prev: LicenseStatus | null, next: LicenseStatus, now: number): LicenseLoss[] {
  if (!prev || !prev.signed_in || !next.signed_in || prev.email !== next.email) return []
  if ((next.state === 'locked' || next.state === 'blocked') && next.state !== prev.state) {
    return [{ kind: 'stopped', state: next.state }]
  }
  const after = new Map(next.keys.map((k) => [k.hint, k]))
  const revoked: LicenseLoss[] = prev.keys
    .filter((k) => {
      const was = keyStateAt(k, now)
      const later = after.get(k.hint)
      return (was === 'running' || was === 'queued') && (!later || later.state === 'revoked')
    })
    .map((k) => ({ kind: 'revoked', hint: k.hint }))
  if (revoked.length > 0) return revoked
  const freeRunning = next.keys.some((k) => k.free && keyStateAt(k, now) === 'running')
  if (hasPro(prev) && !hasPro(next) && next.state === 'no_time' && !freeRunning) return [{ kind: 'expired' }]
  return []
}

/** What "verify" did (mirrors `crate::license::Started`). */
export type Started = { result: 'signed_in'; status: LicenseStatus } | { result: 'code_sent' }

/** Whether a key can be redeemed in this state. */
export function canRedeem(status: LicenseStatus): boolean {
  return status.signed_in && status.state !== 'locked' && status.state !== 'blocked'
}

/** The server's error codes the panel has words for; anything else is shown
 *  as the generic failure. */
export const LICENSE_ERRORS = [
  'bad_request',
  'blocked',
  'too_soon',
  'mail_failed',
  'no_code',
  'code_expired',
  'wrong_code',
  'too_many_attempts',
  'bound_elsewhere',
  'locked',
  'invalid_session',
  'signed_out',
  'invalid_key',
  'revoked',
  'already_used',
  'too_many_redeems',
  'no_machine_id',
  'device_expired',
  'bad_token',
  'pro_required',
  'network',
] as const

/** The words for an error a command answered with: the paid-feature
 *  refusals (not paid; paid but the paid module still on its way) in the
 *  player's language, anything else as it came. */
export function commandError(e: unknown, t: (key: string) => string): string {
  const code = String(e)
  if (code === 'pro_required') return t('license.pro_required')
  if (code === 'pro_not_ready') return t('pro.not_ready')
  return code
}

/** How a paid feature that runs in the paid module stands: `unknown` until
 *  the license answers, `locked` without the paid feature, `downloading`
 *  while the module is on its way, `open` when it can run. */
export type PaidAccess = 'unknown' | 'locked' | 'downloading' | 'open'

export function paidAccess(
  license: LicenseStatus | null,
  moduleReady: boolean | null,
): PaidAccess {
  if (license === null) return 'unknown'
  if (!hasPro(license)) return 'locked'
  // The module's status not read yet: not shown locked meanwhile.
  if (moduleReady === false) return 'downloading'
  return 'open'
}

export function errorKey(code: unknown): string {
  const c = String(code)
  return (LICENSE_ERRORS as readonly string[]).includes(c)
    ? `license.error.${c}`
    : 'license.error.unknown'
}
