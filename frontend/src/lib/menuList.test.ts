import { describe, expect, it } from 'vitest'

import {
  DEFAULT_MENU_ORDER,
  LEGACY_MENU_ORDER,
  getMenuList,
  moveMenuItem,
  normalizeMenuOrder,
} from '@/lib/menuList'

const ids = (order: string[]) => getMenuList((k) => k, true, order)[0].menus.map((m) => m.id)

describe('sidebar order', () => {
  it('is the default when nothing was saved', () => {
    expect(normalizeMenuOrder(undefined)).toEqual([...DEFAULT_MENU_ORDER])
    expect(normalizeMenuOrder([])).toEqual([...DEFAULT_MENU_ORDER])
    expect(ids([])).toEqual([...DEFAULT_MENU_ORDER])
  })

  it('starts as the order the owner chose: game screen, profile, play, history, bots, appearance, review, settings, logs', () => {
    expect([...DEFAULT_MENU_ORDER]).toEqual([
      'play',
      'profile',
      'game',
      'history',
      'bots',
      'appearance',
      'review',
      'settings',
      'logs',
    ])
  })

  it('keeps only real pages once each and puts missing ones after the rest', () => {
    const order = normalizeMenuOrder(['settings', 'nope', 'play', 'settings'])
    expect(order.slice(0, 2)).toEqual(['settings', 'play'])
    expect(order).toHaveLength(DEFAULT_MENU_ORDER.length)
    expect(new Set(order).size).toBe(order.length)
    expect(order.slice(2)).toEqual(DEFAULT_MENU_ORDER.filter((id) => id !== 'settings' && id !== 'play'))
  })

  it('shows the pages in the saved order', () => {
    expect(ids(['settings', 'appearance']).slice(0, 2)).toEqual(['settings', 'appearance'])
  })

  it('moves a page a place among the shown ones, leaving the hidden ones where they are', () => {
    const shown = LEGACY_MENU_ORDER.filter((id) => id !== 'logs')
    // `logs` sits between `review` and `appearance`; moving `appearance` up
    // passes `review` and leaves `logs` where it was.
    const up = moveMenuItem(LEGACY_MENU_ORDER, shown, 'appearance', -1)
    expect(up).toEqual(['play', 'game', 'bots', 'profile', 'history', 'appearance', 'logs', 'review', 'settings'])
    const down = moveMenuItem(LEGACY_MENU_ORDER, shown, 'play', 1)
    expect(down.slice(0, 2)).toEqual(['game', 'play'])
    expect(down[down.indexOf('logs')]).toBe('logs')
  })

  it('does not move past either end', () => {
    const first = DEFAULT_MENU_ORDER[0]
    const last = DEFAULT_MENU_ORDER[DEFAULT_MENU_ORDER.length - 1]
    expect(moveMenuItem(DEFAULT_MENU_ORDER, DEFAULT_MENU_ORDER, first, -1)).toEqual([...DEFAULT_MENU_ORDER])
    expect(moveMenuItem(DEFAULT_MENU_ORDER, DEFAULT_MENU_ORDER, last, 1)).toEqual([...DEFAULT_MENU_ORDER])
    expect(moveMenuItem(DEFAULT_MENU_ORDER, ['play'], 'game', 1)).toEqual([...DEFAULT_MENU_ORDER])
  })
})
