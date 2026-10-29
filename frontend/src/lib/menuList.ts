import {
  Bot,
  Gamepad2,
  Palette,
  MonitorPlay,
  History as HistoryIcon,
  ScrollText,
  SearchCheck,
  Settings as SettingsIcon,
  UserRound,
  type LucideIcon,
} from 'lucide-react'

// `t` is i18next's TFunction — we keep the parameter typed loosely so this
// module doesn't pull in i18next types just to declare a signature.
type Translate = (key: string) => string

export type Submenu = {
  href: string
  label: string
  active?: boolean
}

export type MenuItem = {
  /** Stable name of the page, what the saved order is made of. */
  id: string
  href: string
  label: string
  icon: LucideIcon
  active?: boolean
  submenus?: Submenu[]
}

export type MenuGroup = {
  groupLabel: string
  menus: MenuItem[]
}

/** The sidebar's pages in their default order. */
export const DEFAULT_MENU_ORDER: readonly string[] = [
  'play',
  'profile',
  'game',
  'history',
  'bots',
  'appearance',
  'review',
  'settings',
  'logs',
]

/** The default before 0.1.2's own: a kept order that is exactly this was
 *  never chosen, so it moves to the current default. */
export const LEGACY_MENU_ORDER: readonly string[] = [
  'play',
  'game',
  'bots',
  'profile',
  'history',
  'review',
  'logs',
  'appearance',
  'settings',
]

/** A saved order made whole: only pages that exist, once each, and any page
 *  the order does not name (a newer one) after the rest, in default order. */
export function normalizeMenuOrder(order: readonly string[] | null | undefined): string[] {
  const known = new Set(DEFAULT_MENU_ORDER)
  const seen = new Set<string>()
  const kept = (order ?? []).filter((id) => known.has(id) && !seen.has(id) && !!seen.add(id))
  return [...kept, ...DEFAULT_MENU_ORDER.filter((id) => !seen.has(id))]
}

/**
 * Move one page a place up or down among the pages that are shown. Pages that
 * are not shown (the log page, outside developer mode) keep their own places.
 */
export function moveMenuItem(
  order: readonly string[],
  visible: readonly string[],
  id: string,
  direction: -1 | 1,
): string[] {
  const full = normalizeMenuOrder(order)
  const shown = full.filter((item) => visible.includes(item))
  const at = shown.indexOf(id)
  const to = at + direction
  if (at < 0 || to < 0 || to >= shown.length) return full
  ;[shown[at], shown[to]] = [shown[to], shown[at]]
  let next = 0
  return full.map((item) => (visible.includes(item) ? shown[next++] : item))
}

/** The pages in the order given. */
export function orderMenus(menus: MenuItem[], order: readonly string[] | null | undefined): MenuItem[] {
  const rank = new Map(normalizeMenuOrder(order).map((id, i) => [id, i]))
  return [...menus].sort((a, b) => (rank.get(a.id) ?? 0) - (rank.get(b.id) ?? 0))
}

/** The sidebar's pages. The log page is for developers: it shows only in
 *  developer mode (Settings → Advanced). */
export function getMenuList(
  t: Translate,
  developerMode = false,
  order?: readonly string[] | null,
): MenuGroup[] {
  return [
    {
      groupLabel: '',
      menus: [
        { id: 'play', href: '/play', label: t('nav.play'), icon: MonitorPlay },
        { id: 'game', href: '/game', label: t('nav.game'), icon: Gamepad2 },
        { id: 'bots', href: '/bots', label: t('nav.bots'), icon: Bot },
        { id: 'profile', href: '/profile', label: t('nav.profile'), icon: UserRound },
        { id: 'history', href: '/history', label: t('nav.history'), icon: HistoryIcon },
        { id: 'review', href: '/review', label: t('nav.review'), icon: SearchCheck },
        ...(developerMode ? [{ id: 'logs', href: '/logs', label: t('nav.logs'), icon: ScrollText }] : []),
        { id: 'appearance', href: '/appearance', label: t('nav.appearance'), icon: Palette },
        { id: 'settings', href: '/settings', label: t('nav.settings'), icon: SettingsIcon },
      ],
    },
  ].map((group) => ({ ...group, menus: orderMenus(group.menus, order) }))
}
