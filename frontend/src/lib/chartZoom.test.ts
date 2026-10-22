import { describe, expect, it } from 'vitest'

import { MIN_SPAN, panWindow, visibleRange, zoomWindow } from './chartZoom'

describe('zoomWindow', () => {
  it('keeps the point under the cursor in place', () => {
    // 101 points; zoom in at a fifth of the way across.
    const w = zoomWindow(null, 101, 0.2, 0.5)!
    expect(w).toEqual({ lo: 10, hi: 60 })
    // The point at 20 % stays at 20 % of the new window.
    expect(w.lo + 0.2 * (w.hi - w.lo)).toBe(20)
  })

  it('stops at the narrowest span', () => {
    let w = zoomWindow(null, 101, 0.5, 0.5)
    for (let i = 0; i < 30; i++) w = zoomWindow(w, 101, 0.5, 0.5)
    expect(w!.hi - w!.lo).toBe(MIN_SPAN)
  })

  it('stays inside the series at its edges', () => {
    const w = zoomWindow(null, 101, 0, 0.5)!
    expect(w.lo).toBe(0)
    const z = zoomWindow({ lo: 90, hi: 100 }, 101, 1, 1.5)!
    expect(z.hi).toBe(100)
    expect(z.hi - z.lo).toBe(15)
  })

  it('zooming out past everything is the whole series again', () => {
    expect(zoomWindow({ lo: 10, hi: 90 }, 101, 0.5, 1.5)).toBeNull()
    expect(zoomWindow(null, 101, 0.5, 1.25)).toBeNull()
  })

  it('a series too short to zoom stays whole', () => {
    expect(zoomWindow(null, 4, 0.5, 0.5)).toBeNull()
  })
})

describe('panWindow', () => {
  it('moves the window by whole points and keeps its span', () => {
    expect(panWindow({ lo: 10, hi: 30 }, 101, 5)).toEqual({ lo: 15, hi: 35 })
    expect(panWindow({ lo: 10, hi: 30 }, 101, 2.4)).toEqual({ lo: 12, hi: 32 })
  })

  it('stops at both ends', () => {
    expect(panWindow({ lo: 10, hi: 30 }, 101, -50)).toEqual({ lo: 0, hi: 20 })
    expect(panWindow({ lo: 70, hi: 90 }, 101, 50)).toEqual({ lo: 80, hi: 100 })
  })

  it('does nothing to the whole series', () => {
    expect(panWindow(null, 101, 5)).toBeNull()
  })
})

describe('visibleRange', () => {
  it('shows both ends of the window', () => {
    expect(visibleRange({ lo: 10, hi: 20 }, 101)).toEqual([10, 21])
    expect(visibleRange({ lo: 0, hi: 100 }, 101)).toEqual([0, 101])
  })

  it('is everything without a window', () => {
    expect(visibleRange(null, 7)).toEqual([0, 7])
  })
})
