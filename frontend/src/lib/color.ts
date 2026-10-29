export interface Rgb {
  r: number
  g: number
  b: number
}

/** Hue in degrees (0-360), saturation and value from 0 to 1. */
export interface Hsv {
  h: number
  s: number
  v: number
}

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n))

/** `#rrggbb` (lower case) from `#rgb`, `rgb`, `#rrggbb` or `rrggbb`; null when it is none of those. */
export function parseHex(text: string): string | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim())
  if (!m) return null
  const digits = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1]
  return `#${digits.toLowerCase()}`
}

export function hexToRgb(hex: string): Rgb {
  const digits = (parseHex(hex) ?? '#000000').slice(1)
  return {
    r: parseInt(digits.slice(0, 2), 16),
    g: parseInt(digits.slice(2, 4), 16),
    b: parseInt(digits.slice(4, 6), 16),
  }
}

export function rgbToHex({ r, g, b }: Rgb): string {
  return `#${[r, g, b]
    .map((n) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, '0'))
    .join('')}`
}

export function hsvToRgb({ h, s, v }: Hsv): Rgb {
  const hue = ((h % 360) + 360) % 360
  const f = (n: number) => {
    const k = (n + hue / 60) % 6
    return (v - v * s * Math.max(0, Math.min(k, 4 - k, 1))) * 255
  }
  return { r: f(5), g: f(3), b: f(1) }
}

/** `hueFallback` stands in for the hue of a grey, which has none. */
export function rgbToHsv({ r, g, b }: Rgb, hueFallback = 0): Hsv {
  const R = r / 255
  const G = g / 255
  const B = b / 255
  const max = Math.max(R, G, B)
  const d = max - Math.min(R, G, B)
  let h = hueFallback
  if (d > 0) {
    if (max === R) h = ((G - B) / d) % 6
    else if (max === G) h = (B - R) / d + 2
    else h = (R - G) / d + 4
    h = (h * 60 + 360) % 360
  }
  return { h, s: max === 0 ? 0 : d / max, v: max }
}

export const hsvToHex = (hsv: Hsv): string => rgbToHex(hsvToRgb(hsv))

export function hexToHsv(hex: string, hueFallback = 0): Hsv {
  return rgbToHsv(hexToRgb(hex), hueFallback)
}

interface Rgba extends Rgb {
  a: number
}

let probe: CanvasRenderingContext2D | null | undefined

/** Any CSS color (rgb, hsl, oklch, color-mix ...) as rgba, through the browser's own parser. */
export function cssToRgba(css: string): Rgba | null {
  if (probe === undefined) {
    try {
      const canvas = document.createElement('canvas')
      canvas.width = canvas.height = 1
      probe = canvas.getContext('2d', { willReadFrequently: true })
    } catch {
      probe = null
    }
  }
  if (probe) {
    probe.clearRect(0, 0, 1, 1)
    probe.fillStyle = '#000'
    probe.fillStyle = css
    probe.fillRect(0, 0, 1, 1)
    const [r, g, b, a] = probe.getImageData(0, 0, 1, 1).data
    return { r, g, b, a: a / 255 }
  }
  const hex = parseHex(css)
  if (hex) return { ...hexToRgb(hex), a: 1 }
  const m = /^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)(?:[,\s/]+([\d.]+))?/i.exec(css.trim())
  if (m) return { r: +m[1], g: +m[2], b: +m[3], a: m[4] === undefined ? 1 : +m[4] }
  return null
}

/** The color a person sees where `el` is: its own background laid over those of
 *  the elements behind it, up to the first one that is solid. */
export function colorOfElement(el: Element): string {
  const layers: Rgba[] = []
  for (let node: Element | null = el; node; node = node.parentElement) {
    const c = cssToRgba(getComputedStyle(node).backgroundColor)
    if (c && c.a > 0) {
      layers.push(c)
      if (c.a >= 1) break
    }
  }
  let out: Rgb = { r: 0, g: 0, b: 0 }
  for (let i = layers.length - 1; i >= 0; i--) {
    const { r, g, b, a } = layers[i]
    out = { r: r * a + out.r * (1 - a), g: g * a + out.g * (1 - a), b: b * a + out.b * (1 - a) }
  }
  return rgbToHex(out)
}
