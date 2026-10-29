import { beforeEach, describe, expect, it } from 'vitest'

import { useSidebar } from '@/hooks/useSidebar'
import { DEFAULT_MENU_ORDER, LEGACY_MENU_ORDER } from '@/lib/menuList'

const KEY = 'mokora.sidebar'

beforeEach(() => localStorage.clear())

async function loadStored(state: object, version: number) {
  localStorage.setItem(KEY, JSON.stringify({ state, version }))
  await useSidebar.persist.rehydrate()
  return useSidebar.getState().order
}

describe('the sidebar order a person already has', () => {
  it('moves from the old default to the new one, nobody having chosen it', async () => {
    const order = await loadStored({ isOpen: true, settings: { disabled: false, isHoverOpen: true }, order: [...LEGACY_MENU_ORDER] }, 0)
    expect(order).toEqual([...DEFAULT_MENU_ORDER])
  })

  it('keeps an order that was chosen', async () => {
    const chosen = ['settings', ...DEFAULT_MENU_ORDER.filter((id) => id !== 'settings')]
    const order = await loadStored({ isOpen: true, settings: { disabled: false, isHoverOpen: true }, order: chosen }, 0)
    expect(order).toEqual(chosen)
  })
})
