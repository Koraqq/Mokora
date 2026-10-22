import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'

import { mockMatchMedia } from '@/testing/setup'
import { useSidebar } from '@/hooks/useSidebar'
import { Sidebar } from './Sidebar'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: 'en', changeLanguage: vi.fn() },
  }),
  // Sidebar pulls in `@/i18n` for the language picker, and that module calls
  // `.use(initReactI18next)` at import time.
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.0.0-test'),
}))

// The account picker lists accounts and listens for changes; with no
// accounts it renders nothing.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

function renderSidebar(initialPath = '/') {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Sidebar />
    </MemoryRouter>,
  )
}

function openDrawer() {
  act(() => useSidebar.getState().setDrawerOpen(true))
}

describe('Sidebar drawer (below lg)', () => {
  beforeEach(() => {
    useSidebar.setState({
      isOpen: true,
      isHover: false,
      isDrawerOpen: false,
      settings: { disabled: false, isHoverOpen: true },
    })
  })

  it('marks the closed drawer inert so its links leave the Tab order', () => {
    mockMatchMedia(true)
    const { container } = renderSidebar()
    const aside = container.querySelector('aside')!
    expect(aside.hasAttribute('inert')).toBe(true)

    openDrawer()
    expect(aside.hasAttribute('inert')).toBe(false)
  })

  it('never marks the docked sidebar inert', () => {
    mockMatchMedia(false)
    const { container } = renderSidebar()
    expect(container.querySelector('aside')!.hasAttribute('inert')).toBe(false)
  })

  it('closes on Escape', () => {
    mockMatchMedia(true)
    renderSidebar()
    openDrawer()

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(useSidebar.getState().isDrawerOpen).toBe(false)
  })

  it('ignores Escape while docked', () => {
    mockMatchMedia(false)
    renderSidebar()
    // A stale drawer flag must not let a docked sidebar swallow Escape from
    // whatever dialog is actually focused.
    act(() => useSidebar.getState().setDrawerOpen(true))
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(useSidebar.getState().isDrawerOpen).toBe(true)
  })

  it('closes when the backdrop is clicked', () => {
    mockMatchMedia(true)
    const { container } = renderSidebar()
    openDrawer()

    const backdrop = container.querySelector('[aria-hidden="true"].fixed.inset-0')
    expect(backdrop).not.toBeNull()
    fireEvent.click(backdrop!)
    expect(useSidebar.getState().isDrawerOpen).toBe(false)
  })

  it('renders no backdrop while docked', () => {
    mockMatchMedia(false)
    const { container } = renderSidebar()
    act(() => useSidebar.getState().setDrawerOpen(true))
    expect(container.querySelector('[aria-hidden="true"].fixed.inset-0')).toBeNull()
  })

  // Regression: the close-on-navigate effect was keyed on `pathname`, so
  // tapping the nav item for the route you were already on pushed a new
  // history entry without changing the path — and the drawer stayed open,
  // covering the page.
  it('closes when navigating to the route it is already on', () => {
    mockMatchMedia(true)
    renderSidebar('/play')
    openDrawer()

    // `nav.play` points at '/play', the route MemoryRouter starts on.
    fireEvent.click(screen.getByRole('link', { name: 'nav.play' }))
    expect(useSidebar.getState().isDrawerOpen).toBe(false)
  })

  it('closes when navigating to a different route', () => {
    mockMatchMedia(true)
    renderSidebar('/')
    openDrawer()

    fireEvent.click(screen.getByRole('link', { name: 'nav.game' }))
    expect(useSidebar.getState().isDrawerOpen).toBe(false)
  })

  it('drops the drawer when the window grows back past lg', () => {
    const setMatches = mockMatchMedia(true)
    renderSidebar()
    openDrawer()
    expect(useSidebar.getState().isDrawerOpen).toBe(true)

    act(() => setMatches(false))
    expect(useSidebar.getState().isDrawerOpen).toBe(false)
  })

  it('does not track hover state in drawer mode', () => {
    mockMatchMedia(true)
    const { container } = renderSidebar()
    fireEvent.mouseEnter(container.querySelector('aside > div')!)
    expect(useSidebar.getState().isHover).toBe(false)
  })
})
