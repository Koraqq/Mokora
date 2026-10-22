import { describe, expect, it } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { mockMatchMedia } from '@/testing/setup'
import { LG_BREAKPOINT_PX, useIsNarrow } from './useIsNarrow'

describe('useIsNarrow', () => {
  it('mirrors Tailwind’s lg breakpoint', () => {
    // If this drifts from Tailwind's `lg`, the drawer and the `lg:ml-*`
    // margins in App.tsx would disagree about who owns the layout.
    expect(LG_BREAKPOINT_PX).toBe(1024)
  })

  it('reports narrow when the media query matches', () => {
    mockMatchMedia(true)
    const { result } = renderHook(() => useIsNarrow())
    expect(result.current).toBe(true)
  })

  it('reports docked when the media query does not match', () => {
    mockMatchMedia(false)
    const { result } = renderHook(() => useIsNarrow())
    expect(result.current).toBe(false)
  })

  it('re-renders when the viewport crosses the breakpoint', () => {
    const setMatches = mockMatchMedia(false)
    const { result } = renderHook(() => useIsNarrow())
    expect(result.current).toBe(false)

    act(() => setMatches(true))
    expect(result.current).toBe(true)

    act(() => setMatches(false))
    expect(result.current).toBe(false)
  })

  it('falls back to docked when matchMedia is unavailable', () => {
    // @ts-expect-error — deliberately simulating a host without matchMedia.
    window.matchMedia = undefined
    const { result } = renderHook(() => useIsNarrow())
    expect(result.current).toBe(false)
  })
})
