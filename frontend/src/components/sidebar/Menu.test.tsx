import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'

import { mockMatchMedia } from '@/testing/setup'
import { useSidebar } from '@/hooks/useSidebar'
import { DEFAULT_MENU_ORDER } from '@/lib/menuList'
import { Menu } from './Menu'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

function renderMenu() {
  return render(
    <MemoryRouter>
      <Menu isOpen />
    </MemoryRouter>,
  )
}

const labels = () => [...document.querySelectorAll('nav ul li > div p')].map((p) => p.textContent)

beforeEach(() => {
  mockMatchMedia(false)
  useSidebar.setState({
    order: [...DEFAULT_MENU_ORDER],
    previewOrder: null,
    ordering: false,
    picked: null,
  })
})

describe('Menu', () => {
  it('lists the pages in the kept order, and a preview goes over it', () => {
    useSidebar.setState({ order: ['settings', ...DEFAULT_MENU_ORDER.filter((id) => id !== 'settings')] })
    renderMenu()
    expect(labels()[0]).toBe('nav.settings')
    useSidebar.setState({ previewOrder: [...DEFAULT_MENU_ORDER] })
  })

  it('picks pages instead of opening them while the order is set, and moves the picked one', () => {
    useSidebar.setState({ ordering: true, previewOrder: [...DEFAULT_MENU_ORDER] })
    renderMenu()
    expect(screen.queryAllByRole('link')).toHaveLength(0)
    expect(screen.queryByLabelText('settings.look.menu_up')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /nav.game/ }))
    expect(useSidebar.getState().picked).toBe('game')
    fireEvent.click(screen.getByLabelText('settings.look.menu_up'))
    expect(useSidebar.getState().previewOrder?.slice(0, 3)).toEqual(['play', 'game', 'profile'])
    // The kept order is not touched.
    expect(useSidebar.getState().order.slice(0, 3)).toEqual(['play', 'profile', 'game'])
    fireEvent.click(screen.getByLabelText('settings.look.menu_down'))
    expect(useSidebar.getState().previewOrder?.slice(0, 3)).toEqual(['play', 'profile', 'game'])
  })
})
