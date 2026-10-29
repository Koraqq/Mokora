import { useEffect } from 'react'
import { create } from 'zustand'

import { HAS_TAURI, invoke, listen } from '@/lib/tauri'
import { useSidebar } from '@/hooks/useSidebar'
import { normalizeMenuOrder } from '@/lib/menuList'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'
import {
  SCALE_DEFAULT,
  SCALE_MAX,
  SCALE_MIN,
  SCALE_STEP,
  cleanScalePresets,
  useUiPrefsStore,
} from '@/stores/uiPrefsStore'
import {
  previewTheme,
  useThemeStore,
  type CustomTheme,
  type ThemeMode,
  type ThemePalette,
} from '@/stores/themeStore'

/** What the player sees: everything the appearance page changes at once. */
export type Look = {
  mode: ThemeMode
  palette: ThemePalette
  custom: CustomTheme
  scale: number
  /** The order of the sidebar's pages (see `lib/menuList`). */
  menu: string[]
  /** The size buttons the player added, in percent. */
  presets: number[]
}

/** The look every account shows while `shared` is on (backend `Appearance`). */
export type SharedLook = Look & { shared: boolean }

/** This account interface's own look, kept while a shared one replaces it. */
const OWN_KEY = 'mokora.appearance.own'

type SharedState = { shared: boolean }

/** Whether this account follows the shared look. */
export const useSharedAppearanceStore = create<SharedState>(() => ({ shared: false }))

export function currentLook(): Look {
  const theme = useThemeStore.getState()
  return {
    mode: theme.mode,
    palette: theme.palette,
    custom: structuredClone(theme.custom),
    scale: useUiPrefsStore.getState().scale,
    menu: normalizeMenuOrder(useSidebar.getState().order),
    presets: [...useUiPrefsStore.getState().scalePresets],
  }
}

/** Keep a look: it is what this account shows from now on. */
export function applyLook(look: Look): void {
  const theme = useThemeStore.getState()
  theme.setMode(look.mode)
  theme.setCustom(structuredClone(look.custom))
  theme.setPalette(look.palette)
  useUiPrefsStore.getState().setScale(look.scale)
  useSidebar.getState().setOrder(look.menu)
  useUiPrefsStore.getState().setScalePresets(look.presets)
}

/** Show a look without keeping it (the settings preview). */
export function previewLook(look: Look): void {
  previewTheme(look.mode, look.palette, look.custom)
  document.documentElement.style.fontSize = `${16 * look.scale}px`
}

/** Drop a preview: show the look that is kept. */
export function restoreLook(): void {
  previewLook(currentLook())
}

export function readOwnLook(): Look | null {
  try {
    const raw = localStorage.getItem(OWN_KEY)
    return raw ? (JSON.parse(raw) as Look) : null
  } catch {
    return null
  }
}

export function rememberOwnLook(look: Look): void {
  try {
    localStorage.setItem(OWN_KEY, JSON.stringify(look))
  } catch {
    /* the shared look is then kept when sharing ends */
  }
}

function forgetOwnLook(): void {
  try {
    localStorage.removeItem(OWN_KEY)
  } catch {
    /* ignore */
  }
}

function sameLook(a: Look, b: Look): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

function lookOf(shared: SharedLook): Look {
  return {
    mode: shared.mode,
    palette: shared.palette,
    custom: shared.custom ?? currentLook().custom,
    scale: shared.scale,
    menu: normalizeMenuOrder(shared.menu),
    presets: cleanScalePresets(shared.presets),
  }
}

/**
 * Make this account match the shared state. While sharing is on the shared
 * look replaces the account's own, which is set aside first; when sharing
 * ends the own look comes back.
 */
export function reconcileAppearance(shared: SharedLook): void {
  useSharedAppearanceStore.setState({ shared: shared.shared })
  if (shared.shared) {
    if (readOwnLook() === null) rememberOwnLook(currentLook())
    const next = lookOf(shared)
    if (!sameLook(next, currentLook())) applyLook(next)
    return
  }
  const own = readOwnLook()
  if (own === null) return
  forgetOwnLook()
  if (!sameLook(own, currentLook())) applyLook(own)
}

/** Follow the shared look for the life of this account interface. */
export function useSharedAppearance(): void {
  useEffect(() => {
    if (!HAS_TAURI) return
    let live = true
    let stop: (() => void) | undefined
    invoke<SharedLook>('get_appearance')
      .then((shared) => {
        if (live) reconcileAppearance(shared)
      })
      .catch(() => {})
    void Promise.resolve(
      listen<SharedLook>('appearance-changed', (shared) => reconcileAppearance(shared)),
    )
      .then((unlisten) => {
        if (live) stop = unlisten
        else unlisten?.()
      })
      .catch(() => {})
    return () => {
      live = false
      stop?.()
    }
  }, [])
}

/** The size one step of the zoom shortcut away; `0` is the default size. */
export function steppedScale(scale: number, direction: -1 | 0 | 1): number {
  if (direction === 0) return SCALE_DEFAULT
  const stepped = Math.round((scale + direction * SCALE_STEP) / SCALE_STEP) * SCALE_STEP
  return Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, stepped)) * 100) / 100
}

/**
 * Change the interface size by one step, for good (the zoom shortcut). While
 * the look is shared the new size goes to every account too.
 */
export function zoomBy(direction: -1 | 0 | 1): void {
  const look = currentLook()
  const scale = steppedScale(look.scale, direction)
  if (scale === look.scale) return
  const next = { ...look, scale }
  applyLook(next)
  if (HAS_TAURI && useSharedAppearanceStore.getState().shared) {
    invoke('set_appearance', { look: { shared: true, ...next } }).catch(() => {})
  }
}

/** Ctrl + / Ctrl - / Ctrl 0 change the interface size while it is switched on. */
export function useZoomShortcut(): void {
  const enabled = useUiPrefsStore((s) => s.zoomShortcut)
  useEffect(() => {
    if (!enabled) return
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return
      let direction: -1 | 0 | 1
      if (e.key === '+' || e.key === '=') direction = 1
      else if (e.key === '-' || e.key === '_') direction = -1
      else if (e.key === '0') direction = 0
      else return
      e.preventDefault()
      // Unsaved changes lock the rest of the app; the size is no exception.
      const guard = useSettingsGuardStore.getState()
      if (guard.changes > 0) {
        guard.nudge()
        return
      }
      zoomBy(direction)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [enabled])
}

/** A size typed in as a percent: a whole number inside the limits, else `null`. */
export function customScalePercent(text: string): number | null {
  if (!/^\d{2,3}$/.test(text.trim())) return null
  const percent = Number(text.trim())
  return percent >= Math.round(SCALE_MIN * 100) && percent <= Math.round(SCALE_MAX * 100)
    ? percent
    : null
}
