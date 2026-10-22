// The bot button in the play screen's top bar: a quick way to change the model
// without going to Settings -> Bots. Pointing at it (or clicking it) opens a
// small panel with two rows, "3p bot" and "4p bot", each with how many bots it
// holds; pointing at or clicking a row lists the bots for that player count on
// its right by their short names, and clicking a name makes it the active one.
//
// The panel opens centred under the button and grows downward, kept inside the
// right-hand column (`minLeft` is where that column starts): the game's web
// view paints above the page, so anything reaching over the game would be
// hidden behind it. A change applies from the next game (the bot manager reads
// the setting when a game starts).
//
// Yakuman is a paid feature, its planner in the paid module: until both are
// there its rows carry a lock, choosing one says so (or that the module is
// still on its way), and a saved Yakuman shows (and plays) as Mora.

import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Bot, Check, ChevronRight, Lock } from 'lucide-react'
import { toast } from 'sonner'

import { useLicense } from '@/hooks/useLicense'
import { useProStatus } from '@/hooks/useProStatus'
import { commandError, hasPro } from '@/lib/license'
import { moduleReady } from '@/lib/pro'
import { invoke } from '@/lib/tauri'
import { botInForce, isNativeBot, nativeBotShortName, nativeModes } from '@/lib/nativeBots'
import { placePanel } from '@/lib/playLayout'
import { cn } from '@/lib/utils'
import { useBotStore } from '@/stores/botStore'
import { useConfigStore } from '@/stores/configStore'
import type { AppConfig, BotInfo } from '@/types'

type Mode = '3p' | '4p'

const MODES: Mode[] = ['3p', '4p']
/** The panel's width when the column has room for it (the bot names column is
 *  what shrinks in a narrow one). */
const PANEL_WIDTH = 304
/** How long the panel stays after the pointer leaves it, so a short slip
 *  between the button and the panel does not close it. */
const LEAVE_MS = 250

export function BotSwitcher({ minLeft }: { minLeft: number }) {
  const { t } = useTranslation()
  const list = useBotStore((s) => s.list)
  const setList = useBotStore((s) => s.setList)
  const config = useConfigStore((s) => s.config)
  const setConfig = useConfigStore((s) => s.setConfig)
  const [open, setOpen] = useState(false)
  const [mode, setMode] = useState<Mode>('3p')
  // The panel's left edge relative to the button's box, and its width.
  const [place, setPlace] = useState({ left: 0, width: PANEL_WIDTH })
  const box = useRef<HTMLDivElement>(null)
  const leaveTimer = useRef<number | undefined>(undefined)
  const [license] = useLicense()
  const pro = useProStatus()
  // Unknown until both answer: nothing is shown locked meanwhile.
  const licensed = license === null || hasPro(license)
  const paid = licensed && (pro === null || moduleReady(pro))
  const lockText = licensed ? t('pro.not_ready') : t('license.pro_required')

  const load = useCallback(async () => {
    try {
      const [bots, cfg] = await Promise.all([
        invoke<BotInfo[]>('list_bots'),
        invoke<AppConfig>('get_config'),
      ])
      setList(bots)
      setConfig(cfg)
    } catch {
      /* the backend's notify event surfaces it */
    }
  }, [setList, setConfig])

  const show = () => {
    window.clearTimeout(leaveTimer.current)
    if (!open) {
      void load()
      const r = box.current?.getBoundingClientRect()
      if (r) {
        const p = placePanel(r.left + r.width / 2, window.innerWidth, minLeft, PANEL_WIDTH)
        setPlace({ left: p.left - r.left, width: p.width })
      }
    }
    setOpen(true)
  }
  const hideSoon = () => {
    window.clearTimeout(leaveTimer.current)
    leaveTimer.current = window.setTimeout(() => setOpen(false), LEAVE_MS)
  }

  // A click outside, or Escape, closes it at once.
  useEffect(() => {
    if (!open) return
    const outside = (e: PointerEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false)
    }
    const escape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    document.addEventListener('pointerdown', outside)
    document.addEventListener('keydown', escape)
    return () => {
      document.removeEventListener('pointerdown', outside)
      document.removeEventListener('keydown', escape)
    }
  }, [open])
  useEffect(() => () => window.clearTimeout(leaveTimer.current), [])

  const activeOf = (m: Mode) =>
    botInForce((m === '3p' ? config?.bot.active_3p : config?.bot.active_4p) ?? '', paid)
  const shut = (name: string) => botInForce(name, paid) !== name

  // The row already says the player count, so a built-in bot goes by its
  // short name ("Mora", "Yakuman").
  const label = (bot: BotInfo): string =>
    (isNativeBot(bot.name) ? nativeBotShortName(bot.name) : null) ??
    bot.manifest?.bot.display ??
    bot.name

  const supports = (bot: BotInfo, m: Mode): boolean =>
    isNativeBot(bot.name)
      ? nativeModes(bot.name).includes(m)
      : (bot.manifest?.bot.supported_modes ?? ['4p']).includes(m)

  const choicesFor = (m: Mode) =>
    list.filter((b) => supports(b, m) && (isNativeBot(b.name) || b.env_ready))
  const choices = choicesFor(mode)

  const pick = async (m: Mode, name: string) => {
    if (activeOf(m) === name) return
    if (shut(name)) {
      toast.error(lockText)
      return
    }
    // Flip at once so the check follows the click; `load` below backfills the
    // real value if the call fails.
    if (config) {
      setConfig({ ...config, bot: { ...config.bot, [m === '3p' ? 'active_3p' : 'active_4p']: name } })
    }
    try {
      await invoke('set_active_bot', { mode: m, name })
    } catch (e) {
      toast.error(t('bots.activate_failed'), { description: commandError(e, t) })
    } finally {
      void load()
    }
  }

  return (
    <div
      ref={box}
      className="relative"
      onMouseEnter={show}
      onMouseLeave={hideSoon}
    >
      <button
        type="button"
        title={t('play.bot_switch')}
        aria-label={t('play.bot_switch')}
        aria-expanded={open}
        onClick={() => (open ? setOpen(false) : show())}
        className={cn(
          'flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-all duration-200 hover:bg-muted hover:text-foreground active:scale-90',
          open && 'bg-muted text-foreground',
        )}
      >
        <Bot className="h-4 w-4" />
      </button>
      {open && (
        <div
          style={{ left: place.left, width: place.width }}
          className="absolute top-full z-50 flex rounded-md border border-border bg-popover text-popover-foreground shadow-md animate-in fade-in duration-150"
          role="menu"
        >
          <div className="flex w-28 shrink-0 flex-col p-1">
            {MODES.map((m) => (
              <button
                key={m}
                type="button"
                role="menuitem"
                onMouseEnter={() => setMode(m)}
                onClick={() => setMode(m)}
                className={cn(
                  'flex items-center justify-between rounded px-2 py-1.5 text-sm transition-colors',
                  mode === m ? 'bg-muted text-foreground' : 'text-muted-foreground hover:bg-muted/60',
                )}
              >
                <span>{t(`play.bot_${m}`)}</span>
                <span className="ml-auto mr-1 text-xs tabular-nums text-muted-foreground">
                  {choicesFor(m).length}
                </span>
                <ChevronRight className="h-3.5 w-3.5 shrink-0" />
              </button>
            ))}
          </div>
          <div className="flex min-w-0 flex-1 flex-col border-l border-border p-1">
            {choices.length === 0 && (
              <span className="flex min-h-[4.5rem] flex-1 items-center justify-center px-2 text-center text-xs text-muted-foreground">
                {t('play.bot_none')}
              </span>
            )}
            {choices.map((b) => {
              const active = activeOf(mode) === b.name
              return (
                <button
                  key={b.name}
                  type="button"
                  role="menuitemradio"
                  aria-checked={active}
                  onClick={() => void pick(mode, b.name)}
                  title={shut(b.name) ? lockText : undefined}
                  className={cn(
                    'flex items-center justify-between gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted',
                    active && 'text-foreground',
                    shut(b.name) && 'opacity-60',
                  )}
                >
                  <span className="truncate">{label(b)}</span>
                  {shut(b.name) ? (
                    <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  ) : (
                    active && <Check className="h-3.5 w-3.5 shrink-0" />
                  )}
                </button>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
