import { useEffect, useState } from 'react'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { AppearanceSection } from '@/components/settings/AppearanceSection'
import type { AppearanceDraft } from '@/components/settings/draft'
import { DEFAULT_MENU_ORDER } from '@/lib/menuList'
import { DEFAULT_CUSTOM_DARK, DEFAULT_CUSTOM_LIGHT } from '@/stores/themeStore'

const start: AppearanceDraft = {
  themeMode: 'system',
  themePalette: 'default',
  customTheme: { light: { ...DEFAULT_CUSTOM_LIGHT }, dark: { ...DEFAULT_CUSTOM_DARK } },
  uiScale: 1,
  menuOrder: [...DEFAULT_MENU_ORDER],
  scalePresets: [],
  sharedAppearance: false,
  sidebarHoverOpen: true,
  sidebarDisabled: false,
  zoomShortcut: false,
}

let latest: AppearanceDraft = start

function Harness() {
  const [draft, setDraft] = useState(start)
  useEffect(() => {
    latest = draft
  }, [draft])
  return <AppearanceSection draft={draft} setDraft={setDraft} />
}

describe('AppearanceSection', () => {
  it('picks a mode and a palette as radios', () => {
    render(<Harness />)
    const modes = screen.getByRole('radiogroup', { name: 'settings.mode' })
    const modeOrder = [...modes.querySelectorAll('[role=radio]')].map((r) => r.textContent)
    expect(modeOrder).toEqual(['settings.mode_system', 'settings.mode_light', 'settings.mode_dark'])
    fireEvent.click(modes.querySelectorAll('[role=radio]')[2])
    expect(latest.themeMode).toBe('dark')

    const palettes = screen.getByRole('radiogroup', { name: 'settings.theme' })
    const paletteOrder = [...palettes.querySelectorAll('[role=radio]')].map((r) => r.textContent)
    expect(paletteOrder).toEqual([
      'settings.theme_mokora',
      'settings.theme_default',
      'settings.theme_crimson',
      'settings.theme_slate',
      'settings.theme_custom',
    ])
    const mokora = palettes.querySelectorAll('[role=radio]')[0]
    fireEvent.click(mokora)
    expect(latest.themePalette).toBe('mokora')
    expect(mokora.getAttribute('aria-checked')).toBe('true')
  })

  it('shows the custom editor only for the custom palette', () => {
    render(<Harness />)
    expect(screen.queryByText('settings.custom_hint')).toBeNull()
    const palettes = screen.getByRole('radiogroup', { name: 'settings.theme' })
    fireEvent.click(palettes.querySelectorAll('[role=radio]')[4])
    expect(screen.getByText('settings.custom_hint')).toBeTruthy()
  })

  it('sets the scale from a preset and turns sharing on', async () => {
    render(<Harness />)
    fireEvent.click(screen.getByRole('button', { name: '115%' }))
    // A preset glides to its size rather than jumping.
    await waitFor(() => expect(latest.uiScale).toBe(1.15))
    const shared = screen.getByRole('button', { name: 'settings.look.shared' })
    expect(shared.getAttribute('aria-pressed')).toBe('false')
    fireEvent.click(shared)
    expect(latest.sharedAppearance).toBe(true)
    expect(shared.getAttribute('aria-pressed')).toBe('true')
  })

  it('moves the scale from the liquid slider by keyboard', () => {
    render(<Harness />)
    const slider = screen.getByRole('slider', { name: 'settings.ui_scale' })
    expect(slider.getAttribute('aria-valuenow')).toBe('1')
    fireEvent.keyDown(slider, { key: 'ArrowRight' })
    expect(latest.uiScale).toBe(1.05)
    fireEvent.keyDown(slider, { key: 'End' })
    expect(latest.uiScale).toBe(1.5)
    fireEvent.keyDown(slider, { key: 'Home' })
    expect(latest.uiScale).toBe(0.7)
  })

  it('switches the zoom shortcut on', () => {
    render(<Harness />)
    fireEvent.click(screen.getByLabelText('settings.look.zoom_shortcut'))
    expect(latest.zoomShortcut).toBe(true)
  })

  it('restores every default at once', async () => {
    render(<Harness />)
    const reset = screen.getByRole('button', { name: /settings.look.reset_all/ })
    expect((reset as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: '130%' }))
    await waitFor(() => expect((reset as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(reset)
    expect(latest).toMatchObject({ uiScale: 1, themeMode: 'system', themePalette: 'default' })
  })

  it('plays the sun when it is picked, and fills the moon in', () => {
    const { container } = render(<Harness />)
    const icons = () => [...container.querySelectorAll('[role=radio] svg')] as SVGElement[]
    // Nothing plays on load.
    expect(icons().some((svg) => svg.getAttribute('class')?.includes('mokora-sun-pop'))).toBe(false)

    const radios = container.querySelectorAll('[role=radio]')
    // Order: system, light, dark.
    fireEvent.click(radios[1])
    expect(icons()[1].getAttribute('class')).toContain('mokora-sun-pop')
    fireEvent.click(radios[2])
    expect(icons()[1].getAttribute('class')).not.toContain('mokora-sun-pop')
    expect(icons()[2].style.fill).toBe('currentcolor')
    expect(icons()[1].style.fill).toBe('')
    fireEvent.click(radios[0])
    expect(icons()[2].style.fill).toBe('transparent')
  })
})
