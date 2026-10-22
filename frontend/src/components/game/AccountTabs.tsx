// The accounts that are open, as tabs beside the automation switch: one framed
// chip each — "name  in the lobby" with a green light, "name  in a game" with a
// red one, or "name  logging in" with a yellow one — and pressing a chip switches the window to that account (see
// `src/accounts/`). An account that has not been opened in this run has no chip.
//
// Three chips show at a time. With more, an arrow at the right slides the row
// along one chip (and one at the left slides back).

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { toast } from 'sonner'

import { CharacterIcon } from '@/components/game/CharacterIcon'
import { useAccounts } from '@/hooks/useAccounts'
import { accountLabel, type AccountInfo } from '@/lib/accounts'
import { invoke } from '@/lib/tauri'
import { cn } from '@/lib/utils'

const VISIBLE = 3
/** One chip's width plus the gap after it, in rem — the row slides by this. */
const CHIP_REM = 12
const GAP_REM = 0.5

type Phase = 'in_game' | 'lobby' | 'connecting'

const phaseOf = (a: AccountInfo): Phase => (a.in_game ? 'in_game' : a.online ? 'lobby' : 'connecting')

const LIGHT: Record<Phase, string> = {
  in_game: 'bg-red-500 shadow-[0_0_6px_1px_rgb(239_68_68/0.6)]',
  lobby: 'bg-green-500 shadow-[0_0_6px_1px_rgb(34_197_94/0.6)]',
  connecting: 'bg-yellow-400 shadow-[0_0_6px_1px_rgb(250_204_21/0.6)]',
}
const TEXT: Record<Phase, string> = {
  in_game: 'text-red-400',
  lobby: 'text-green-400',
  connecting: 'text-yellow-300',
}

export function AccountTabs() {
  const { t } = useTranslation()
  const { accounts, refresh } = useAccounts()
  const [first, setFirst] = useState(0)

  const open = accounts.filter((a) => a.running)
  if (open.length === 0) return null

  const last = Math.max(0, open.length - VISIBLE)
  const start = Math.min(first, last)
  const shown = Math.min(open.length, VISIBLE)

  const name = (a: AccountInfo) => accountLabel(a, t)

  const pick = async (a: AccountInfo) => {
    if (a.current) return
    try {
      await invoke('switch_account', { id: a.id })
    } catch (e) {
      toast.error(String(e))
    } finally {
      refresh()
    }
  }

  const arrow = (dir: -1 | 1, enabled: boolean) => (
    <button
      type="button"
      disabled={!enabled}
      onClick={() => setFirst(Math.min(last, Math.max(0, start + dir)))}
      className={cn(
        'flex h-7 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-all duration-200',
        enabled ? 'hover:bg-muted hover:text-foreground active:scale-90' : 'opacity-0',
      )}
      aria-label={dir === 1 ? '>' : '<'}
    >
      {dir === 1 ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
    </button>
  )

  return (
    <div className="flex min-w-0 items-center gap-1">
      {open.length > VISIBLE && arrow(-1, start > 0)}
      <div
        className="overflow-hidden"
        style={{ width: `${shown * CHIP_REM + (shown - 1) * GAP_REM}rem` }}
      >
        <div
          className="flex transition-transform duration-300 ease-out"
          style={{
            gap: `${GAP_REM}rem`,
            transform: `translateX(-${start * (CHIP_REM + GAP_REM)}rem)`,
          }}
        >
          {open.map((a) => {
            const phase = phaseOf(a)
            return (
              <button
                key={a.id}
                type="button"
                onClick={() => void pick(a)}
                title={name(a)}
                style={{ width: `${CHIP_REM}rem` }}
                className={cn(
                  'flex h-8 shrink-0 items-center gap-2 rounded-lg border px-2.5 text-left text-xs',
                  'transition-all duration-200 active:scale-[0.98]',
                  a.current
                    ? 'border-primary/70 bg-primary/10'
                    : 'border-border bg-muted/30 hover:border-foreground/30 hover:bg-muted/60',
                )}
              >
                <span
                  className={cn(
                    'h-2 w-2 shrink-0 rounded-full transition-colors duration-300',
                    LIGHT[phase],
                  )}
                />
                <CharacterIcon id={a.character} className="h-5 w-5" />
                <span className="min-w-0 flex-1 truncate">
                  <span className="font-medium">{name(a)}</span>{' '}
                  <span className={cn('transition-colors duration-300', TEXT[phase])}>
                    {t(`play.tabs.${phase}`)}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      </div>
      {open.length > VISIBLE && arrow(1, start < last)}
    </div>
  )
}
