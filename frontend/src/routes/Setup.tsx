import { useEffect, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Toaster } from '@/components/ui/sonner'
import { InstallBlockingOverlay } from '@/components/InstallBlockingOverlay'
import { invoke } from '@/lib/tauri'
import { mergeExternal } from '@/lib/merge'
import { useTauriBridge } from '@/hooks/useTauriBridge'
import { useConfigStore } from '@/stores/configStore'
import { isNativeBot } from '@/lib/nativeBots'
import { platformInfo } from '@/lib/platforms'
import { EMBEDDED_SUPPORTED } from '@/lib/captureModes'
import { BROWSER_PATH_EXAMPLE } from '@/lib/os'
import { LANG_LABELS, SUPPORTED_LANGS, type SupportedLang } from '@/i18n'
import type { AppConfig, DetectedBrowser } from '@/types'

type Step = 'welcome' | 'mode' | 'config' | 'finish'

/** The game in Mokora's own window needs nothing configured; the other two
 *  capture modes have a step of their own. */
function stepsFor(mode: AppConfig['capture']['mode']): Step[] {
  return mode === 'embedded'
    ? ['welcome', 'mode', 'finish']
    : ['welcome', 'mode', 'config', 'finish']
}

export function Setup() {
  const { t } = useTranslation()
  // The wizard renders standalone (no <App> parent), so we wire the
  // tauri event bridge + toast surface here ourselves. Without this the
  // CfT download progress notifications wouldn't show up during setup.
  useTauriBridge()
  const stored = useConfigStore((s) => s.config)
  const setStored = useConfigStore((s) => s.setConfig)
  // The editable draft, seeded from the saved config.
  const [draft, setDraft] = useState<AppConfig | null>(() => stored)
  const [step, setStep] = useState<Step>('welcome')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const [params] = useSearchParams()
  const navigate = useNavigate()
  // Whether the Chromium step has a working browser (and a valid channel), so
  // Next can wait for it.
  const [chromiumReady, setChromiumReady] = useState(true)
  // Stored-config snapshot the wizard draft was last synced against — the
  // merge base for folding external config changes into the open draft.
  const syncedStoredRef = useRef<AppConfig | null>(stored)

  // True when the user is re-running setup from Settings (not first run).
  const isRerun = params.get('rerun') === '1' || stored?.general.first_run_completed === true

  useEffect(() => {
    if (!stored) return
    const prev = syncedStoredRef.current
    syncedStoredRef.current = stored
    if (!prev) {
      setDraft(stored)
      return
    }
    // The stored config changed mid-wizard. Three-way merge instead of
    // re-seeding: only fields the user hasn't modified relative to the
    // previous stored snapshot adopt the new stored value, and every
    // not-yet-saved wizard choice (platform, capture mode, …) survives.
    setDraft((cur) => (cur ? mergeExternal(cur, prev, stored) : stored))
  }, [stored])

  useEffect(() => {
    if (!stored) {
      invoke<AppConfig>('get_config').then(setStored).catch(() => {})
    }
  }, [stored, setStored])

  if (!draft) {
    return <div className="p-6 text-muted-foreground">{t('setup.loading')}</div>
  }

  const STEPS = stepsFor(draft.capture.mode)
  const idx = STEPS.indexOf(step)
  const canBack = idx > 0
  const canNext = idx < STEPS.length - 1
  // The Chromium step blocks Next until its browser and channel check out.
  const nextAllowed = step !== 'config' || draft.capture.mode !== 'chromium' || chromiumReady

  const next = () => setStep(STEPS[idx + 1])
  const back = () => setStep(STEPS[idx - 1])

  const finish = async () => {
    setBusy(true)
    setErr(null)
    try {
      const final: AppConfig = {
        ...draft,
        general: { ...draft.general, first_run_completed: true },
        // Enable the assistant on setup completion: the built-in bot is the
        // default, so recommendations work out of the box.
        bot: { ...draft.bot, enabled: true },
        // A first run starts in "auto" (the player's tiles are played); the
        // finish page says so, and the switch on the play screen changes it.
        autoplay: isRerun ? draft.autoplay : { ...draft.autoplay, enabled: true },
      }
      await invoke('update_config', { newConfig: final })
      setStored(final)
      navigate('/', { replace: true })
    } catch (e) {
      setErr(String(e))
    } finally {
      setBusy(false)
    }
  }

  const cancel = () => navigate('/', { replace: true })

  return (
    <div className="min-h-screen w-full flex items-center justify-center p-6">
      <Toaster />
      <InstallBlockingOverlay />
      <Card className="w-full max-w-2xl">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{t('setup.title')}</CardTitle>
            <span className="text-xs text-muted-foreground">
              {t('setup.step_progress', { current: idx + 1, total: STEPS.length })}
            </span>
          </div>
          <Stepper current={idx} total={STEPS.length} />
        </CardHeader>
        <CardContent className="grid grid-cols-1 gap-6">
          {step === 'welcome' && <WelcomeStep />}
          {step === 'mode' && <ModeStep draft={draft} setDraft={setDraft} />}
          {step === 'config' && (
            <ConfigStep draft={draft} setDraft={setDraft} onReadyChange={setChromiumReady} />
          )}
          {step === 'finish' && <FinishStep draft={draft} firstRun={!isRerun} />}

          {err && (
            <div className="rounded-md border border-red-500/40 bg-red-500/10 px-3 py-2 text-sm text-red-400">
              {err}
            </div>
          )}

          <div className="flex justify-between">
            <div className="flex gap-2">
              {canBack ? (
                <Button variant="outline" onClick={back} disabled={busy}>{t('common.back')}</Button>
              ) : (
                <span />
              )}
              {isRerun && (
                <Button variant="ghost" onClick={cancel} disabled={busy}>{t('common.cancel')}</Button>
              )}
            </div>
            {canNext ? (
              <Button onClick={next} disabled={busy || !nextAllowed}>
                {t('common.next')}
              </Button>
            ) : (
              <Button onClick={finish} disabled={busy}>
                {busy ? t('common.saving') : t('common.finish')}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

function Stepper({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex gap-1.5 mt-3">
      {Array.from({ length: total }, (_, i) => (
        <div
          key={i}
          className={`h-1 flex-1 rounded ${i <= current ? 'bg-primary' : 'bg-muted'}`}
        />
      ))}
    </div>
  )
}

function WelcomeStep() {
  const { t, i18n } = useTranslation()
  return (
    <div className="grid grid-cols-1 gap-3">
      <div className="grid gap-1.5">
        {/* Trilingual label so a user who can't read English/JP/CN still
            recognises this as a language picker before changing it. */}
        <Label className="text-xs">{t('setup.lang_label')}</Label>
        <Select
          value={i18n.language}
          onValueChange={(v) => void i18n.changeLanguage(v)}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SUPPORTED_LANGS.map((lang) => (
              <SelectItem key={lang} value={lang}>
                {LANG_LABELS[lang as SupportedLang]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <h2 className="text-lg font-semibold mt-2">{t('setup.welcome.title')}</h2>
      <p className="text-sm text-muted-foreground">
        {t('setup.welcome.body1')}
      </p>
      <p className="text-sm text-muted-foreground">
        {t(EMBEDDED_SUPPORTED ? 'setup.welcome.body2' : 'setup.welcome.body2_browser')}
      </p>
    </div>
  )
}

function ModeStep({
  draft,
  setDraft,
}: {
  draft: AppConfig
  setDraft: (c: AppConfig) => void
}) {
  const { t } = useTranslation()
  const mode = draft.capture.mode
  const pick = (m: AppConfig['capture']['mode']) =>
    setDraft({ ...draft, capture: { ...draft.capture, mode: m } })
  return (
    <div className="grid grid-cols-1 gap-3">
      <h2 className="text-lg font-semibold">{t('setup.mode.title')}</h2>
      {EMBEDDED_SUPPORTED && (
        <ModeCard
          title={t('setup.mode.embedded_title')}
          active={mode === 'embedded'}
          onClick={() => pick('embedded')}
          description={t('setup.mode.embedded_desc')}
        />
      )}
      <ModeCard
        title={t('setup.mode.chromium_title')}
        active={mode === 'chromium'}
        onClick={() => pick('chromium')}
        description={t('setup.mode.chromium_desc')}
      />
      <ModeCard
        title={t('setup.mode.mitm_title')}
        active={mode === 'mitm'}
        onClick={() => pick('mitm')}
        description={t('setup.mode.mitm_desc')}
      />
    </div>
  )
}

function ModeCard({
  title,
  description,
  active,
  onClick,
  disabled = false,
}: {
  title: string
  description: string
  active: boolean
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`text-left rounded-md border p-4 transition-colors ${
        disabled
          ? 'border-border opacity-50 cursor-not-allowed'
          : active
            ? 'border-primary bg-primary/5'
            : 'border-border hover:border-primary/40'
      }`}
    >
      <div className="flex items-center gap-2">
        <span
          className={`h-3 w-3 rounded-full border-2 ${
            active ? 'border-primary bg-primary' : 'border-muted-foreground/40'
          }`}
        />
        <span className="font-medium">{title}</span>
      </div>
      <p className="text-sm text-muted-foreground mt-2">{description}</p>
    </button>
  )
}

function ConfigStep({
  draft,
  setDraft,
  onReadyChange,
}: {
  draft: AppConfig
  setDraft: (c: AppConfig) => void
  onReadyChange: (ready: boolean) => void
}) {
  const { t } = useTranslation()
  if (draft.capture.mode === 'mitm') {
    return (
      <div className="grid grid-cols-1 gap-3">
        <h2 className="text-lg font-semibold">{t('setup.mitm.title')}</h2>
        <Field label={t('setup.mitm.listen')}>
          <Input
            value={draft.proxy.addr}
            onChange={(e) => setDraft({ ...draft, proxy: { ...draft.proxy, addr: e.target.value } })}
            placeholder="127.0.0.1:23410"
          />
        </Field>
        <Field
          label={t('settings.ca_dir')}
          hint={t('setup.mitm.ca_dir_hint')}
        >
          <Input
            value={draft.proxy.ca_dir}
            onChange={(e) => setDraft({ ...draft, proxy: { ...draft.proxy, ca_dir: e.target.value } })}
          />
        </Field>
        <Field label={t('settings.proxy_enabled')} hint={t('setup.mitm.proxy_enabled_hint')}>
          <Select
            value={draft.proxy.enabled ? 'on' : 'off'}
            onValueChange={(v) => setDraft({ ...draft, proxy: { ...draft.proxy, enabled: v === 'on' } })}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="on">{t('common.on')}</SelectItem>
              <SelectItem value="off">{t('common.off')}</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      </div>
    )
  }
  return <ChromiumConfigStep draft={draft} setDraft={setDraft} onReadyChange={onReadyChange} />
}

const MANUAL = '__manual__'
const CUSTOM = '__custom__'
const CHANNEL_PRESETS = ['stable', 'beta', 'dev', 'canary']
const BROWSER_NAMES: Record<DetectedBrowser['kind'], string> = {
  chrome: 'Google Chrome',
  edge: 'Microsoft Edge',
  brave: 'Brave',
  chromium: 'Chromium',
  chrome_for_testing: 'Chrome for Testing',
}

type Check = { state: 'idle' | 'checking' | 'ok' | 'fail'; message?: string }

/** Run `check(value)` shortly after `value` stops changing; the result of a
 *  superseded run is dropped. An empty value is `idle`. */
function useDebouncedCheck(
  value: string,
  enabled: boolean,
  check: (value: string) => Promise<void>,
  delayMs: number,
): Check {
  const [result, setResult] = useState<{ value: string; check: Check } | null>(null)
  const checkRef = useRef(check)
  useEffect(() => {
    checkRef.current = check
  })
  useEffect(() => {
    const v = value.trim()
    if (!enabled || !v) return
    let stale = false
    const timer = setTimeout(() => {
      checkRef
        .current(v)
        .then(() => !stale && setResult({ value: v, check: { state: 'ok' } }))
        .catch((e) => !stale && setResult({ value: v, check: { state: 'fail', message: String(e) } }))
    }, delayMs)
    return () => {
      stale = true
      clearTimeout(timer)
    }
  }, [value, enabled, delayMs])
  const v = value.trim()
  if (!enabled || !v) return { state: 'idle' }
  return result?.value === v ? result.check : { state: 'checking' }
}

function CheckLine({ check, ok, checking, fail }: {
  check: Check
  ok: string
  checking: string
  fail: (reason: string) => string
}) {
  if (check.state === 'idle') return null
  if (check.state === 'checking') {
    return <span className="text-xs text-muted-foreground">{checking}</span>
  }
  if (check.state === 'ok') return <span className="text-xs text-emerald-400">{ok}</span>
  return <span className="text-xs text-red-400 break-all">{fail(check.message ?? '')}</span>
}

function ChromiumConfigStep({
  draft,
  setDraft,
  onReadyChange,
}: {
  draft: AppConfig
  setDraft: (c: AppConfig) => void
  onReadyChange: (ready: boolean) => void
}) {
  const { t } = useTranslation()
  const chromium = draft.capture.chromium
  const setChromium = (patch: Partial<typeof chromium>) =>
    setDraft({
      ...draft,
      capture: { ...draft.capture, chromium: { ...chromium, ...patch } },
    })

  const [detected, setDetected] = useState<DetectedBrowser[] | null>(null)
  const [installed, setInstalled] = useState<string[] | null>(null)
  const [downloading, setDownloading] = useState(false)
  // The browser dropdown is on "type a path" instead of one of the detected ones.
  const [manual, setManual] = useState(false)
  const [channelCustom, setChannelCustom] = useState(
    () => chromium.cft_channel !== '' && !CHANNEL_PRESETS.includes(chromium.cft_channel),
  )
  const [customText, setCustomText] = useState(() =>
    CHANNEL_PRESETS.includes(chromium.cft_channel) ? '' : chromium.cft_channel,
  )

  // Look for browsers once when the step opens, and fill the dropdown with
  // the first one unless the user already has a working choice saved.
  const refresh = async (): Promise<DetectedBrowser[]> => {
    let d: DetectedBrowser[] = []
    let i: string[] = []
    try {
      ;[d, i] = await Promise.all([
        invoke<DetectedBrowser[]>('detect_system_chrome'),
        invoke<string[]>('list_cft_installed'),
      ])
    } catch {
      /* nothing detected */
    }
    setDetected(d)
    setInstalled(i)
    return d
  }
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const d = await refresh()
      if (cancelled) return
      const saved = chromium.executable
      if (saved && !d.some((b) => b.path === saved)) {
        setManual(true)
      } else if (!saved && d.length > 0) {
        setChromium({ executable: d[0].path })
      } else if (d.length === 0) {
        setManual(true)
      }
    })()
    return () => {
      cancelled = true
    }
    // Mount-time load only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const noneDetected = detected !== null && detected.length === 0
  const scanned = detected !== null

  const browserCheck = useDebouncedCheck(
    chromium.executable,
    scanned,
    async (path) => {
      await invoke('validate_browser_executable', { path })
    },
    manual ? 600 : 0,
  )

  const customCheck = useDebouncedCheck(
    customText,
    channelCustom,
    async (spec) => {
      await invoke<string>('validate_cft_channel', { channel: spec })
    },
    600,
  )
  const customVersion = customCheck.state === 'ok' ? customText.trim() : null
  const channelReady = !channelCustom || customVersion !== null
  const browserReady = browserCheck.state === 'ok'
  const ready = browserReady && channelReady

  useEffect(() => {
    onReadyChange(ready)
  }, [ready, onReadyChange])
  useEffect(() => () => onReadyChange(true), [onReadyChange])

  // A custom version that checked out is the channel the config keeps.
  const customOk = customVersion
  useEffect(() => {
    if (channelCustom && customOk && chromium.cft_channel !== customOk) {
      setChromium({ cft_channel: customOk })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channelCustom, customOk])

  const downloadChannel = channelCustom ? (customVersion ?? '') : chromium.cft_channel || 'stable'
  const downloadCft = async () => {
    setDownloading(true)
    try {
      await invoke<string>('download_chrome_for_testing', { channel: downloadChannel })
      const d = await refresh()
      // Explicit download in the wizard = explicit opt-in to CfT: use it.
      const cft = d.find((b) => b.kind === 'chrome_for_testing')
      setManual(false)
      setChromium({ force_cft: true, ...(cft ? { executable: cft.path } : {}) })
    } catch {
      /* surfaced via notify */
    } finally {
      setDownloading(false)
    }
  }

  const browserChoice = manual ? MANUAL : chromium.executable
  const channelChoice = channelCustom ? CUSTOM : chromium.cft_channel || 'stable'

  return (
    <div className="grid grid-cols-1 gap-3">
      <h2 className="text-lg font-semibold">{t('setup.chromium.title')}</h2>

      <Field label={t('settings.browser_executable')}>
        {!scanned ? (
          <span className="text-xs text-muted-foreground">{t('common.scanning')}</span>
        ) : (
          <>
            {noneDetected ? (
              <span className="text-xs text-amber-300">{t('setup.chromium.browser_none')}</span>
            ) : (
              <Select
                value={browserChoice}
                onValueChange={(v) => {
                  if (v === MANUAL) {
                    setManual(true)
                  } else {
                    setManual(false)
                    setChromium({ executable: v })
                  }
                }}
              >
                {/* A browser's path can be long (Chrome for Testing's sits deep in
                    the user's config dir): it is cut short with an ellipsis rather
                    than widening the card past the window. */}
                <SelectTrigger className="w-full min-w-0 *:data-[slot=select-value]:min-w-0">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(detected ?? []).map((b) => (
                    <SelectItem key={b.path} value={b.path} title={b.path}>
                      <span className="truncate">
                        {BROWSER_NAMES[b.kind]} — {b.path}
                      </span>
                    </SelectItem>
                  ))}
                  <SelectItem value={MANUAL}>{t('setup.chromium.browser_manual')}</SelectItem>
                </SelectContent>
              </Select>
            )}
            {manual && (
              <Input
                value={chromium.executable}
                onChange={(e) => setChromium({ executable: e.target.value })}
                placeholder={BROWSER_PATH_EXAMPLE}
              />
            )}
            <CheckLine
              check={browserCheck}
              ok={t('setup.chromium.probe_ok')}
              checking={t('setup.chromium.probe_checking')}
              fail={(reason) =>
                // The backend's code for a system that blocks the browser's
                // sandbox (`probe::SANDBOX_BLOCKED`): advice, not a raw error.
                reason === 'sandbox_blocked'
                  ? t('setup.chromium.probe_sandbox')
                  : t('setup.chromium.probe_fail', { reason })
              }
            />
          </>
        )}
      </Field>

      <div className="rounded-md border border-border/50 p-3 grid gap-2">
        <div className="flex items-center justify-between">
          <Label>{t('settings.cft_title')}</Label>
          <span className="text-xs text-muted-foreground">
            {installed === null
              ? t('settings.cft_status_loading')
              : installed.length === 0
                ? t('settings.cft_status_none')
                : t('settings.cft_status_count', { count: installed.length })}
          </span>
        </div>
        <Field label={t('settings.cft_channel')}>
          <Select
            value={channelChoice}
            onValueChange={(v) => {
              if (v === CUSTOM) {
                setChannelCustom(true)
              } else {
                setChannelCustom(false)
                setChromium({ cft_channel: v })
              }
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {CHANNEL_PRESETS.map((c) => (
                <SelectItem key={c} value={c}>
                  {c}
                </SelectItem>
              ))}
              <SelectItem value={CUSTOM}>{t('setup.chromium.channel_custom')}</SelectItem>
            </SelectContent>
          </Select>
          {channelCustom && (
            <>
              <Input
                value={customText}
                onChange={(e) => setCustomText(e.target.value)}
                placeholder={t('setup.chromium.channel_custom_placeholder')}
              />
              <CheckLine
                check={customCheck}
                ok={t('setup.chromium.channel_ok')}
                checking={t('setup.chromium.channel_checking')}
                fail={(reason) => t('setup.chromium.channel_fail', { reason })}
              />
            </>
          )}
        </Field>
        <Button onClick={downloadCft} disabled={downloading || !channelReady} size="sm">
          {downloading ? t('common.downloading') : t('common.download')}
        </Button>
      </div>

      <Field
        label={t('settings.start_url')}
        hint={t('settings.start_url_hint', {
          platform: t(platformInfo(draft.platform.kind).labelKey),
          url: platformInfo(draft.platform.kind).defaultStartUrl,
        })}
      >
        <Input
          value={chromium.start_url}
          onChange={(e) => setChromium({ start_url: e.target.value })}
          placeholder={platformInfo(draft.platform.kind).defaultStartUrl}
        />
      </Field>
    </div>
  )
}

function FinishStep({ draft, firstRun }: { draft: AppConfig; firstRun: boolean }) {
  const { t } = useTranslation()
  const m = draft.capture.mode
  const eff4p = draft.bot.active_4p
  const eff3p = draft.bot.active_3p
  const botLabel = (name: string) => (isNativeBot(name) ? t('bots.native_builtin') : name)
  const botParts = [
    eff4p ? `${botLabel(eff4p)} (4P)` : null,
    eff3p ? `${botLabel(eff3p)} (3P)` : null,
  ].filter((p): p is string => p !== null)
  const botSummary =
    botParts.length > 0 ? botParts.join(', ') : t('setup.finish.bots_none')
  return (
    <div className="grid grid-cols-1 gap-3">
      <h2 className="text-lg font-semibold">{t('setup.finish.title')}</h2>
      <div className="rounded-md border border-border/50 p-3 text-sm">
        <div><b>{t('setup.finish.platform_label')}</b> {t(platformInfo(draft.platform.kind).labelKey)}</div>
        <div><b>{t('setup.finish.mode_label')}</b> {m === 'embedded'
            ? t('setup.finish.mode_embedded')
            : m === 'chromium'
              ? t('setup.finish.mode_chromium')
              : t('setup.finish.mode_mitm')}</div>
        {m === 'mitm' && (
          <>
            <div><b>{t('setup.finish.listen_label')}</b> {draft.proxy.addr}</div>
            <div><b>{t('setup.finish.ca_dir_label')}</b> {draft.proxy.ca_dir}</div>
          </>
        )}
        {m === 'chromium' && (
          <>
            <div>
              <b>{t('setup.finish.exec_label')}</b>{' '}
              {draft.capture.chromium.executable
                ? draft.capture.chromium.executable
                : draft.capture.chromium.force_cft
                  ? t('setup.finish.exec_cft', { channel: draft.capture.chromium.cft_channel || 'stable' })
                  : t('setup.finish.exec_autodetect')}
            </div>
            <div><b>{t('setup.finish.start_url_label')}</b> {draft.capture.chromium.start_url}</div>
            <div><b>{t('setup.finish.cft_channel_label')}</b> {draft.capture.chromium.cft_channel}</div>
          </>
        )}
        <div><b>{t('setup.finish.bots_label')}</b> {botSummary}</div>
      </div>
      {firstRun && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-200">
          {t('setup.finish.autoplay_note')}
        </div>
      )}
      <p className="text-sm text-muted-foreground">
        {t('setup.finish.click_finish_pre')}<b>{t('setup.finish.click_finish_btn')}</b>{t('setup.finish.click_finish_post')}
      </p>
    </div>
  )
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="grid min-w-0 grid-cols-1 gap-1.5">
      <Label>{label}</Label>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  )
}
