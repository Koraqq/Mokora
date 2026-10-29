import { create } from 'zustand'

// Frontend-only UI preferences kept out of the Tauri-owned `AppConfig` since
// they don't affect any backend behavior. Sidebar collapsed/hover state lives
// in `useSidebar` (own zustand+persist store ported from shadcn-ui-sidebar).
const SCALE_KEY = 'mokora.ui.scale'

// One-time flag: has the user seen the dashboard onboarding hint (drag /
// resize / remove / add tiles)? Deliberately NOT reset by "Reset Layout" —
// this tracks "has seen the tutorial", not layout state.
const ONBOARDED_KEY = 'mokora.dashboard.onboarded'

// Developer mode: shows the pages only a developer can read (the log page).
// Off by default.
const DEVELOPER_KEY = 'mokora.ui.developerMode'

// The size buttons the player added next to the built-in ones.
const SCALE_PRESETS_KEY = 'mokora.ui.scalePresets'

// Ctrl + / Ctrl - / Ctrl 0 change the interface size (see `useZoomShortcut`).
const ZOOM_SHORTCUT_KEY = 'mokora.ui.zoomShortcut'

export const SCALE_MIN = 0.7
export const SCALE_MAX = 1.5
export const SCALE_STEP = 0.05
export const SCALE_DEFAULT = 1.0

/** The size buttons that are always there, in percent. */
export const BUILTIN_SCALE_PRESETS: readonly number[] = [85, 100, 115, 130]
/** How many buttons of their own a player may add. */
export const MAX_SCALE_PRESETS = 10

/** A size that may become a button: a whole percent inside the limits that is
 *  not a button already. */
export function canAddScalePreset(percent: number, added: readonly number[]): boolean {
  return (
    Number.isInteger(percent) &&
    percent >= Math.round(SCALE_MIN * 100) &&
    percent <= Math.round(SCALE_MAX * 100) &&
    !BUILTIN_SCALE_PRESETS.includes(percent) &&
    !added.includes(percent) &&
    added.length < MAX_SCALE_PRESETS
  )
}

/** Added buttons made whole: valid, once each, ascending, no more than the limit. */
export function cleanScalePresets(list: unknown): number[] {
  if (!Array.isArray(list)) return []
  const kept: number[] = []
  for (const item of list) {
    if (typeof item === 'number' && canAddScalePreset(item, kept)) kept.push(item)
  }
  return kept.sort((a, b) => a - b)
}

function loadScalePresets(): number[] {
  if (typeof localStorage === 'undefined') return []
  try {
    return cleanScalePresets(JSON.parse(localStorage.getItem(SCALE_PRESETS_KEY) ?? '[]'))
  } catch {
    return []
  }
}

function clampScale(v: number): number {
  if (!Number.isFinite(v)) return SCALE_DEFAULT
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, v))
}

function loadScale(): number {
  if (typeof localStorage === 'undefined') return SCALE_DEFAULT
  try {
    const raw = localStorage.getItem(SCALE_KEY)
    if (!raw) return SCALE_DEFAULT
    return clampScale(parseFloat(raw))
  } catch {
    return SCALE_DEFAULT
  }
}

function loadFlag(key: string): boolean {
  if (typeof localStorage === 'undefined') return false
  try {
    return localStorage.getItem(key) === '1'
  } catch {
    return false
  }
}

type UiPrefsStore = {
  scale: number
  setScale: (v: number) => void
  resetScale: () => void
  /** Whether the dashboard onboarding hint has been dismissed at least once. */
  dashboardOnboarded: boolean
  markDashboardOnboarded: () => void
  /** Developer mode: the log page shows in the sidebar. */
  developerMode: boolean
  setDeveloperMode: (on: boolean) => void
  /** Ctrl + / Ctrl - change the interface size straight away. */
  zoomShortcut: boolean
  setZoomShortcut: (on: boolean) => void
  /** The size buttons the player added (percent, ascending). */
  scalePresets: number[]
  setScalePresets: (presets: number[]) => void
}

export const useUiPrefsStore = create<UiPrefsStore>((set) => ({
  scale: loadScale(),
  setScale: (v) => {
    const scale = clampScale(v)
    try {
      localStorage.setItem(SCALE_KEY, String(scale))
    } catch {
      /* quota — ignore */
    }
    set({ scale })
  },
  resetScale: () => {
    try {
      localStorage.setItem(SCALE_KEY, String(SCALE_DEFAULT))
    } catch {
      /* quota — ignore */
    }
    set({ scale: SCALE_DEFAULT })
  },
  dashboardOnboarded: loadFlag(ONBOARDED_KEY),
  markDashboardOnboarded: () => {
    try {
      localStorage.setItem(ONBOARDED_KEY, '1')
    } catch {
      /* quota — ignore */
    }
    set({ dashboardOnboarded: true })
  },
  developerMode: loadFlag(DEVELOPER_KEY),
  setDeveloperMode: (on) => {
    try {
      localStorage.setItem(DEVELOPER_KEY, on ? '1' : '0')
    } catch {
      /* quota — ignore */
    }
    set({ developerMode: on })
  },
  zoomShortcut: loadFlag(ZOOM_SHORTCUT_KEY),
  setZoomShortcut: (on) => {
    try {
      localStorage.setItem(ZOOM_SHORTCUT_KEY, on ? '1' : '0')
    } catch {
      /* quota — ignore */
    }
    set({ zoomShortcut: on })
  },
  scalePresets: loadScalePresets(),
  setScalePresets: (presets) => {
    const scalePresets = cleanScalePresets(presets)
    try {
      localStorage.setItem(SCALE_PRESETS_KEY, JSON.stringify(scalePresets))
    } catch {
      /* quota — ignore */
    }
    set({ scalePresets })
  },
}))
