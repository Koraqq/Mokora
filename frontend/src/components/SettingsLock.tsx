import type { ReactNode } from 'react'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'
import { cn } from '@/lib/utils'

// Wraps a part of the app outside the settings page (sidebar, top bar,
// status bar). While settings changes wait to be saved or cancelled, a press
// on it does nothing but nudge the settings bar. `display: contents` keeps
// the wrapper out of the layout; presses still pass through it.
export function SettingsLock({
  children,
  exempt = false,
}: {
  children: ReactNode
  /** Leave this part usable while changes wait (the sidebar, as its order is set). */
  exempt?: boolean
}) {
  const waiting = useSettingsGuardStore((s) => s.changes > 0)
  const locked = waiting && !exempt
  const nudge = useSettingsGuardStore((s) => s.nudge)

  const swallow = (e: React.SyntheticEvent) => {
    if (!locked) return
    e.preventDefault()
    e.stopPropagation()
  }

  return (
    <div
      className={cn('contents', locked && '[&_*]:cursor-not-allowed')}
      aria-disabled={locked || undefined}
      onPointerDownCapture={(e) => {
        swallow(e)
        if (locked) nudge()
      }}
      onMouseDownCapture={swallow}
      onClickCapture={swallow}
      onKeyDownCapture={swallow}
    >
      {children}
    </div>
  )
}
