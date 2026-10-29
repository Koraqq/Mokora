import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { RouterProvider, createMemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it } from 'vitest'

import { DEFAULT_MENU_ORDER } from '@/lib/menuList'
import { applyLook, currentLook } from '@/lib/appearance'
import { useSidebar } from '@/hooks/useSidebar'
import { useUiPrefsStore } from '@/stores/uiPrefsStore'
import { Appearance } from '@/routes/Appearance'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'
import { DEFAULT_CUSTOM_DARK, DEFAULT_CUSTOM_LIGHT } from '@/stores/themeStore'

function renderPage() {
  const router = createMemoryRouter([{ path: '/', element: <Appearance /> }])
  return render(<RouterProvider router={router} />)
}

beforeEach(() => {
  localStorage.clear()
  applyLook({
    mode: 'light',
    palette: 'default',
    custom: { light: { ...DEFAULT_CUSTOM_LIGHT }, dark: { ...DEFAULT_CUSTOM_DARK } },
    scale: 1,
    menu: [...DEFAULT_MENU_ORDER],
    presets: [],
  })
  useSettingsGuardStore.setState({ changes: 0, nudges: 0 })
  useSidebar.setState({
    preview: null,
    previewOrder: null,
    ordering: false,
    order: [...DEFAULT_MENU_ORDER],
    settings: { disabled: false, isHoverOpen: true },
  })
  useUiPrefsStore.setState({ zoomShortcut: false, scalePresets: [] })
})

describe('Appearance page', () => {
  it('previews an edit, locks the app, and puts everything back on Cancel', async () => {
    renderPage()
    expect(screen.queryByRole('alert')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: '130%' }))
    await waitFor(() => expect(document.documentElement.style.fontSize).toBe('20.8px'))
    expect(screen.getByRole('alert')).toBeTruthy()
    expect(useSettingsGuardStore.getState().changes).toBeGreaterThan(0)
    // Previewed on the page, not kept.
    expect(currentLook().scale).toBe(1)

    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }))
    expect(screen.queryByRole('alert')).toBeNull()
    expect(document.documentElement.style.fontSize).toBe('16px')
    expect(useSettingsGuardStore.getState().changes).toBe(0)
  })

  it('previews the sidebar switches too, and keeps them only on Save', async () => {
    renderPage()
    fireEvent.click(screen.getByLabelText('settings.appearance_hide_sidebar'))
    expect(useSidebar.getState().preview).toMatchObject({ disabled: true })
    expect(useSidebar.getState().settings.disabled).toBe(false)
    expect(localStorage.getItem('mokora.sidebar') ?? '').not.toContain('preview')

    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }))
    expect(useSidebar.getState().preview).toMatchObject({ disabled: false })

    fireEvent.click(screen.getByLabelText('settings.appearance_hide_sidebar'))
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }))
    await waitFor(() => expect(useSidebar.getState().settings.disabled).toBe(true))
  })

  it('keeps the zoom shortcut switch only on Save', async () => {
    renderPage()
    fireEvent.click(screen.getByLabelText('settings.look.zoom_shortcut'))
    expect(useUiPrefsStore.getState().zoomShortcut).toBe(false)
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }))
    await waitFor(() => expect(useUiPrefsStore.getState().zoomShortcut).toBe(true))
  })

  it('sets the sidebar order: opens it, follows the moves, and Cancel leaves it as it was', async () => {
    renderPage()
    const set = screen.getByRole('button', { name: /settings.look.menu_order_set/ })
    expect(screen.queryByRole('button', { name: 'common.cancel' })).toBeNull()
    fireEvent.click(set)
    expect(useSidebar.getState().ordering).toBe(true)
    // The cancel for the setting appears next to it.
    expect(screen.getByRole('button', { name: 'common.cancel' })).toBeTruthy()

    // A page moved in the sidebar is a change waiting to be saved.
    const moved = ['game', ...DEFAULT_MENU_ORDER.filter((id) => id !== 'game')]
    act(() => useSidebar.getState().setPreviewOrder(moved))
    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy())
    expect(useSidebar.getState().order).toEqual([...DEFAULT_MENU_ORDER])

    fireEvent.click(screen.getAllByRole('button', { name: 'common.cancel' })[0])
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
    expect(useSidebar.getState().ordering).toBe(false)
    expect(useSidebar.getState().previewOrder).toEqual([...DEFAULT_MENU_ORDER])
  })

  it('keeps the order on Save, and saving ends the setting', async () => {
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: /settings.look.menu_order_set/ }))
    const moved = ['game', ...DEFAULT_MENU_ORDER.filter((id) => id !== 'game')]
    act(() => useSidebar.getState().setPreviewOrder(moved))
    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy())
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }))
    await waitFor(() => expect(useSidebar.getState().order).toEqual(moved))
    await waitFor(() => expect(useSidebar.getState().ordering).toBe(false))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('slides a new size button out of the plus, and saves it only when it is a size that can be one', async () => {
    renderPage()
    const field = screen.getByLabelText('settings.look.custom_scale') as HTMLInputElement
    const plus = screen.getByRole('button', { name: 'settings.look.custom_scale_add' })
    expect(field.closest('[aria-hidden]')?.getAttribute('aria-hidden')).toBe('true')

    fireEvent.click(plus)
    // The button is out, and that alone is a change waiting: the bar is up
    // and Save is out of reach until a size is in.
    expect(field.closest('[aria-hidden]')?.getAttribute('aria-hidden')).toBe('false')
    expect(screen.getByRole('alert')).toBeTruthy()
    const save = screen.getByRole('button', { name: 'common.save' }) as HTMLButtonElement
    expect(save.disabled).toBe(true)
    expect((plus as HTMLButtonElement).disabled).toBe(true)

    // A typed size is no change of the interface's size.
    fireEvent.change(field, { target: { value: '90' } })
    expect(save.disabled).toBe(false)
    expect(document.documentElement.style.fontSize).toBe('16px')

    // Out of the limits, not a number, empty, or a button already.
    for (const out of ['20', '151', '', 'abc', '100', '85']) {
      fireEvent.change(field, { target: { value: out } })
      expect(save.disabled).toBe(true)
    }
    fireEvent.change(field, { target: { value: '200' } })
    expect(field.getAttribute('aria-invalid')).toBe('true')
    // The limits themselves are inside.
    for (const edge of ['70', '150']) {
      fireEvent.change(field, { target: { value: edge } })
      expect(save.disabled).toBe(false)
    }

    // Cancel: the field slides back into the plus and nothing was added.
    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }))
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
    expect(field.closest('[aria-hidden]')?.getAttribute('aria-hidden')).toBe('true')
    expect(field.value).toBe('')
    expect(screen.queryByRole('button', { name: '150%' })).toBeNull()
    expect(useUiPrefsStore.getState().scalePresets).toEqual([])
    expect((screen.getByRole('button', { name: 'settings.look.custom_scale_add' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('adds the typed size as a button of its own on Save, and it can be taken away again', async () => {
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'settings.look.custom_scale_add' }))
    const field = screen.getByLabelText('settings.look.custom_scale') as HTMLInputElement
    fireEvent.change(field, { target: { value: '120' } })
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }))
    await waitFor(() => expect(useUiPrefsStore.getState().scalePresets).toEqual([120]))
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
    expect(field.closest('[aria-hidden]')?.getAttribute('aria-hidden')).toBe('true')
    // The size itself did not change; the button is there to press.
    expect(currentLook().scale).toBe(1)
    expect(screen.getByRole('button', { name: '120%' })).toBeTruthy()

    // Pressing it sizes the interface, like the built-in ones.
    fireEvent.click(screen.getByRole('button', { name: '120%' }))
    await waitFor(() => expect(document.documentElement.style.fontSize).toBe('19.2px'))
    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }))

    // Taking it away is a change too, kept on Save.
    fireEvent.click(screen.getByRole('button', { name: 'settings.look.preset_remove' }))
    await waitFor(() => expect(screen.queryByRole('button', { name: '120%' })).toBeNull())
    expect(useUiPrefsStore.getState().scalePresets).toEqual([120])
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }))
    await waitFor(() => expect(useUiPrefsStore.getState().scalePresets).toEqual([]))
  })

  it('stops at ten added buttons', () => {
    useUiPrefsStore.setState({ scalePresets: [70, 71, 72, 73, 74, 75, 76, 77, 78, 79] })
    applyLook({ ...currentLook() })
    renderPage()
    expect((screen.getByRole('button', { name: 'settings.look.custom_scale_add' }) as HTMLButtonElement).disabled).toBe(true)
  })
})
