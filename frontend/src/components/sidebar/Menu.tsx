import { Link, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ChevronDown, ChevronUp, Ellipsis } from 'lucide-react'

import { cn } from '@/lib/utils'
import { getMenuList, moveMenuItem } from '@/lib/menuList'
import { useMenuOrder, useSidebar } from '@/hooks/useSidebar'
import { useUiPrefsStore } from '@/stores/uiPrefsStore'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'

interface MenuProps {
  isOpen: boolean
}

// `/` is special-cased in `isActive` because every path starts with "/" — a
// naive `startsWith` would mark Overview active on every route.
function isItemActive(pathname: string, href: string): boolean {
  if (href === '/') return pathname === '/'
  return pathname === href || pathname.startsWith(`${href}/`)
}

export function Menu({ isOpen }: MenuProps) {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  const developerMode = useUiPrefsStore((s) => s.developerMode)
  const order = useMenuOrder()
  const menuList = getMenuList(t, developerMode, order)
  const ordering = useSidebar((s) => s.ordering)
  const picked = useSidebar((s) => s.picked)
  const setPicked = useSidebar((s) => s.setPicked)
  const setPreviewOrder = useSidebar((s) => s.setPreviewOrder)
  const shown = menuList.flatMap((group) => group.menus.map((menu) => menu.id))
  const move = (id: string, direction: -1 | 1) =>
    setPreviewOrder(moveMenuItem(order, shown, id, direction))

  return (
    // `flex-1 min-h-0` lets ScrollArea claim whatever vertical space the
    // parent flex column has left over. Without `min-h-0`, flex items
    // refuse to shrink below their content size and the sidebar overflows
    // the viewport. The upstream registry pinned the menu height with a
    // `min-h-[calc(100vh-...)]` so it could push a Sign Out button to the
    // bottom — we don't have one, so the natural height is correct.
    <ScrollArea className="flex-1 min-h-0 [&>div>div[style]]:!block">
      <nav className="mt-4 h-full w-full">
        <ul className="flex flex-col items-start space-y-1 px-2">
          {menuList.map(({ groupLabel, menus }, index) => (
            <li className={cn('w-full', groupLabel ? 'pt-5' : '')} key={index}>
              {isOpen && groupLabel ? (
                <p className="text-sm font-medium text-muted-foreground px-4 pb-2 max-w-[15.5rem] truncate">
                  {groupLabel}
                </p>
              ) : !isOpen && groupLabel ? (
                <TooltipProvider>
                  <Tooltip delayDuration={100}>
                    <TooltipTrigger className="w-full">
                      <div className="w-full flex justify-center items-center">
                        <Ellipsis className="h-5 w-5" />
                      </div>
                    </TooltipTrigger>
                    <TooltipContent side="right">
                      <p>{groupLabel}</p>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              ) : (
                <p className="pb-2"></p>
              )}
              {menus.map(({ id, href, label, icon: Icon, active }, i) => {
                if (ordering) {
                  // Setting the order: pages are picked, not opened.
                  const isPicked = picked === id
                  return (
                    <div className="mb-1 flex w-full items-center gap-1" key={id}>
                      <Button
                        variant={isPicked ? 'secondary' : 'ghost'}
                        aria-pressed={isPicked}
                        onClick={() => setPicked(isPicked ? null : id)}
                        className={cn(
                          'h-10 min-w-0 flex-1 justify-start',
                          isPicked && 'ring-1 ring-primary',
                        )}
                      >
                        <span className="mr-4 shrink-0">
                          <Icon size={18} />
                        </span>
                        <p className="max-w-[9rem] truncate">{label}</p>
                      </Button>
                      {isPicked && (
                        <>
                          <Button
                            variant="outline"
                            size="icon"
                            className="h-8 w-8 shrink-0"
                            aria-label={t('settings.look.menu_up')}
                            disabled={i === 0 && index === 0}
                            onClick={() => move(id, -1)}
                          >
                            <ChevronUp className="h-4 w-4" />
                          </Button>
                          <Button
                            variant="outline"
                            size="icon"
                            className="h-8 w-8 shrink-0"
                            aria-label={t('settings.look.menu_down')}
                            disabled={i === menus.length - 1 && index === menuList.length - 1}
                            onClick={() => move(id, 1)}
                          >
                            <ChevronDown className="h-4 w-4" />
                          </Button>
                        </>
                      )}
                    </div>
                  )
                }
                const itemActive =
                  active === undefined ? isItemActive(pathname, href) : active
                return (
                  <div className="w-full" key={i}>
                    <TooltipProvider disableHoverableContent>
                      <Tooltip delayDuration={100}>
                        <TooltipTrigger asChild>
                          <Button
                            variant={itemActive ? 'secondary' : 'ghost'}
                            className={cn(
                              'w-full h-10 mb-1',
                              // Collapsed: center the icon. Open: left-align so
                              // icon + label stay visually anchored.
                              isOpen ? 'justify-start' : 'justify-center',
                            )}
                            asChild
                          >
                            <Link to={href}>
                              <span className={cn('shrink-0', isOpen && 'mr-4')}>
                                <Icon size={18} />
                              </span>
                              {/* Hide the label entirely when collapsed —
                                  rendering it (even invisible) would push the
                                  icon off-center. */}
                              {isOpen && (
                                <p className="max-w-[12.5rem] truncate">
                                  {label}
                                </p>
                              )}
                            </Link>
                          </Button>
                        </TooltipTrigger>
                        {!isOpen && (
                          <TooltipContent side="right">{label}</TooltipContent>
                        )}
                      </Tooltip>
                    </TooltipProvider>
                  </div>
                )
              })}
            </li>
          ))}
        </ul>
      </nav>
    </ScrollArea>
  )
}
