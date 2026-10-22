// One-day picker for the History page: a select of the days played (newest
// first, `YYYY/MM/DD (count)`, five tall with older days scrolling) and a
// select-all button that shows every day again, lit while it does. Used by
// the game list and by the PT-rule card's page-wide view.

import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { GameDay } from '@/lib/historyDays'
import { cn } from '@/lib/utils'

export function DayPicker({
  days,
  value,
  onChange,
}: {
  days: GameDay[]
  /** The picked day; null for every day. */
  value: string | null
  onChange: (day: string | null) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-1">
      <Select value={value ?? ''} onValueChange={onChange} disabled={days.length === 0}>
        <SelectTrigger
          size="sm"
          className="w-40 text-xs"
          aria-label={t('history.day.label')}
          title={t('history.day.label')}
        >
          <SelectValue placeholder={t('history.day.pick')} />
        </SelectTrigger>
        {/* Five days tall; older days scroll. */}
        <SelectContent position="popper" align="end" className="max-h-37">
          {days.map((d) => (
            <SelectItem key={d.day} value={d.day} className="font-mono text-xs">
              {d.day} ({d.count})
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        type="button"
        size="sm"
        variant="outline"
        aria-pressed={value == null}
        title={t('history.day.all_hint')}
        onClick={() => onChange(null)}
        // Lit while every day shows, like the kind switches.
        className={cn(
          'h-7 px-2 text-xs transition-colors',
          value == null
            ? 'border-primary/70 bg-primary/20 text-foreground hover:bg-primary/30 dark:border-primary/70 dark:bg-primary/25 dark:hover:bg-primary/35'
            : 'text-muted-foreground',
        )}
      >
        {t('history.day.all')}
      </Button>
    </div>
  )
}
