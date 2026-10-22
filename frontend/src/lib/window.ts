import { getCurrentWindow } from '@tauri-apps/api/window'
import { HAS_TAURI } from '@/lib/tauri'

/** Label of the always-on-top suggestion overlay. Mirrors `ipc::overlay::LABEL`. */
export const OVERLAY_LABEL = 'overlay'

/**
 * Which window is this document running in?
 *
 * The overlay and the main app load the same `index.html`, so the label is
 * what `main.tsx` branches on to decide which root to mount. Outside Tauri
 * (plain `npm run dev` in a browser) there is no window label — report the
 * main window so the normal app renders.
 */
export function currentWindowLabel(): string {
  if (!HAS_TAURI) return 'main'
  try {
    return getCurrentWindow().label
  } catch {
    return 'main'
  }
}

export function isOverlayWindow(): boolean {
  return currentWindowLabel() === OVERLAY_LABEL
}
