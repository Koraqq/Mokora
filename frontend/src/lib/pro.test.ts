import { describe, expect, it } from 'vitest'

import { moduleReady, percent, size } from './pro'

describe('paid module download', () => {
  it('counts whole percent, never past 100 or below 0', () => {
    expect(percent(0, 1000)).toBe(0)
    expect(percent(499, 1000)).toBe(49)
    expect(percent(1000, 1000)).toBe(100)
    expect(percent(5, 0)).toBe(0)
    expect(percent(2000, 1000)).toBe(100)
  })

  it('tells sizes in KB under a megabyte, MB above', () => {
    expect(size(292352)).toBe('286 KB')
    expect(size(1)).toBe('1 KB')
    expect(size(3 * 1024 * 1024 + 400 * 1024)).toBe('3.4 MB')
  })

  it('the paid features run only with the module in use', () => {
    expect(moduleReady({ state: 'ready', version: '0.1.0' })).toBe(true)
    expect(moduleReady({ state: 'restart', version: '0.1.1' })).toBe(false)
    expect(moduleReady({ state: 'downloading', version: '0.1.0', received: 1, total: 2 })).toBe(false)
    expect(moduleReady(null)).toBe(false)
  })
})
