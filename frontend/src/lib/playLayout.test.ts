import { describe, expect, it } from 'vitest'
import {
  ASIDE_DEFAULT,
  ASIDE_MIN,
  ASIDE_DEFAULT_RATIO,
  asideWidthFor,
  clampAsideWidth,
  placePanel,
} from './playLayout'

describe('asideWidthFor', () => {
  it('gives the panel the same share of any window', () => {
    expect(asideWidthFor(ASIDE_DEFAULT_RATIO, 1560)).toBe(343)
    expect(asideWidthFor(ASIDE_DEFAULT_RATIO, 2560)).toBe(563)
  })
  it('stays within the bounds', () => {
    expect(asideWidthFor(ASIDE_DEFAULT_RATIO, 900)).toBe(ASIDE_MIN)
    expect(asideWidthFor(0.5, 5000)).toBe(800)
  })
})

describe('clampAsideWidth', () => {
  it('keeps a width that fits', () => {
    expect(clampAsideWidth(400, 1600)).toBe(400)
  })
  it('never goes under the minimum', () => {
    expect(clampAsideWidth(50, 1600)).toBe(ASIDE_MIN)
  })
  it('leaves the game its share of a narrow window', () => {
    expect(clampAsideWidth(900, 1000)).toBe(520)
  })
  it('caps a very wide window', () => {
    expect(clampAsideWidth(5000, 4000)).toBe(800)
  })
  it('falls back to the default for a bad value', () => {
    expect(clampAsideWidth(Number.NaN, 1600)).toBe(ASIDE_DEFAULT)
  })
  it('keeps the minimum even when the window is tiny', () => {
    expect(clampAsideWidth(400, 600)).toBe(ASIDE_MIN)
  })
})

describe('placePanel', () => {
  it('centres the panel under the button when there is room', () => {
    expect(placePanel(1000, 1600, 700, 300)).toEqual({ left: 850, width: 300 })
  })
  it('keeps it inside the right-hand column, off the game', () => {
    // Button near the right edge: pushed left only as far as the window allows.
    expect(placePanel(1560, 1562, 1210, 304)).toEqual({ left: 1250, width: 304 })
    // Button near the column's left edge: never reaches over the game.
    expect(placePanel(1230, 1562, 1210, 304)).toEqual({ left: 1218, width: 304 })
  })
  it('narrows to the column when the column is narrow', () => {
    expect(placePanel(1500, 1562, 1302, 304)).toEqual({ left: 1310, width: 244 })
  })
})
