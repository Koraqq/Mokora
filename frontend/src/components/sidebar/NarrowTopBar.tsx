import { useTranslation } from 'react-i18next'
import { Menu as MenuIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { MokoraWordmark } from '@/components/BrandLogo'
import { useSidebar, useSidebarSettings } from '@/hooks/useSidebar'
import { useIsNarrow } from '@/hooks/useIsNarrow'

/**
 * The only way to reach navigation below the `lg` breakpoint, where the
 * sidebar can't be docked and lives as an overlay drawer instead.
 *
 * Rendered in the document flow at the top of `<main>` rather than floating
 * over it: a `fixed` trigger would cover the page headers every route already
 * draws, and the 3rem it costs is only paid in narrow mode.
 */
export function NarrowTopBar() {
  const { t } = useTranslation()
  const isNarrow = useIsNarrow()
  const settings = useSidebarSettings()
  const setDrawerOpen = useSidebar((s) => s.setDrawerOpen)

  // Wide enough to dock the sidebar, or the user disabled it outright — in
  // both cases there is nothing to trigger.
  if (!isNarrow || settings.disabled) return null

  return (
    <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-background px-2">
      <Button
        variant="ghost"
        size="icon"
        onClick={() => setDrawerOpen(true)}
        aria-label={t('sidebar.openMenu')}
      >
        <MenuIcon className="h-5 w-5" />
      </Button>
      <MokoraWordmark className="h-6" />
    </div>
  )
}
