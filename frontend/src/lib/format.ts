export function fmtScore(n: number | null | undefined): string {
  return new Intl.NumberFormat('en-US').format(n ?? 0)
}

export function fmtTime(d: Date = new Date()): string {
  const h = String(d.getHours()).padStart(2, '0')
  const m = String(d.getMinutes()).padStart(2, '0')
  const s = String(d.getSeconds()).padStart(2, '0')
  return `${h}:${m}:${s}`
}

export function pct(v: number | null | undefined, digits = 1): string {
  if (v == null || Number.isNaN(v)) return '—'
  return `${Number(v).toFixed(digits)}%`
}

export function riskClass(v: number | null | undefined): string {
  if (v == null) return ''
  if (v >= 20) return 'risk-high'
  if (v >= 10) return 'risk-mid'
  return 'risk-low'
}

export type RelativeKind = 'self' | 'shimocha' | 'toimen' | 'kamicha'

/** Compute the relative seat position of `seat` from `ourSeat`'s perspective.
 * `numPlayers` defaults to 4 (yonma); pass 3 for sanma. In 3p there's no
 * `toimen` (no opposite seat in a triangle) — only `self / shimocha / kamicha`. */
export function relativeKind(
  seat: number,
  ourSeat: number | null,
  numPlayers: number = 4,
): RelativeKind {
  if (ourSeat == null) return 'self'
  const n = Math.max(1, numPlayers)
  const d = (seat - ourSeat + n) % n
  if (n === 3) {
    return d === 0 ? 'self' : d === 1 ? 'shimocha' : 'kamicha'
  }
  return d === 0 ? 'self' : d === 1 ? 'shimocha' : d === 2 ? 'toimen' : 'kamicha'
}

export const BAKAZE_LABEL: Record<string, string> = {
  E: '東',
  S: '南',
  W: '西',
  N: '北',
}

export function kyokuLabel(bakaze: string, kyoku: number): string {
  return `${BAKAZE_LABEL[bakaze] ?? bakaze} ${kyoku}局`
}

/** Seat wind (自風) label for `seat`. East = oya seat; each subsequent seat
 * rotates S → W → N (4p) or S → W (3p — sanma has no N self-wind, only E/S/W). */
export function bakazeFor(seat: number, oya: number, numPlayers: number): string {
  const order = ['E', 'S', 'W', 'N']
  const n = Math.max(1, numPlayers)
  return BAKAZE_LABEL[order[(seat - oya + n) % n]]
}
