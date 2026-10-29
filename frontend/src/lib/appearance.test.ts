import { fireEvent, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import {
  applyLook,
  currentLook,
  previewLook,
  readOwnLook,
  reconcileAppearance,
  restoreLook,
  steppedScale,
  useZoomShortcut,
  zoomBy,
  useSharedAppearanceStore,
  type Look,
  type SharedLook,
} from '@/lib/appearance'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'
import { useUiPrefsStore } from '@/stores/uiPrefsStore'
import { DEFAULT_MENU_ORDER } from '@/lib/menuList'
import { DEFAULT_CUSTOM_DARK, DEFAULT_CUSTOM_LIGHT, useThemeStore } from '@/stores/themeStore'

const custom = { light: { ...DEFAULT_CUSTOM_LIGHT }, dark: { ...DEFAULT_CUSTOM_DARK } }
const own: Look = { mode: 'light', palette: 'slate', custom, scale: 1.1, menu: [...DEFAULT_MENU_ORDER], presets: [] }
const shared: SharedLook = {
  shared: true,
  mode: 'dark',
  palette: 'crimson',
  custom,
  scale: 1.3,
  menu: ['settings', ...DEFAULT_MENU_ORDER.filter((id) => id !== 'settings')],
  presets: [90, 120],
}

beforeEach(() => {
  localStorage.clear()
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.className = ''
  document.documentElement.style.fontSize = ''
  useSharedAppearanceStore.setState({ shared: false })
  applyLook({ ...own, custom: structuredClone(custom) })
})

describe('shared appearance', () => {
  it('sets the own look aside and shows the shared one', () => {
    reconcileAppearance(shared)
    expect(currentLook()).toMatchObject({ mode: 'dark', palette: 'crimson', scale: 1.3 })
    expect(readOwnLook()).toMatchObject({ mode: 'light', palette: 'slate', scale: 1.1 })
    expect(useUiPrefsStore.getState().scalePresets).toEqual([90, 120])
    expect(useSharedAppearanceStore.getState().shared).toBe(true)
  })

  it('keeps the first own look when the shared look changes again', () => {
    reconcileAppearance(shared)
    reconcileAppearance({ ...shared, mode: 'light', scale: 0.9 })
    expect(currentLook()).toMatchObject({ mode: 'light', scale: 0.9 })
    expect(readOwnLook()).toMatchObject({ mode: 'light', palette: 'slate', scale: 1.1 })
  })

  it('gives every account its own look back when sharing ends', () => {
    reconcileAppearance(shared)
    reconcileAppearance({ ...shared, shared: false })
    expect(currentLook()).toMatchObject({ mode: 'light', palette: 'slate', scale: 1.1 })
    expect(readOwnLook()).toBeNull()
    // The buttons the account had of its own come back too.
    expect(useUiPrefsStore.getState().scalePresets).toEqual([])
    expect(useSharedAppearanceStore.getState().shared).toBe(false)
  })

  it('leaves an account that never shared untouched', () => {
    reconcileAppearance({ ...shared, shared: false })
    expect(currentLook()).toMatchObject({ mode: 'light', palette: 'slate', scale: 1.1 })
  })
})

describe('appearance preview', () => {
  it('shows a look without keeping it', () => {
    previewLook({
      mode: 'dark',
      palette: 'crimson',
      custom,
      scale: 1.5,
      menu: [...DEFAULT_MENU_ORDER],
      presets: [],
    })
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(document.documentElement.getAttribute('data-theme')).toBe('crimson')
    expect(document.documentElement.style.fontSize).toBe('24px')
    expect(useThemeStore.getState()).toMatchObject({ mode: 'light', palette: 'slate' })

    restoreLook()
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(document.documentElement.getAttribute('data-theme')).toBe('slate')
    expect(document.documentElement.style.fontSize).toBe('17.6px')
  })
})

describe('zoom shortcut', () => {
  it('steps the size within its limits and resets on zero', () => {
    expect(steppedScale(1, 1)).toBe(1.05)
    expect(steppedScale(1, -1)).toBe(0.95)
    expect(steppedScale(1.5, 1)).toBe(1.5)
    expect(steppedScale(0.7, -1)).toBe(0.7)
    expect(steppedScale(1.3, 0)).toBe(1)
    // No drift from the float arithmetic after many steps.
    let scale = 1
    for (let i = 0; i < 8; i++) scale = steppedScale(scale, 1)
    expect(scale).toBe(1.4)
  })

  it('keeps the new size for good', () => {
    zoomBy(1)
    expect(currentLook().scale).toBe(1.15)
    expect(document.documentElement.style.fontSize).toBe('')
    zoomBy(0)
    expect(currentLook().scale).toBe(1)
  })

  it('answers Ctrl + and Ctrl - only while switched on and nothing waits to be saved', () => {
    useSettingsGuardStore.setState({ changes: 0, nudges: 0 })
    useUiPrefsStore.setState({ zoomShortcut: false })
    const { rerender } = renderHook(() => useZoomShortcut())
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(currentLook().scale).toBe(1.1)

    useUiPrefsStore.setState({ zoomShortcut: true })
    rerender()
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(currentLook().scale).toBe(1.15)
    fireEvent.keyDown(window, { key: '-', ctrlKey: true })
    fireEvent.keyDown(window, { key: '-', ctrlKey: true })
    expect(currentLook().scale).toBe(1.05)
    // A plain key, or Ctrl with something else, is not for it.
    fireEvent.keyDown(window, { key: '=' })
    fireEvent.keyDown(window, { key: 'a', ctrlKey: true })
    expect(currentLook().scale).toBe(1.05)
    fireEvent.keyDown(window, { key: '0', ctrlKey: true })
    expect(currentLook().scale).toBe(1)

    // Unsaved settings lock the app: the key only nudges the bar.
    useSettingsGuardStore.setState({ changes: 2, nudges: 0 })
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(currentLook().scale).toBe(1)
    expect(useSettingsGuardStore.getState().nudges).toBe(1)
  })
})
