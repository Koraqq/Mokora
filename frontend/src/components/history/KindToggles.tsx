// Friendly / event switches for a History card: neither kind earns rank
// points, so a card leaves them out unless its switch is pressed. Sits in the
// card header beside the bot tabs; a pressed switch lights up in the accent.

import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import type { KindView } from '@/lib/historyKinds'
import { cn } from '@/lib/utils'

export function KindToggles({
  value,
  onChange,
}: {
  value: KindView
  onChange: (v: KindView) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-1">
      {(['friendly', 'event'] as const).map((kind) => (
        <Button
          key={kind}
          type="button"
          size="sm"
          variant="outline"
          aria-pressed={value[kind]}
          title={t(`history.kind.${kind}_hint`)}
          onClick={() => onChange({ ...value, [kind]: !value[kind] })}
          // Off reads dimmed; on lights up in the accent, like a switch.
          className={cn(
            'h-7 px-2 text-xs transition-colors',
            value[kind]
              ? 'border-primary/70 bg-primary/20 text-foreground hover:bg-primary/30 dark:border-primary/70 dark:bg-primary/25 dark:hover:bg-primary/35'
              : 'text-muted-foreground',
          )}
        >
          {t(`history.kind.${kind}`)}
        </Button>
      ))}
    </div>
  )
}
