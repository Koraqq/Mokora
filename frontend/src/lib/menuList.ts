import {
  Bot,
  Gamepad2,
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

/** The sidebar's pages. The log page is for developers: it shows only in
 *  developer mode (Settings → Advanced). */
export function getMenuList(t: Translate, developerMode = false): MenuGroup[] {
  return [
    {
      groupLabel: '',
      menus: [
        { href: '/play', label: t('nav.play'), icon: MonitorPlay },
        { href: '/game', label: t('nav.game'), icon: Gamepad2 },
        { href: '/bots', label: t('nav.bots'), icon: Bot },
        { href: '/profile', label: t('nav.profile'), icon: UserRound },
        { href: '/history', label: t('nav.history'), icon: HistoryIcon },
        { href: '/review', label: t('nav.review'), icon: SearchCheck },
        ...(developerMode ? [{ href: '/logs', label: t('nav.logs'), icon: ScrollText }] : []),
        { href: '/settings', label: t('nav.settings'), icon: SettingsIcon },
      ],
    },
  ]
}
