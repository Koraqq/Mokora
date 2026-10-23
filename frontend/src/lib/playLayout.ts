// The width of the right-hand panel on the play screen, which the player drags
// to resize. It is kept as a share of the window, not in pixels, so the panel
// and the game keep their proportions in any window size (a width chosen in a
// maximized window would crowd the game in a smaller one). Kept within bounds
// so neither the panel nor the game can be squeezed away, and remembered
// between runs.

/** The panel's share of the window until the player sizes it: the game keeps
 *  the rest, which fits its table without wide empty bands around it. */
export const ASIDE_DEFAULT_RATIO = 0.343
/** The width used when the window's width is unknown. */
export const ASIDE_DEFAULT = 535
export const ASIDE_MIN = 260
const ASIDE_MAX = 1000
/** The game keeps at least this much of the window. */
const GAME_MIN = 480

/** Where the bot panel goes: centred under its button, but kept inside the
 *  right-hand column (from `minLeft` to the window's right edge), because the
 *  game's web view paints above the page and would hide whatever reached over
 *  it. Returns the panel's left edge and width, in window pixels. */
export function placePanel(
  buttonCenter: number,
  viewport: number,
  minLeft: number,
  preferredWidth: number,
  margin = 8,
): { left: number; width: number } {
  const room = Math.max(0, viewport - minLeft - 2 * margin)
  const width = Math.min(preferredWidth, room)
  const left = Math.min(
    Math.max(buttonCenter - width / 2, minLeft + margin),
    viewport - margin - width,
  )
  return { left: Math.round(left), width: Math.round(width) }
}

// The share of the window. (The pixel width older builds kept under
// `mokora.play.asideWidth` is not read: it is what this replaces.)
const KEY = 'mokora.play.asideShare'

/** `width` limited to what a window `viewport` pixels wide can hold. */
export function clampAsideWidth(width: number, viewport: number): number {
  const max = Math.max(ASIDE_MIN, Math.min(ASIDE_MAX, viewport - GAME_MIN))
  if (!Number.isFinite(width)) return Math.min(ASIDE_DEFAULT, max)
  return Math.round(Math.min(max, Math.max(ASIDE_MIN, width)))
}

/** The panel's width for share `share` of a window `viewport` pixels wide. */
export function asideWidthFor(share: number, viewport: number): number {
  return clampAsideWidth(share * viewport, viewport)
}

/** The share of the window the player sized the panel to, or the default. */
export function loadAsideShare(): number {
  try {
    const v = Number(window.localStorage.getItem(KEY))
    if (v > 0 && v < 1) return v
  } catch {
    /* storage can be blocked: use the default */
  }
  return ASIDE_DEFAULT_RATIO
}

/** Remember the panel at `width` pixels of a window `viewport` wide. */
export function saveAsideShare(width: number, viewport: number) {
  if (!(viewport > 0)) return
  try {
    window.localStorage.setItem(KEY, (width / viewport).toFixed(4))
  } catch {
    /* not remembered; it still works for this run */
  }
}

/** Back to the default share (forget the player's own). */
export function clearAsideShare() {
  try {
    window.localStorage.removeItem(KEY)
  } catch {
    /* nothing was remembered */
  }
}
