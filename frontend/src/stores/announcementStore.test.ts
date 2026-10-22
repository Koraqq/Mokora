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

const LAST_SEEN_KEY = 'mokora.announcement.lastSeen'

type Fixture = { id: string; date: string; version?: string }

function fixtureEntry({ id, date, version }: Fixture) {
  return { id, date, version, features: [{ icon: () => null, key: 'x' }] }
}

// The store reads localStorage at module-init and bakes in the bundled
// data, so each test mocks the data module and re-imports a fresh copy.
async function freshStore(fixtures: Fixture[]) {
  vi.resetModules()
  vi.doMock('@/announcements/entries', () => ({
    ANNOUNCEMENTS: fixtures.map(fixtureEntry),
  }))
  const mod = await import('./announcementStore')
  return mod.useAnnouncementStore
}

const V360: Fixture = { id: 'v3_6_0', date: '2026-09-01', version: '3.6.0' }
const V350: Fixture = { id: 'v3_5_0', date: '2026-08-12', version: '3.5.0' }
const NEWS: Fixture = { id: 'news', date: '2026-08-09' }

describe('announcementStore', () => {
  beforeEach(() => {
    stubLocalStorage()
  })

  it('arms the launch showing when there are unseen entries', async () => {
    localStorage.setItem(LAST_SEEN_KEY, '2026-08-12')
    const store = await freshStore([V360, V350, NEWS])

    expect(store.getState().prepareLaunch()).toBe(true)
    expect(store.getState().launchShowPending).toBe(true)
    expect(store.getState().open).toBe(false)
    expect(store.getState().entries.map((e) => e.id)).toEqual(['v3_6_0'])

    store.getState().showLaunch()
    expect(store.getState().open).toBe(true)
  })

  it('replays skipped entries, news included', async () => {
    localStorage.setItem(LAST_SEEN_KEY, '2026-08-01')
    const store = await freshStore([V360, V350, NEWS])

    store.getState().prepareLaunch()
    expect(store.getState().entries.map((e) => e.id)).toEqual([
      'v3_6_0',
      'v3_5_0',
      'news',
    ])
  })

  it('does nothing when the baseline is current, and never opens un-armed', async () => {
    localStorage.setItem(LAST_SEEN_KEY, '2026-09-01')
    const store = await freshStore([V360, V350, NEWS])

    expect(store.getState().prepareLaunch()).toBe(false)
    expect(store.getState().launchShowPending).toBe(false)
    store.getState().showLaunch()
    expect(store.getState().open).toBe(false)
  })

  it('records the newest shown entry as seen on close, across restarts', async () => {
    const store = await freshStore([V360, V350, NEWS])
    store.getState().prepareLaunch()
    store.getState().showLaunch()
    store.getState().close()

    expect(store.getState().open).toBe(false)
    expect(store.getState().launchShowPending).toBe(false)
    expect(localStorage.getItem(LAST_SEEN_KEY)).toBe('2026-09-01')

    // "Restart": a fresh import must load the persisted baseline and
    // decide there is nothing left to show.
    const restarted = await freshStore([V360, V350, NEWS])
    expect(restarted.getState().lastSeenDate).toBe('2026-09-01')
    expect(restarted.getState().prepareLaunch()).toBe(false)
  })

  it('never moves the baseline backwards on close', async () => {
    // Baseline sits ahead of every bundled entry (e.g. a future-dated
    // manual override). Opening history shows the full list, but closing
    // it must not drag the baseline back to the newest entry's date.
    localStorage.setItem(LAST_SEEN_KEY, '2099-01-01')
    const store = await freshStore([V360, V350, NEWS])

    expect(store.getState().prepareLaunch()).toBe(false)
    store.getState().openHistory()
    expect(store.getState().entries.map((e) => e.id)).toEqual([
      'v3_6_0',
      'v3_5_0',
      'news',
    ])
    store.getState().close()
    expect(localStorage.getItem(LAST_SEEN_KEY)).toBe('2099-01-01')
  })

  it('history mode shows every entry, launch state untouched', async () => {
    localStorage.setItem(LAST_SEEN_KEY, '2026-09-01')
    const store = await freshStore([V360, V350, NEWS])
    store.getState().prepareLaunch()

    store.getState().openHistory()
    expect(store.getState().open).toBe(true)
    expect(store.getState().entries.map((e) => e.id)).toEqual([
      'v3_6_0',
      'v3_5_0',
      'news',
    ])
  })
})
