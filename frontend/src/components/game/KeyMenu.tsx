// The key button in the play screen's top bar, and its panel "會員功能": the
// Mokora account this computer is signed in with, and redeeming keys onto it.
// The state lives on the Rust side (`crate::license`).
//
// Signed out: "尚未登入" with a "登入" button. Pressing it turns the row into
//   the email field (focused) and "驗證". Verify signs in at once on a computer
//   that signed in with a code within 7 days; otherwise a code is mailed (one
//   per 100 seconds) and a code row appears ("請輸入6位數驗證碼", "確定").
// Signed in: the email and "登出", the status line (free member, or paid
//   member with the end date and days left), the redeemed keys, and a
//   "兌換金鑰" row that opens into the key field and "兌換".
//
// The panel opens under the button inside the right-hand column (the game's
// web view paints above the page, so nothing may reach over it). The button
// carries a red dot only when something needs looking at (`needsAttention`).

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, KeyRound, Loader2 } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useLicense } from '@/hooks/useLicense'
import {
  canRedeem,
  cleanCode,
  daysLeft,
  errorKey,
  hm,
  isEmail,
  isKeyShape,
  isPaid,
  keyStateAt,
  minutesLeft,
  needsAttention,
  RESEND_SECONDS,
  underADay,
  ymd,
  type LicenseStatus,
  type Started,
} from '@/lib/license'
import { placePanel } from '@/lib/playLayout'
import { invoke } from '@/lib/tauri'
import { cn } from '@/lib/utils'

const PANEL_WIDTH = 340

const KEY_TONE: Record<string, string> = {
  running: 'text-green-400',
  queued: 'text-sky-400',
  used: 'text-muted-foreground',
  revoked: 'text-red-400',
}

/** The current time (unix seconds), moved on every minute. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now() / 1000)
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now() / 1000), 60_000)
    return () => window.clearInterval(id)
  }, [])
  return now
}

export function KeyMenu({ minLeft }: { minLeft: number }) {
  const { t } = useTranslation()
  const [status, setStatus] = useLicense()
  const [open, setOpen] = useState(false)
  const [place, setPlace] = useState({ left: 0, width: PANEL_WIDTH })
  const box = useRef<HTMLDivElement>(null)

  // A press outside, or Escape, closes it.
  useEffect(() => {
    if (!open) return
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    document.addEventListener('pointerdown', away)
    window.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('pointerdown', away)
      window.removeEventListener('keydown', esc)
    }
  }, [open])

  const toggle = () => {
    if (!open) {
      const r = box.current?.getBoundingClientRect()
      if (r) {
        const p = placePanel(r.left + r.width / 2, window.innerWidth, minLeft, PANEL_WIDTH)
        setPlace({ left: p.left - r.left, width: p.width })
      }
    }
    setOpen((o) => !o)
  }

  return (
    <div ref={box} className="relative">
      <button
        type="button"
        onClick={toggle}
        title={t('license.title')}
        aria-label={t('license.title')}
        className="relative flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      >
        <KeyRound className="h-4 w-4" />
        {needsAttention(status) && (
          <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-red-500" />
        )}
      </button>
      {open && status && (
        <div
          className="absolute top-10 z-50 grid gap-3 rounded-lg border border-border bg-popover p-4 text-sm shadow-lg animate-in fade-in slide-in-from-top-2 duration-200"
          style={{ left: place.left, width: place.width }}
        >
          <h3 className="font-semibold">{t('license.title')}</h3>
          {status.signed_in ? (
            <SignedIn status={status} onChange={setStatus} />
          ) : (
            <SignedOut status={status} onChange={setStatus} />
          )}
        </div>
      )}
    </div>
  )
}

function SignedOut({
  status,
  onChange,
}: {
  status: LicenseStatus
  onChange: (s: LicenseStatus) => void
}) {
  const { t } = useTranslation()
  const [stage, setStage] = useState<'idle' | 'email' | 'code'>('idle')
  const [email, setEmail] = useState(status.email ?? '')
  const [code, setCode] = useState('')
  const [wait, setWait] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const emailField = useRef<HTMLInputElement>(null)

  // The email field comes up focused and selected.
  useEffect(() => {
    if (stage === 'email') {
      emailField.current?.focus()
      emailField.current?.select()
    }
  }, [stage])

  // The resend countdown.
  useEffect(() => {
    if (wait <= 0) return
    const id = window.setTimeout(() => setWait((w) => w - 1), 1000)
    return () => window.clearTimeout(id)
  }, [wait])

  const run = async (fn: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await fn()
    } catch (e) {
      setError(t(errorKey(e)))
    } finally {
      setBusy(false)
    }
  }

  const verify = () =>
    run(async () => {
      const started = await invoke<Started>('license_start', { email: email.trim() })
      if (started.result === 'signed_in') {
        onChange(started.status)
        toast.success(t('license.signed_in'))
        return
      }
      setStage('code')
      setWait(RESEND_SECONDS)
      toast.success(t('license.code_sent', { email: email.trim() }))
    })

  const confirm = () =>
    run(async () => {
      onChange(await invoke<LicenseStatus>('license_login', { email: email.trim(), code: cleanCode(code) }))
      toast.success(t('license.signed_in'))
    })

  return (
    <div className="grid gap-2">
      {stage === 'idle' ? (
        <div className="flex items-center justify-between gap-2">
          <span className="text-muted-foreground">{t('license.signed_out')}</span>
          <Button size="sm" onClick={() => setStage('email')}>
            {t('license.login')}
          </Button>
        </div>
      ) : (
        <div className="flex gap-2">
          <Input
            ref={emailField}
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && isEmail(email) && !busy && wait === 0 && void verify()}
            placeholder="name@example.com"
            type="email"
            autoComplete="email"
            className="h-8"
          />
          <Button
            size="sm"
            className="h-8 w-20 shrink-0"
            disabled={!isEmail(email) || busy || wait > 0}
            onClick={() => void verify()}
          >
            {busy && stage === 'email' ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : wait > 0 ? (
              `${wait}s`
            ) : (
              t('license.verify')
            )}
          </Button>
        </div>
      )}
      {stage === 'code' && (
        <div className="flex gap-2">
          <Input
            value={code}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && cleanCode(code).length === 6 && !busy && void confirm()}
            placeholder={t('license.code_placeholder')}
            inputMode="numeric"
            maxLength={7}
            autoComplete="one-time-code"
            autoFocus
            className="h-8 text-center font-mono tracking-[0.4em] placeholder:font-sans placeholder:tracking-normal"
          />
          <Button
            size="sm"
            className="h-8 w-20 shrink-0"
            disabled={cleanCode(code).length !== 6 || busy}
            onClick={() => void confirm()}
          >
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : t('license.confirm')}
          </Button>
        </div>
      )}
      {(status.state === 'locked' || status.state === 'blocked') && (
        <p className="text-xs text-red-400">{t(`license.state_hint.${status.state}`)}</p>
      )}
      {error && <p className="text-xs text-red-400">{error}</p>}
    </div>
  )
}

function SignedIn({
  status,
  onChange,
}: {
  status: LicenseStatus
  onChange: (s: LicenseStatus) => void
}) {
  const { t, i18n } = useTranslation()
  const now = useNow()
  const [confirmOut, setConfirmOut] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const paid = isPaid(status, now)
  const keys = status.keys
    .map((k) => ({ ...k, state: keyStateAt(k, now) }))
    .filter((k) => k.state === 'running' || k.state === 'queued')

  const logout = async () => {
    setBusy(true)
    setError(null)
    try {
      onChange(await invoke<LicenseStatus>('license_logout'))
    } catch (e) {
      setError(t(errorKey(e)))
    } finally {
      setBusy(false)
      setConfirmOut(false)
    }
  }

  return (
    <div className="grid gap-3">
      {confirmOut ? (
        <div className="grid gap-2">
          <p className="text-xs text-muted-foreground">{t('license.logout_confirm')}</p>
          <div className="flex justify-end gap-2">
            <Button variant="outline" size="sm" onClick={() => setConfirmOut(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" size="sm" disabled={busy} onClick={() => void logout()}>
              {busy && <Loader2 className="h-4 w-4 animate-spin" />}
              {t('license.logout')}
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex items-center justify-between gap-2">
          <span className="truncate font-medium">{status.email}</span>
          <Button variant="outline" size="sm" onClick={() => setConfirmOut(true)}>
            {t('license.logout')}
          </Button>
        </div>
      )}

      <p className={cn('text-xs', paid ? 'text-foreground' : 'text-muted-foreground')}>
        {paid && status.paid_until != null
          ? underADay(status.paid_until, now)
            ? t('license.status_paid_hours', {
                time: hm(status.paid_until),
                hours: Math.floor(minutesLeft(status.paid_until, now) / 60),
                minutes: minutesLeft(status.paid_until, now) % 60,
              })
            : t('license.status_paid', { date: ymd(status.paid_until), days: daysLeft(status.paid_until, now) })
          : t('license.status_free')}
      </p>
      {status.state !== 'active' && status.state !== 'no_time' && (
        <p className="text-xs text-yellow-300">{t(`license.state_hint.${status.state}`)}</p>
      )}

      {keys.length > 0 && (
        <ul className="grid gap-1">
          {keys.map((k, i) => (
            <li key={i} className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-xs">
              <span className="flex min-w-0 items-center gap-1.5">
                <span className="truncate font-mono">{k.hint}</span>
                {k.free && (
                  <span className="shrink-0 rounded bg-muted px-1 text-[10px] text-muted-foreground">
                    {t('license.free_key')}
                  </span>
                )}
              </span>
              <span className="shrink-0 tabular-nums text-muted-foreground">
                {/* A key of under a day: its times, not its dates. */}
                {k.start != null &&
                  (k.minutes != null && k.minutes < 1440
                    ? hm(k.start)
                    : new Date(k.start * 1000).toLocaleDateString(i18n.language, { month: 'numeric', day: 'numeric' }))}
                {' – '}
                {k.end != null &&
                  (k.minutes != null && k.minutes < 1440
                    ? hm(k.end)
                    : new Date(k.end * 1000).toLocaleDateString(i18n.language, { month: 'numeric', day: 'numeric' }))}
              </span>
              <span className={cn('shrink-0', KEY_TONE[k.state])}>{t(`license.key_state.${k.state}`)}</span>
            </li>
          ))}
        </ul>
      )}

      <Redeem status={status} onChange={onChange} />
      {error && <p className="text-xs text-red-400">{error}</p>}
    </div>
  )
}

function Redeem({ status, onChange }: { status: LicenseStatus; onChange: (s: LicenseStatus) => void }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const enabled = canRedeem(status)

  const redeem = async () => {
    setBusy(true)
    setError(null)
    try {
      onChange(await invoke<LicenseStatus>('license_redeem', { key: key.trim() }))
      setKey('')
      setOpen(false)
      toast.success(t('license.redeemed'))
    } catch (e) {
      setError(t(errorKey(e)))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-md border border-border">
      <button
        type="button"
        disabled={!enabled}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between px-3 py-2 text-left font-medium hover:bg-muted/50 disabled:opacity-50"
      >
        {t('license.redeem_title')}
        <ChevronDown className={cn('h-4 w-4 text-muted-foreground transition-transform', open && 'rotate-180')} />
      </button>
      {open && (
        <div className="grid gap-2 border-t border-border p-3">
          <div className="flex gap-2">
            <Input
              value={key}
              onChange={(e) => setKey(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && isKeyShape(key) && !busy && void redeem()}
              placeholder="Mokora-xxxxxx-xxxxxx-xxxxxx"
              spellCheck={false}
              autoComplete="off"
              autoFocus
              className="h-8 font-mono text-xs"
            />
            <Button size="sm" className="h-8 shrink-0" disabled={!isKeyShape(key) || busy} onClick={() => void redeem()}>
              {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : t('license.redeem')}
            </Button>
          </div>
          {error && <p className="text-xs text-red-400">{error}</p>}
        </div>
      )}
    </div>
  )
}
