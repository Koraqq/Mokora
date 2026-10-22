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
})
