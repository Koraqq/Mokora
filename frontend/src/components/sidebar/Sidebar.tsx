import { useEffect } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ChevronLeft, X } from 'lucide-react'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { useSidebar, useSidebarSettings } from '@/hooks/useSidebar'
import { useIsNarrow } from '@/hooks/useIsNarrow'
import { MokoraIcon, MokoraWordmark } from '@/components/BrandLogo'
import { Menu } from './Menu'

// The sidebar is the logo and the pages, nothing else: accounts are switched
// from the avatar on the play screen, the version and the release notes are in
// the announcements, and the language is in Settings.
export function Sidebar() {
  const { t } = useTranslation()
  const isOpen = useSidebar((s) => s.isOpen)
  const toggleOpen = useSidebar((s) => s.toggleOpen)
  const setIsHover = useSidebar((s) => s.setIsHover)
  const isHover = useSidebar((s) => s.isHover)
  const ordering = useSidebar((s) => s.ordering)
  const settings = useSidebarSettings()
  const isDrawerOpen = useSidebar((s) => s.isDrawerOpen)
  const setDrawerOpen = useSidebar((s) => s.setDrawerOpen)
  const isNarrow = useIsNarrow()
  // `key`, not `pathname`: clicking the nav item for the route you're already
  // on pushes a new history entry without changing the path, and the drawer
  // still has to get out of the way.
  const { key: locationKey } = useLocation()
  // Navigating closes the drawer — otherwise it would sit on top of the very
  // page the user just asked for.
  useEffect(() => {
    setDrawerOpen(false)
  }, [locationKey, setDrawerOpen])

  // Growing back past `lg` re-docks the sidebar. Drop the drawer state so the
  // backdrop can't linger, and so shrinking again starts from closed.
  useEffect(() => {
    if (!isNarrow) setDrawerOpen(false)
  }, [isNarrow, setDrawerOpen])

  useEffect(() => {
    if (!isNarrow || !isDrawerOpen) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setDrawerOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [isNarrow, isDrawerOpen, setDrawerOpen])

  // `open` includes the transient hover-open state. Only `isOpen` (pinned)
  // affects main content margin in App.tsx — hover-open expands the sidebar
  // visually as an overlay above main, so width-sensitive widgets like
  // react-grid-layout don't thrash on every cursor pass.
  //
  // As a drawer it is always fully expanded: a 5.625rem rail of bare icons is
  // a pointless middle state for something that's already an overlay.
  // While the order is being set the sidebar stays open, whatever it is set to.
  const open = isNarrow || isOpen || ordering || (settings.isHoverOpen && isHover)
  const showBackdrop = isNarrow && isDrawerOpen && !settings.disabled

  return (
    <>
      {showBackdrop && (
        <div
          className="fixed inset-0 z-30 bg-black/50"
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      )}
      <aside
        // Off-screen drawer contents stay in the tree (so the slide-out
        // animates) but must not be reachable by Tab or screen readers.
        inert={isNarrow && !isDrawerOpen}
        className={cn(
          'fixed top-0 left-0 h-screen transition-[width,transform] ease-in-out duration-300',
          isNarrow
            ? cn('z-40 w-[18rem]', isDrawerOpen ? 'translate-x-0' : '-translate-x-full')
            : cn('z-20 translate-x-0', open ? 'w-[18rem]' : 'w-[5.625rem]'),
          settings.disabled && !ordering && 'hidden',
        )}
      >
        <div
          // Hover-peek is a docked-sidebar affordance. In drawer mode the
          // element is off-screen, so hovering it is meaningless.
          onMouseEnter={() => !isNarrow && setIsHover(true)}
          onMouseLeave={() => !isNarrow && setIsHover(false)}
          className="relative h-full flex flex-col px-3 py-4 overflow-hidden bg-sidebar text-sidebar-foreground border-r border-border shadow-md dark:shadow-zinc-800"
        >
        <div
          className={cn(
            'flex items-center mb-2 shrink-0',
            open ? 'justify-between gap-2' : 'justify-center',
          )}
        >
          <Link
            to="/"
            className={cn(
              'flex items-center gap-2 rounded-md px-1 py-1 hover:opacity-80 transition-opacity',
              !open && 'justify-center',
            )}
            aria-label="Mokora"
          >
            {open ? (
              <span className="flex items-center gap-1.5 whitespace-nowrap">
                <MokoraWordmark className="h-7" />
              </span>
            ) : (
              <MokoraIcon className="h-7" />
            )}
          </Link>
          {open &&
            !ordering &&
            (isNarrow ? (
              // Pinning has no meaning for an overlay — the only useful
              // action here is dismissing it.
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setDrawerOpen(false)}
                className="h-7 w-7 text-muted-foreground hover:text-foreground"
                aria-label={t('sidebar.closeMenu')}
              >
                <X className="h-4 w-4" />
              </Button>
            ) : (
              <Button
                variant="ghost"
                size="icon"
                onClick={toggleOpen}
                className="h-7 w-7 text-muted-foreground hover:text-foreground"
                aria-label={isOpen ? t('sidebar.collapse') : t('sidebar.pin')}
              >
                <ChevronLeft
                  className={cn(
                    'h-4 w-4 transition-transform duration-300',
                    !isOpen && 'rotate-180',
                  )}
                />
              </Button>
            ))}
        </div>
        <Menu isOpen={open} />
        </div>
      </aside>
    </>
  )
}
