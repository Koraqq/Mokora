// The play screen: the game on the left (a web view laid over the placeholder
// below — see `useGamePlacement`), the AI's read on the right, a top bar with
// the automation switch and the way into the other pages, and a status bar.
//
//   top      automation switch · connection · title · reload, history, logs,
//            settings · account avatar
//   left     the game
//   right    bot status · the AI's recommendation · the deal-in risk table
//   bottom   the models this account runs (left) · the site (right)

import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Megaphone, PictureInPicture2, RefreshCw, Settings as SettingsIcon } from 'lucide-react'

import { AccountAvatar } from '@/components/game/AccountAvatar'
import { AccountTabs } from '@/components/game/AccountTabs'
import { PowerButton } from '@/components/game/PowerButton'
import { AutomationSwitch } from '@/components/game/AutomationSwitch'
import { BotSwitcher } from '@/components/game/BotSwitcher'
import { KeyMenu } from '@/components/game/KeyMenu'
import { SlideBar } from '@/components/game/SlideBar'
import { useAccountId, useToday } from '@/hooks/useAccountId'
import { useGamePlacement } from '@/hooks/useGamePlacement'
import { useOverlayToggle } from '@/hooks/useOverlayToggle'
import {
  dailyStats,
  formatDuration,
  gamesTone,
  todaysGames,
  winRateTone,
} from '@/lib/dailyStats'
import { invoke } from '@/lib/tauri'
import { nativeBotShortName } from '@/lib/nativeBots'
import {
  ASIDE_DEFAULT_RATIO,
  asideWidthFor,
  clampAsideWidth,
  clearAsideShare,
  loadAsideShare,
  saveAsideShare,
} from '@/lib/playLayout'
import { cn } from '@/lib/utils'
import { useConfigStore } from '@/stores/configStore'
import { useAnnouncementStore } from '@/stores/announcementStore'
import { useGameStore } from '@/stores/gameStore'
import { useHistoryStore } from '@/stores/historyStore'
import { BotActionTile } from '@/tiles/BotActionTile'
import { BotShowTile } from '@/tiles/BotShowTile'
import { DangerTile } from '@/tiles/DangerTile'

const WIND_LABEL = { E: '東', S: '南', W: '西', N: '北' } as const

export function Play() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const gameBox = useRef<HTMLDivElement>(null)
  // The game steps aside while the screen fades out.
  const [leaving, setLeaving] = useState(false)
  // The right-hand panel's width, dragged from the bar between it and the game.
  // The game's web view steps aside while the bar is held, so it cannot take the
  // pointer from the page; it is put back at its new size on release.
  // The panel is a share of the window (`playLayout`), so it keeps its
  // proportion when the window is resized or maximized.
  const [asideWidth, setAsideWidth] = useState(() =>
    asideWidthFor(loadAsideShare(), window.innerWidth),
  )
  const [dragging, setDragging] = useState(false)
  useGamePlacement(gameBox, !leaving && !dragging)

  useEffect(() => {
    const fit = () => setAsideWidth(asideWidthFor(loadAsideShare(), window.innerWidth))
    window.addEventListener('resize', fit)
    return () => window.removeEventListener('resize', fit)
  }, [])

  const resizeTo = (width: number, remember: boolean) => {
    const next = clampAsideWidth(width, window.innerWidth)
    setAsideWidth(next)
    if (remember) saveAsideShare(next, window.innerWidth)
  }
  const resetWidth = () => {
    clearAsideShare()
    setAsideWidth(asideWidthFor(ASIDE_DEFAULT_RATIO, window.innerWidth))
  }
  const onBarDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId)
    setDragging(true)
  }
  const onBarMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragging) resizeTo(window.innerWidth - e.clientX, false)
  }
  const onBarUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging) return
    e.currentTarget.releasePointerCapture(e.pointerId)
    setDragging(false)
    saveAsideShare(asideWidth, window.innerWidth)
  }
  const onBarKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 80 : 20
    if (e.key === 'ArrowLeft') resizeTo(asideWidth + step, true)
    else if (e.key === 'ArrowRight') resizeTo(asideWidth - step, true)
    else return
    e.preventDefault()
  }

  // The four buttons fade the screen out first. The game is a web view that
  // cannot fade, so it goes off screen at once (see `useGamePlacement`) and the
  // page fades over it.
  const FADE_MS = 280
  const go = (path: string) => {
    if (leaving) return
    setLeaving(true)
    window.setTimeout(() => navigate(path), FADE_MS)
  }
  const reload = () => {
    if (leaving) return
    setLeaving(true)
    invoke('reload_game').catch(() => {})
    // Back in once the page has had a moment to restart.
    window.setTimeout(() => setLeaving(false), 900)
  }

  const overlay = useOverlayToggle()

  const iconButton = (label: string, onClick: () => void, icon: React.ReactNode) => (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-all duration-200 hover:bg-muted hover:text-foreground active:scale-90"
    >
      {icon}
    </button>
  )

  return (
    <div
      className={cn(
        'flex h-screen flex-col overflow-hidden bg-background text-foreground animate-in fade-in duration-300',
        'transition-opacity ease-out',
        leaving ? 'opacity-0 duration-[280ms]' : 'opacity-100 duration-500',
      )}
    >
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-3">
        <AutomationSwitch />
        <AccountTabs />
        <div className="flex-1" />
        {/* Press and slide across these, like an iPhone tab bar. */}
        <SlideBar className="gap-3">
          {iconButton(t('play.settings'), () => go('/settings'), <SettingsIcon className="h-4 w-4" />)}
          {iconButton(
            t('sidebar.announcements'),
            () => useAnnouncementStore.getState().openHistory(),
            <Megaphone className="h-4 w-4" />,
          )}
          <BotSwitcher minLeft={window.innerWidth - asideWidth} />
          {overlay.available && (
            <button
              type="button"
              title={overlay.label}
              aria-label={overlay.label}
              aria-pressed={overlay.enabled}
              disabled={overlay.busy}
              onClick={() => void overlay.toggle()}
              className={cn(
                'flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-all duration-200 hover:bg-muted hover:text-foreground active:scale-90 disabled:opacity-50',
                overlay.enabled && 'bg-muted text-foreground',
              )}
            >
              <PictureInPicture2 className="h-4 w-4" />
            </button>
          )}
          {iconButton(t('play.reload'), reload, <RefreshCw className="h-4 w-4" />)}
          <PowerButton />
          <KeyMenu minLeft={window.innerWidth - asideWidth} />
          <div className="ml-1">
            <AccountAvatar />
          </div>
        </SlideBar>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* The game's web view sits over this. */}
        <div ref={gameBox} className="min-w-0 flex-1 bg-black" />

        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t('play.resize_panel')}
          aria-valuenow={asideWidth}
          tabIndex={0}
          onPointerDown={onBarDown}
          onPointerMove={onBarMove}
          onPointerUp={onBarUp}
          onPointerCancel={onBarUp}
          onLostPointerCapture={() => setDragging(false)}
          onKeyDown={onBarKey}
          onDoubleClick={resetWidth}
          title={t('play.resize_panel')}
          className={cn(
            'group relative w-1.5 shrink-0 cursor-col-resize touch-none select-none bg-border/40 outline-none transition-colors',
            'hover:bg-primary/40 focus-visible:bg-primary/60',
            dragging && 'bg-primary/60',
          )}
        />

        <aside
          style={{ width: asideWidth }}
          className={cn(
            'flex shrink-0 flex-col gap-3 overflow-y-auto p-3',
            // The tiles are shared with the dashboard; here they are not draggable
            // or closable.
            '[&_.tile-drag-handle]:cursor-default [&_.tile-drag-handle_button]:hidden',
          )}
        >
          <GameInfoCard />
          <DailyCard />
          <div className="h-44 shrink-0 animate-in fade-in slide-in-from-right-2 duration-500">
            <BotActionTile bp="lg" />
          </div>
          <div className="shrink-0 animate-in fade-in slide-in-from-right-2 duration-500">
            <BotShowTile bp="lg" limit={3} title={t('tile.top_discards')} />
          </div>
          {/* As tall as its rows: it does not stretch to the bottom, which is left
              free for more panels. A short window scrolls the column instead. */}
          <div className="shrink-0 animate-in fade-in slide-in-from-right-2 duration-700">
            <DangerTile bp="lg" />
          </div>
        </aside>
      </div>

      <PlayStatusbar />
    </div>
  )
}

/** The round the table is at: wind and hand, honba, riichi sticks, seat wind. */
function GameInfoCard() {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)

  const wind = game ? WIND_LABEL[game.bakaze] : null
  const seatWind =
    game && game.our_seat != null && game.players.length > 0
      ? (['東', '南', '西', '北'][(game.our_seat - game.oya + game.players.length) % game.players.length] ??
        null)
      : null

  const field = (label: string, value: string | null) => (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] text-muted-foreground">{label}</span>
      <span className="text-sm font-medium tabular-nums">{value ?? '—'}</span>
    </div>
  )

  return (
    <section className="rounded-lg border border-border bg-card p-3 animate-in fade-in slide-in-from-right-2 duration-300">
      <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        {t('play.game_info')}
      </h3>
      {game ? (
        <div className="grid grid-cols-4 gap-2">
          {field(t('play.round'), wind ? `${wind}${game.kyoku + 1}` : null)}
          {field(t('play.honba'), String(game.honba))}
          {field(t('play.kyotaku'), String(game.kyotaku))}
          {field(t('play.seat_wind'), seatWind)}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{t('play.waiting_first')}</p>
      )}
    </section>
  )
}

/** This account's games of the day: how many, the first-place rate, the
 *  deal-ins and the longest game. The count and the rate turn warning
 *  colours as they climb; the day starts again at 05:00 UTC+8 (`DAY_START_UTC_HOUR`). */
function DailyCard() {
  const { t } = useTranslation()
  const records = useHistoryStore((s) => s.records)
  const accountId = useAccountId()
  const today = useToday()
  const stats = useMemo(
    () => dailyStats(todaysGames(records, accountId, today)),
    [records, accountId, today],
  )

  const field = (label: string, value: string, tone = '') => (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] text-muted-foreground">{label}</span>
      <span className={cn('text-sm font-medium tabular-nums transition-colors', tone)}>{value}</span>
    </div>
  )

  return (
    <section className="rounded-lg border border-border bg-card p-3 animate-in fade-in slide-in-from-right-2 duration-300">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {t('play.daily.title')}
        </h3>
      </div>
      <div className="grid grid-cols-4 gap-2">
        {field(t('play.daily.games'), String(stats.games), gamesTone(stats.games))}
        {field(
          t('play.daily.win_rate'),
          stats.winRate == null ? '—' : `${Math.round(stats.winRate)}%`,
          stats.winRate == null ? '' : winRateTone(stats.winRate),
        )}
        {field(t('play.daily.deal_ins'), String(stats.dealIns))}
        {field(
          t('play.daily.longest'),
          stats.longestMs == null ? '—' : formatDuration(stats.longestMs),
        )}
      </div>
    </section>
  )
}

/** Left: the models this account runs. Right: the site the game is on. */
function PlayStatusbar() {
  const { t } = useTranslation()
  const config = useConfigStore((s) => s.config)
  const [site, setSite] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    const read = () =>
      invoke<string | null>('game_site')
        .then((s) => alive && setSite(s))
        .catch(() => {})
    read()
    const timer = window.setInterval(read, 2000)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [])

  const botName = (name: string | undefined) => {
    if (config && !config.bot.enabled) return t('status.bot_off')
    if (!name) return '—'
    return nativeBotShortName(name) ?? name
  }

  return (
    <footer className="flex h-7 shrink-0 items-center justify-between gap-6 border-t border-border bg-muted/30 px-4 text-xs text-muted-foreground">
      <div className="flex items-center gap-5">
        <span>{t('status.bot_3p', { name: botName(config?.bot.active_3p) })}</span>
        <span>{t('status.bot_4p', { name: botName(config?.bot.active_4p) })}</span>
      </div>
      <span className="truncate transition-opacity duration-300">{site ?? ''}</span>
    </footer>
  )
}
