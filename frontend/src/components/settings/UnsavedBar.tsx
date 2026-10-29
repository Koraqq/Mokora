import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'

/** Slides up from the bottom while settings changes wait: how many, cancel
 *  or save. The rest of the app stays locked until one is pressed; a press
 *  there shakes this bar and flashes it red. */
export function UnsavedBar({
  changes,
  saving,
  error,
  onCancel,
  onSave,
  saveDisabled = false,
}: {
  changes: number
  saving: boolean
  error: string | null
  onCancel: () => void
  onSave: () => void
  /** Something still has to be filled in before it can be saved. */
  saveDisabled?: boolean
}) {
  const { t } = useTranslation()
  const nudges = useSettingsGuardStore((s) => s.nudges)
  const [flash, setFlash] = useState(false)

  useEffect(() => {
    if (nudges === 0) return
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setFlash(true)
    const timer = window.setTimeout(() => setFlash(false), 600)
    return () => window.clearTimeout(timer)
  }, [nudges])

  return (
    <div className="sticky bottom-4 z-10 mt-auto flex justify-center animate-in fade-in-0 slide-in-from-bottom-4 duration-200">
      <div
        key={nudges}
        role="alert"
        className={cn(
          'flex w-full max-w-2xl flex-wrap items-center gap-3 rounded-lg border px-4 py-2.5 shadow-lg transition-colors',
          nudges > 0 && 'animate-[settings-nudge_0.4s_ease-in-out]',
          flash
            ? 'border-destructive bg-destructive text-white'
            : 'border-border bg-popover text-popover-foreground',
        )}
      >
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-sm font-medium">{t('settings.unsaved_bar', { count: changes })}</span>
          {error && <span className="text-xs text-red-400">{error}</span>}
        </div>
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={saving}>
          {t('common.cancel')}
        </Button>
        <Button size="sm" onClick={onSave} disabled={saving || saveDisabled}>
          {saving ? t('common.saving') : t('common.save')}
        </Button>
      </div>
    </div>
  )
}
