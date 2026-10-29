import type { CustomTheme, ThemeMode, ThemePalette } from '@/stores/themeStore'

/** What the appearance page edits. It is a draft: nothing is kept until Save
 *  (the page only previews it). */
export type AppearanceDraft = {
  themeMode: ThemeMode
  themePalette: ThemePalette
  customTheme: CustomTheme
  uiScale: number
  /** The order of the sidebar's pages (see `lib/menuList`). */
  menuOrder: string[]
  /** The size buttons the player added, in percent. */
  scalePresets: number[]
  /** Every account shows this look; turning it off gives each its own back. */
  sharedAppearance: boolean
  sidebarHoverOpen: boolean
  sidebarDisabled: boolean
  /** Ctrl + / Ctrl - change the interface size straight away. */
  zoomShortcut: boolean
}
