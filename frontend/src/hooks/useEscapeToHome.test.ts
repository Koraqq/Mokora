import { describe, expect, it } from 'vitest'
import { leavesOnEscape } from './useEscapeToHome'

const base = { key: 'Escape', defaultPrevented: false, typing: false, layerOpen: false }

describe('leavesOnEscape', () => {
  it('leaves on a plain Esc', () => {
    expect(leavesOnEscape(base)).toBe(true)
  })
  it('ignores other keys', () => {
    expect(leavesOnEscape({ ...base, key: 'Enter' })).toBe(false)
  })
  it('lets an open dialog or menu take the Esc', () => {
    expect(leavesOnEscape({ ...base, layerOpen: true })).toBe(false)
  })
  it('stays while typing or when the press was already used', () => {
    expect(leavesOnEscape({ ...base, typing: true })).toBe(false)
    expect(leavesOnEscape({ ...base, defaultPrevented: true })).toBe(false)
  })
  it('lets an open sidebar drawer take the Esc', () => {
    expect(leavesOnEscape({ ...base, drawerOpen: true })).toBe(false)
  })
})
