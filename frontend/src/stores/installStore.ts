import { create } from 'zustand'

// Drives the global blocking overlay shown while a bot's Python environment
// is being installed (GitHub install / reinstall / "Reinstall environment"
// sync). `count` is a counter rather than a bool so overlapping operations
// don't clear the overlay early. `title`/`body` mirror the latest backend
// progress notification (ids `bot-install-*` / `bot-sync-*`).
type InstallStore = {
  count: number
  title: string | null
  body: string | null
  begin: () => void
  end: () => void
  setProgress: (n: { title: string; body?: string | null }) => void
}

export const useInstallStore = create<InstallStore>((set) => ({
  count: 0,
  title: null,
  body: null,
  begin: () => set((s) => ({ count: s.count + 1 })),
  end: () =>
    set((s) => {
      const count = Math.max(0, s.count - 1)
      return count === 0 ? { count, title: null, body: null } : { count }
    }),
  setProgress: (n) => set({ title: n.title, body: n.body ?? null }),
}))
