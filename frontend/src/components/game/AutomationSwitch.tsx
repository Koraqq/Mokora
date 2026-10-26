// The automation switch at the top left of the play screen. It shows the
// current level; pressing it slides the other two out to its right, and
// choosing one applies it at once (the autoplay switches in the config).
//
//   關閉   — nothing is done for the player; the AI only evaluates
//   自動   — the player's tiles are played
//   全自動 — also starts the next game, with the rest / log-out / delay
//            settings from the settings page
//
// The first time autoplay is turned on, the risk notice comes first. Full auto
// is a paid feature: without it the option carries a lock, choosing it says
// so, and a saved full auto shows (and plays) as auto.

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Bot, ChevronRight, Lock, PowerOff, Repeat } from 'lucide-react'
import { toast } from '@/components/ui/sonner'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useDailyLimitWarning } from '@/components/DailyLimitDialog'
import { useLicense } from '@/hooks/useLicense'
import { useProStatus } from '@/hooks/useProStatus'
import { acknowledgeAutoplayRisk, autoplayRiskAcknowledged } from '@/lib/autoplayRisk'
import { AUTO_MODES, modeInForce, withMode, type AutoMode } from '@/lib/autoplayMode'
import { paidAccess } from '@/lib/license'
import { moduleReady } from '@/lib/pro'
import { invoke } from '@/lib/tauri'
import { cn } from '@/lib/utils'
import { useConfigStore } from '@/stores/configStore'

const ICON: Record<AutoMode, typeof Bot> = { off: PowerOff, auto: Bot, full: Repeat }

// Off is red, auto is the plain light button, full auto is blue. `solid` is the
// button showing the current level; `hint` colours the other options. Auto uses
// foreground/background rather than `primary`, which is red in the red theme.
const TONE: Record<AutoMode, { solid: string; hint: string }> = {
  off: {
    solid: 'bg-red-500 text-white hover:bg-red-500/90',
    hint: 'text-red-400 hover:bg-red-500/15 hover:text-red-300',
  },
  auto: {
    solid: 'bg-foreground text-background hover:bg-foreground/90',
    hint: 'text-foreground/80 hover:bg-muted hover:text-foreground',
  },
  full: {
    solid: 'bg-blue-500 text-white hover:bg-blue-500/90',
    hint: 'text-blue-400 hover:bg-blue-500/15 hover:text-blue-300',
  },
}

export function AutomationSwitch() {
  const { t } = useTranslation()
  const config = useConfigStore((s) => s.config)
  const setConfig = useConfigStore((s) => s.setConfig)
  const [open, setOpen] = useState(false)
  const [asking, setAsking] = useState<AutoMode | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const dailyLimit = useDailyLimitWarning()
  const [license] = useLicense()
  // Unknown until the license answers: nothing is shown locked meanwhile.
  const pro = useProStatus()
  // Full auto runs on the paid module's map of the lobby: paid, and the
  // module here.
  const access = paidAccess(license, pro === null ? null : moduleReady(pro))
  const locked = access === 'locked' || access === 'downloading'
  const lockText = access === 'downloading' ? t('pro.not_ready') : t('license.pro_required')

  // Pressing anywhere else folds the options back in.
  useEffect(() => {
    if (!open) return
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', away)
    return () => document.removeEventListener('pointerdown', away)
  }, [open])

  if (!config) return null
  const current = modeInForce(config, locked)

  const apply = async (mode: AutoMode) => {
    try {
      const next = withMode(config, mode)
      await invoke('update_config', { newConfig: next })
      setConfig(next)
      dailyLimit.check(next)
    } catch (e) {
      toast.error(t('play.mode.failed'), { description: String(e) })
    }
  }

  const choose = (mode: AutoMode) => {
    setOpen(false)
    if (mode === current) return
    if (mode === 'full' && locked) {
      toast.error(lockText)
      return
    }
    if (mode !== 'off' && !autoplayRiskAcknowledged()) {
      setAsking(mode)
      return
    }
    void apply(mode)
  }

  const CurrentIcon = ICON[current]
  const others = AUTO_MODES.filter((m) => m !== current)

  return (
    <>
      <div
        ref={box}
        className="flex items-center rounded-full border border-border bg-muted/40 p-0.5 text-xs"
      >
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          title={t(`play.mode.${current}_hint`)}
          className={cn(
            'flex items-center gap-1.5 rounded-full px-3 py-1 font-medium transition-colors duration-300',
            TONE[current].solid,
          )}
        >
          <CurrentIcon className="h-3.5 w-3.5" />
          {t(`play.mode.${current}`)}
          <ChevronRight
            className={cn('h-3 w-3 transition-transform duration-300', open && 'rotate-180')}
          />
        </button>
        {/* Slides out to the right: the width and the opacity animate together. */}
        <div
          className={cn(
            'flex items-center gap-0.5 overflow-hidden transition-all duration-300 ease-out',
            open ? 'ml-1 max-w-[16rem] opacity-100' : 'pointer-events-none ml-0 max-w-0 opacity-0',
          )}
        >
          {others.map((m, i) => {
            const shut = m === 'full' && locked
            const Icon = shut ? Lock : ICON[m]
            return (
              <button
                key={m}
                type="button"
                tabIndex={open ? 0 : -1}
                onClick={() => choose(m)}
                title={shut ? lockText : t(`play.mode.${m}_hint`)}
                style={{ transitionDelay: open ? `${60 + i * 50}ms` : '0ms' }}
                className={cn(
                  'flex items-center gap-1.5 whitespace-nowrap rounded-full px-3 py-1',
                  'transition-all duration-300',
                  TONE[m].hint,
                  open ? 'translate-x-0 opacity-100' : '-translate-x-2 opacity-0',
                  // After the line above, so it wins over its opacity-100.
                  open && shut && 'opacity-60',
                )}
              >
                <Icon className="h-3.5 w-3.5" />
                {t(`play.mode.${m}`)}
              </button>
            )
          })}
        </div>
      </div>

      {dailyLimit.dialog}

      <Dialog open={asking !== null} onOpenChange={(o) => !o && setAsking(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('settings.autoplay.warning_title')}</DialogTitle>
            <DialogDescription>{t('settings.autoplay.warning_desc')}</DialogDescription>
          </DialogHeader>
          <DialogFooter className="mx-0 mb-0 border-0 bg-transparent p-0">
            <Button variant="outline" size="sm" onClick={() => setAsking(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                acknowledgeAutoplayRisk()
                const mode = asking
                setAsking(null)
                if (mode) void apply(mode)
              }}
            >
              {t('settings.autoplay.warning_ack')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
