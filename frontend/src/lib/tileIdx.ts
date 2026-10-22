// mjai tile string <-> 0..33 index (matches AnalysisResult.mixed_risk layout).
// 0..8   1m..9m
// 9..17  1p..9p
// 18..26 1s..9s
// 27..33 E S W N P F C
// Red fives (5mr/5pr/5sr) collapse to indices 4/13/22.

const HONOR_INDEX: Record<string, number> = {
  E: 27, S: 28, W: 29, N: 30, P: 31, F: 32, C: 33,
}

export function tileIdx(mjai: string): number {
  if (!mjai) return -1
  if (HONOR_INDEX[mjai] !== undefined) return HONOR_INDEX[mjai]
  // 5mr/5pr/5sr → red five
  if (mjai.endsWith('r')) {
    const suit = mjai[mjai.length - 2]
    if (suit === 'm') return 4
    if (suit === 'p') return 13
    if (suit === 's') return 22
    return -1
  }
  const num = parseInt(mjai[0], 10)
  if (Number.isNaN(num) || num < 1 || num > 9) return -1
  const suit = mjai[mjai.length - 1]
  if (suit === 'm') return num - 1
  if (suit === 'p') return 8 + num
  if (suit === 's') return 17 + num
  return -1
}

export const TILE_LABELS_34: readonly string[] = [
  '1m', '2m', '3m', '4m', '5m', '6m', '7m', '8m', '9m',
  '1p', '2p', '3p', '4p', '5p', '6p', '7p', '8p', '9p',
  '1s', '2s', '3s', '4s', '5s', '6s', '7s', '8s', '9s',
  'E', 'S', 'W', 'N', 'P', 'F', 'C',
]

// Convert an array of mjai tile strings to a mahgen DSL string.
// Used as a fallback when the backend MahgenView isn't available
// (e.g. dora indicator from snapshot, recommendation single tile).
const Z_INDEX: Record<string, number> = { E: 1, S: 2, W: 3, N: 4, P: 5, F: 6, C: 7 }

export function mjaiToMahgen(tiles: string[] | null | undefined): string {
  if (!tiles || !tiles.length) return ''
  let backs = 0
  const m: number[] = []
  const p: number[] = []
  const s: number[] = []
  const z: number[] = []
  for (const tile of tiles) {
    if (!tile || tile === '?') {
      backs++
      continue
    }
    if (Z_INDEX[tile]) {
      z.push(Z_INDEX[tile])
      continue
    }
    const isRed = tile.endsWith('r')
    const suit = isRed ? tile[tile.length - 2] : tile[tile.length - 1]
    const num = isRed ? 0 : parseInt(tile[0], 10)
    if (suit === 'm') m.push(num)
    else if (suit === 'p') p.push(num)
    else if (suit === 's') s.push(num)
  }
  const sortKey = (a: number, b: number) => (a === 0 ? 5.5 : a) - (b === 0 ? 5.5 : b)
  m.sort(sortKey); p.sort(sortKey); s.sort(sortKey); z.sort((a, b) => a - b)
  let out = ''
  if (m.length) out += m.join('') + 'm'
  if (p.length) out += p.join('') + 'p'
  if (s.length) out += s.join('') + 's'
  if (z.length) out += z.join('') + 'z'
  if (backs > 0) out += '0'.repeat(backs) + 'z'
  return out
}

// Display-order rank for an mjai tile: m < p < s < honors (E S W N P F C),
// ascending within suit, with red fives (5mr/5pr/5sr) placed just after the
// plain 5. Mirrors the ordering inside mjaiToMahgen.
function tileSortRank(mjai: string): number {
  if (HONOR_INDEX[mjai] !== undefined) return 300 + (Z_INDEX[mjai] ?? 0)
  const isRed = mjai.endsWith('r')
  const suit = isRed ? mjai[mjai.length - 2] : mjai[mjai.length - 1]
  const suitRank = suit === 'm' ? 0 : suit === 'p' ? 100 : suit === 's' ? 200 : 900
  const num = isRed ? 5.5 : parseInt(mjai[0], 10)
  return suitRank + (Number.isNaN(num) ? 99 : num)
}

// Sort mjai tile strings into the same display order the Self Hand panel uses
// (via mjaiToMahgen), returned as an array so callers can annotate each tile
// individually. Stable, so duplicate tiles keep their relative order.
export function sortMjaiTiles(tiles: string[]): string[] {
  return [...tiles].sort((a, b) => tileSortRank(a) - tileSortRank(b))
}
