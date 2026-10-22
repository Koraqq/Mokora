import { create } from 'zustand'

/**
 * Unsaved settings: while the settings page holds changes, `changes` counts
 * them and the rest of the app (sidebar, top bar, status bar) is locked until
 * they are saved or cancelled. A press on a locked part `nudge`s — the
 * settings page's bar shakes to point at the choice still to make.
 */
type SettingsGuardStore = {
  changes: number
  /** Bumped on every nudge; the bar replays its shake when it changes. */
  nudges: number
  setChanges: (changes: number) => void
  nudge: () => void
}

export const useSettingsGuardStore = create<SettingsGuardStore>((set) => ({
  changes: 0,
  nudges: 0,
  setChanges: (changes) => set({ changes }),
  nudge: () => set((s) => ({ nudges: s.nudges + 1 })),
}))
