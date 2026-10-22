// Zoom / pan window over a chart's x axis (the cumulative-PT chart's game
// ordinals). Pure, so the chart only wires pointer and wheel events to it.
//
// The window is a span of whole point indices, `lo..hi` over points
// `0..n-1` (the chart puts its first shown point on the plot's left edge and
// its last on the right); `null` is the whole series. Zooming keeps the
// point under the cursor where it is: `anchor` is the cursor's place across
// the plot, 0 at its left edge, 1 at its right.

export type XWindow = { lo: number; hi: number }

/** The narrowest window, in point gaps (so five points stay in view). */
export const MIN_SPAN = 4

/** One Ctrl+wheel notch: in shrinks the span by this, out grows it. */
export const ZOOM_STEP = 1.25

function clampLo(lo: number, span: number, n: number): number {
  return Math.min(Math.max(lo, 0), n - 1 - span)
}

/**
 * The window after zooming `w` (null for the whole series) by `factor`
 * (< 1 in, > 1 out) around `anchor`. Back to null once it spans everything.
 */
export function zoomWindow(
  w: XWindow | null,
  n: number,
  anchor: number,
  factor: number,
): XWindow | null {
  const full = n - 1
  if (full <= MIN_SPAN) return null
  const cur = w ?? { lo: 0, hi: full }
  const span = cur.hi - cur.lo
  const next = Math.min(Math.max(Math.round(span * factor), MIN_SPAN), full)
  if (next >= full) return null
  const a = Math.min(Math.max(anchor, 0), 1)
  const pivot = cur.lo + a * span
  const lo = clampLo(Math.round(pivot - a * next), next, n)
  return { lo, hi: lo + next }
}

/** The window moved by `shift` points (positive = later games), kept inside. */
export function panWindow(w: XWindow | null, n: number, shift: number): XWindow | null {
  if (!w) return null
  const span = w.hi - w.lo
  const lo = clampLo(w.lo + Math.round(shift), span, n)
  return { lo, hi: lo + span }
}

/** The points a window shows, as a `slice(start, end)` range. */
export function visibleRange(w: XWindow | null, n: number): [number, number] {
  if (!w) return [0, n]
  return [Math.max(w.lo, 0), Math.min(w.hi + 1, n)]
}
