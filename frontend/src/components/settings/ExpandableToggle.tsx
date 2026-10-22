// One feature of a settings card as a row: its name, its switch, and at the
// far right a chevron that unfolds the feature's own settings below. The
// settings can be changed whether the switch is on or off, so they can be set
// up before the feature starts acting.
//
// A locked row (a paid feature without the membership, or with the paid
// module still on its way) shows a lock and the reason, and nothing in it can
// be pressed: neither the switch nor the chevron, and its settings stay
// folded away.

import { useId, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, Lock } from 'lucide-react'

import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'

export function ExpandableToggle({
  label,
  checked,
  onCheckedChange,
  locked = false,
  lockReason,
  children,
}: {
  label: string
  checked: boolean
  onCheckedChange: (on: boolean) => void
  /** Nothing can be pressed; `lockReason` says why. */
  locked?: boolean
  lockReason?: string
  /** The feature's settings, shown when unfolded. */
  children: ReactNode
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const id = useId()
  const shown = open && !locked
  return (
    <div className={cn('rounded-md border', locked && 'opacity-60')} title={locked ? lockReason : undefined}>
      <div className="flex items-center gap-3 px-3 py-2.5">
        <Label htmlFor={`${id}-switch`} className="flex flex-1 items-center gap-2">
          {locked && <Lock className="h-3.5 w-3.5 text-muted-foreground" aria-label={lockReason} />}
          {label}
        </Label>
        <Switch
          id={`${id}-switch`}
          checked={checked}
          disabled={locked}
          onCheckedChange={onCheckedChange}
        />
        <button
          type="button"
          disabled={locked}
          aria-expanded={shown}
          aria-controls={`${id}-settings`}
          aria-label={shown ? t('settings.automation.collapse') : t('settings.automation.expand')}
          onClick={() => setOpen((o) => !o)}
          className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none"
        >
          <ChevronDown className={cn('h-4 w-4 transition-transform duration-200', shown && 'rotate-180')} />
        </button>
      </div>
      {shown && (
        <div id={`${id}-settings`} className="grid gap-4 border-t px-3 py-3 animate-in fade-in slide-in-from-top-1 duration-200">
          {children}
        </div>
      )}
    </div>
  )
}
