import { type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Move, Maximize2, X, Plus, RotateCcw } from 'lucide-react'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

type Props = {
  open: boolean
  onOpenChange: (open: boolean) => void
}

// One-time welcome hint shown the first time the Game dashboard is opened.
// Explains the four ways to customize the tile grid, using the same icons the
// real UI exposes so the copy lines up with what the user sees on screen.
export function DashboardOnboardingDialog({ open, onOpenChange }: Props) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('game.onboarding.title')}</DialogTitle>
          <DialogDescription>{t('game.onboarding.intro')}</DialogDescription>
        </DialogHeader>

        <ul className="flex flex-col gap-3 py-1">
          <HintRow
            icon={<Move className="size-4" />}
            title={t('game.onboarding.drag_title')}
            desc={t('game.onboarding.drag_desc')}
          />
          <HintRow
            icon={<Maximize2 className="size-4" />}
            title={t('game.onboarding.resize_title')}
            desc={t('game.onboarding.resize_desc')}
          />
          <HintRow
            icon={<X className="size-4" />}
            title={t('game.onboarding.delete_title')}
            desc={t('game.onboarding.delete_desc')}
          />
          <HintRow
            icon={<Plus className="size-4" />}
            title={t('game.onboarding.add_title')}
            desc={t('game.onboarding.add_desc')}
          />
        </ul>

        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <RotateCcw className="size-3.5 shrink-0" />
          {t('game.onboarding.reset_hint')}
        </p>

        <DialogFooter>
          <DialogClose asChild>
            <Button>{t('game.onboarding.got_it')}</Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function HintRow({ icon, title, desc }: { icon: ReactNode; title: string; desc: string }) {
  return (
    <li className="flex items-start gap-3">
      <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-foreground">
        {icon}
      </span>
      <div className="min-w-0">
        <div className="text-sm font-medium">{title}</div>
        <div className="text-xs text-muted-foreground">{desc}</div>
      </div>
    </li>
  )
}
