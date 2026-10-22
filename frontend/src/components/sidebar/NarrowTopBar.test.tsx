import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'

import { mockMatchMedia } from '@/testing/setup'
import { useSidebar } from '@/hooks/useSidebar'
import { NarrowTopBar } from './NarrowTopBar'

// The bar only needs `t` as an identity function — asserting on raw keys keeps
// the test independent of copy changes across the four locales.
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

function resetSidebar() {
  useSidebar.setState({
    isOpen: true,
    isHover: false,
    isDrawerOpen: false,
    settings: { disabled: false, isHoverOpen: true },
  })
}

describe('NarrowTopBar', () => {
  beforeEach(resetSidebar)

  // Regression: the sidebar was hidden by `lg:translate-x-0` below 1024px
  // while its only reveal controls lived inside the off-screen element,
  // leaving the app un-navigable between the window's min width and `lg`.
  it('renders a navigation trigger below the lg breakpoint', () => {
    mockMatchMedia(true)
    render(<NarrowTopBar />)
    expect(screen.getByRole('button', { name: 'sidebar.openMenu' })).toBeTruthy()
  })

  it('opens the drawer when the trigger is clicked', () => {
    mockMatchMedia(true)
    render(<NarrowTopBar />)
    expect(useSidebar.getState().isDrawerOpen).toBe(false)

    fireEvent.click(screen.getByRole('button', { name: 'sidebar.openMenu' }))
    expect(useSidebar.getState().isDrawerOpen).toBe(true)
  })

  it('renders nothing once the sidebar can dock', () => {
    mockMatchMedia(false)
    render(<NarrowTopBar />)
    expect(screen.queryByRole('button', { name: 'sidebar.openMenu' })).toBeNull()
  })

  it('renders nothing when the user disabled the sidebar', () => {
    mockMatchMedia(true)
    useSidebar.setState({ settings: { disabled: true, isHoverOpen: true } })
    render(<NarrowTopBar />)
    expect(screen.queryByRole('button', { name: 'sidebar.openMenu' })).toBeNull()
  })

  it('appears when the viewport shrinks past the breakpoint', () => {
    const setMatches = mockMatchMedia(false)
    render(<NarrowTopBar />)
    expect(screen.queryByRole('button', { name: 'sidebar.openMenu' })).toBeNull()

    act(() => setMatches(true))
    expect(screen.getByRole('button', { name: 'sidebar.openMenu' })).toBeTruthy()
  })
})

describe('useSidebar persistence', () => {
  beforeEach(resetSidebar)

  it('never persists transient drawer state', () => {
    useSidebar.setState({ isDrawerOpen: true, isHover: true })
    const persisted = JSON.parse(localStorage.getItem('mokora.sidebar') ?? '{}')
    expect(persisted.state).not.toHaveProperty('isDrawerOpen')
    expect(persisted.state).not.toHaveProperty('isHover')
    expect(persisted.state).toHaveProperty('isOpen')
  })
})
