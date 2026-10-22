import type { ShowItem, ShowMeta } from '@/types'

// Pure helpers for the bot's `meta.show` payload. Used by the Bot Show
// dashboard tile and by the always-on-top overlay window, which get their data
// from different places (a zustand store vs. a raw `bot-response` listener) and
// only agree on shape.

/** "#aabbcc" → "rgba(170,187,204,a)". Undefined when the input isn't a valid hex. */
export function hexToRgba(hex: string | null | undefined, alpha: number): string | undefined {
  if (!hex) return undefined
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim())
  if (!m) return undefined
  const n = parseInt(m[1], 16)
  return `rgba(${(n >> 16) & 0xff}, ${(n >> 8) & 0xff}, ${n & 0xff}, ${alpha})`
}

/** The `show` block of a bot response's `meta`, or null if it carries none. */
export function pickShow(meta: unknown): ShowMeta | null {
  if (!meta || typeof meta !== 'object') return null
  const show = (meta as Record<string, unknown>).show
  if (!show || typeof show !== 'object') return null
  const items = (show as Record<string, unknown>).items
  if (!Array.isArray(items) || items.length === 0) return null
  return show as ShowMeta
}

/** A row is worth a line only if it has something to draw. */
export function hasContent(it: ShowItem): boolean {
  return Boolean(it.label || it.tiles || (it.pais && it.pais.length))
}

/**
 * Drawable rows of `show`, capped at `limit`.
 *
 * `limit` is the overlay's top-N. It is applied *after* the empty-row filter
 * so "top 3" always means three visible rows, not three candidates of which
 * some may render as nothing.
 */
export function visibleItems(show: ShowMeta | null, limit?: number): ShowItem[] {
  const items = show?.items.filter(hasContent) ?? []
  return limit === undefined ? items : items.slice(0, Math.max(0, limit))
}
