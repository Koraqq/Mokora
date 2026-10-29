import { beforeEach, describe, expect, it, vi } from 'vitest'

// Node 22+ ships an experimental `localStorage` global that is undefined
// without `--localstorage-file` and shadows jsdom's, so install an explicit
// in-memory stand-in the store and the assertions both see.
function stubLocalStorage() {
  const backing = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => backing.get(k) ?? null,
    setItem: (k: string, v: string) => void backing.set(k, String(v)),
    removeItem: (k: string) => void backing.delete(k),
    clear: () => backing.clear(),
  })
  return backing
}

// The store reads localStorage at module-init, so each test re-imports a
// fresh copy after seeding storage to exercise the load path.
async function freshStore() {
  vi.resetModules()
  const mod = await import('./uiPrefsStore')
  return mod
}

describe('uiPrefsStore dashboard onboarding flag', () => {
  beforeEach(() => {
    stubLocalStorage()
  })

  it('defaults to not onboarded on first launch', async () => {
    const { useUiPrefsStore } = await freshStore()
    expect(useUiPrefsStore.getState().dashboardOnboarded).toBe(false)
  })

  it('persists across restarts', async () => {
    const { useUiPrefsStore } = await freshStore()
    useUiPrefsStore.getState().markDashboardOnboarded()
    expect(localStorage.getItem('mokora.dashboard.onboarded')).toBe('1')

    const restarted = await freshStore()
    expect(restarted.useUiPrefsStore.getState().dashboardOnboarded).toBe(true)
  })

  it('keeps the size buttons the player added, cleaned, and across a restart', async () => {
    const first = await freshStore()
    expect(first.useUiPrefsStore.getState().scalePresets).toEqual([])
    // Only sizes that can be buttons, once each, ascending, at most ten.
    first.useUiPrefsStore.getState().setScalePresets([120, 69, 100, 90, 120, 151, 90.5])
    expect(first.useUiPrefsStore.getState().scalePresets).toEqual([90, 120])
    const restarted = await freshStore()
    expect(restarted.useUiPrefsStore.getState().scalePresets).toEqual([90, 120])

    restarted.useUiPrefsStore.getState().setScalePresets(Array.from({ length: 14 }, (_, i) => 70 + i))
    expect(restarted.useUiPrefsStore.getState().scalePresets).toHaveLength(10)
  })

  it('tells which sizes may become a button', async () => {
    const { canAddScalePreset } = await freshStore()
    expect(canAddScalePreset(90, [])).toBe(true)
    expect(canAddScalePreset(70, [])).toBe(true)
    expect(canAddScalePreset(150, [])).toBe(true)
    expect(canAddScalePreset(69, [])).toBe(false)
    expect(canAddScalePreset(151, [])).toBe(false)
    expect(canAddScalePreset(100, [])).toBe(false)
    expect(canAddScalePreset(90, [90])).toBe(false)
    expect(canAddScalePreset(95, [70, 71, 72, 73, 74, 75, 76, 77, 78, 79])).toBe(false)
  })
})
