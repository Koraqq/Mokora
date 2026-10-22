// How many settings a draft changes from the saved config: one per leaf
// value that differs (a list counts as one setting), for the settings page's
// "N settings changed" bar.

type Value = unknown

function isRecord(v: Value): v is Record<string, Value> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

/** Leaf values that differ between `a` and `b`; keys present on only one
 *  side count once each. */
export function countChangedSettings(a: Value, b: Value): number {
  if (isRecord(a) && isRecord(b)) {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)])
    let n = 0
    for (const k of keys) n += countChangedSettings(a[k], b[k])
    return n
  }
  return JSON.stringify(a) === JSON.stringify(b) ? 0 : 1
}
