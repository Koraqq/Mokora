import { useSyncExternalStore } from 'react'

/**
 * Tailwind's default `lg` breakpoint, in CSS px.
 *
 * Tailwind v4 declares `lg` as `64rem`, but `rem` inside a media query resolves
 * against the browser's *initial* font size (16px) — not the `:root` font-size
 * App.tsx rewrites for the UI-scale setting. So `lg:` really is a hard 1024 CSS
 * px at any scale, and this constant can mirror it.
 */
export const LG_BREAKPOINT_PX = 1024

// `lg:` applies from 1024px upwards, so "narrow" is everything strictly below.
const NARROW_QUERY = `(max-width: ${LG_BREAKPOINT_PX - 0.02}px)`

function subscribe(onChange: () => void): () => void {
  if (typeof window === 'undefined' || !window.matchMedia) return () => {}
  const mql = window.matchMedia(NARROW_QUERY)
  mql.addEventListener('change', onChange)
  return () => mql.removeEventListener('change', onChange)
}

function getSnapshot(): boolean {
  // Guard rather than assume: jsdom ships no matchMedia, and a missing query
  // list should degrade to the docked layout rather than throw at render.
  if (typeof window === 'undefined' || !window.matchMedia) return false
  return window.matchMedia(NARROW_QUERY).matches
}

/**
 * True when the viewport is too narrow for the docked sidebar — i.e. exactly
 * when the `lg:` variants that reveal it are inactive.
 *
 * The sidebar used to rely on `lg:translate-x-0` alone, which slid it fully
 * off-screen below the breakpoint while leaving its only two reveal controls
 * (the pin button and the hover handlers) inside the off-screen element. That
 * made the app un-navigable at any width under 1024px. Narrow mode is now
 * driven from JS so the shell can render a trigger and close the drawer on
 * navigation / Escape — none of which CSS alone can express.
 */
export function useIsNarrow(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, () => false)
}
