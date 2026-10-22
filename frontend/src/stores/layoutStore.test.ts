import { beforeEach, describe, expect, it, vi } from 'vitest'

// The same in-memory localStorage stand-in as uiPrefsStore.test.ts: Node's
// experimental global would shadow jsdom's.
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

// The store reads localStorage at module-init: a fresh copy per account.
async function freshStore() {
  vi.resetModules()
  return import('./layoutStore')
}

describe('layoutStore shared layouts ("sync layout")', () => {
  beforeEach(() => {
    stubLocalStorage()
  })

  it('carries both modes from one account to another', async () => {
    const { useLayoutStore } = await freshStore()
    const s = useLayoutStore.getState()
    // Account A: a tile moved in 3p, one hidden in 4p.
    s.setMode('3p')
    const moved = s.layouts3p.lg.map((l) => (l.i === 'board' ? { ...l, x: 3, y: 0 } : l))
    useLayoutStore.getState().setLayouts({ ...useLayoutStore.getState().layouts, lg: moved })
    useLayoutStore.getState().setMode('4p')
    useLayoutStore.getState().hide('events', 'lg')
    const shared = JSON.parse(JSON.stringify(useLayoutStore.getState().exportShared()))

    // Account B: its own storage, defaults.
    stubLocalStorage()
    const b = await freshStore()
    b.useLayoutStore.getState().setMode('3p')
    expect(b.useLayoutStore.getState().applyShared(shared)).toBe(true)
    const after = b.useLayoutStore.getState()
    const board = after.layouts3p.lg.find((l) => l.i === 'board')!
    expect([board.x, board.y]).toEqual([3, 0])
    expect(after.layouts.lg.find((l) => l.i === 'board')!.x).toBe(3) // 3p is on show
    expect(after.hidden4p.lg).toContain('events')

    // …and it is B's own now: a later start reads it back.
    const again = await freshStore()
    expect(again.useLayoutStore.getState().hidden4p.lg).toContain('events')
  })

  it('keeps the constraints of this version', async () => {
    const { readShared, useLayoutStore } = await freshStore()
    const shared = useLayoutStore.getState().exportShared()
    // Saved items carry only positions and sizes; constraints come back.
    const header = readShared(shared)!.layouts4p.lg.find((l) => l.i === 'header')!
    expect(header.minW).toBeGreaterThan(0)
  })

  it('refuses what it cannot read', async () => {
    const { readShared, useLayoutStore } = await freshStore()
    expect(readShared(null)).toBeNull()
    expect(readShared({ schema: 999 })).toBeNull()
    expect(useLayoutStore.getState().applyShared('nonsense')).toBe(false)
  })
})
