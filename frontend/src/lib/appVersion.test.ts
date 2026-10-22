import { describe, expect, it } from 'vitest'

import { compareVersions } from './appVersion'

describe('compareVersions', () => {
  it('treats identical versions as equal', () => {
    expect(compareVersions('3.5.0', '3.5.0')).toBe(0)
  })

  it('compares segments numerically, not lexically', () => {
    expect(compareVersions('3.10.0', '3.9.9')).toBeGreaterThan(0)
    expect(compareVersions('3.6.0', '3.5.9')).toBeGreaterThan(0)
    expect(compareVersions('4.0.0', '3.99.99')).toBeGreaterThan(0)
  })

  it('orders the legacy beta shape numerically too', () => {
    // 3.0.0-8 < 3.0.0-10 — a lexical compare would invert these.
    expect(compareVersions('3.0.0-8', '3.0.0-10')).toBeLessThan(0)
    expect(compareVersions('3.0.0-10', '3.0.0-8')).toBeGreaterThan(0)
  })

  it('pads missing segments with zero', () => {
    expect(compareVersions('3.5', '3.5.0')).toBe(0)
    // The plain release is "version 0" of its own beta line under the
    // legacy scheme; all that matters here is a stable total order.
    expect(compareVersions('3.5.0', '3.5.0-1')).toBeLessThan(0)
  })

  it('is antisymmetric', () => {
    expect(compareVersions('3.4.3', '3.5.0')).toBeLessThan(0)
    expect(compareVersions('3.5.0', '3.4.3')).toBeGreaterThan(0)
  })
})
