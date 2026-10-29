import { create } from 'zustand'

import { HAS_TAURI, invoke } from '@/lib/tauri'
import { ANNOUNCEMENTS, type AnnouncementEntry } from '@/announcements/entries'
import { selectUnseenEntries } from '@/announcements/select'

// ISO date of the newest announcement the user has already seen. Absent
// on fresh installs and on the first run of a build that introduced the
// system — `selectUnseenEntries` treats that as "show the newest few".
const LAST_SEEN_KEY = 'mokora.announcement.lastSeen'

function loadLastSeen(): string | null {
  if (typeof localStorage === 'undefined') return null
  try {
    return localStorage.getItem(LAST_SEEN_KEY)
  } catch {
    return null
  }
}

type AnnouncementStore = {
  /** Persisted baseline; null until the user closes their first showing. */
  lastSeenDate: string | null
  /** Every bundled entry, newest first, whenever the dialog is open. */
  entries: AnnouncementEntry[]
  open: boolean
  /** True while an armed launch showing hasn't been opened yet. */
  launchShowPending: boolean
  /**
   * Launch path: use unseen entries only to decide whether the dialog
   * should open. The dialog itself always receives the complete history,
   * exactly like the manual entry point.
   */
  prepareLaunch: () => boolean
  /** Take over what any other account already showed (one answer for all). */
  hydrate: () => Promise<void>
  /** Another account interface closed the announcements up to `date`. */
  seenElsewhere: (date: string) => void
  /** Actually open the armed launch showing (after the caller's delay). */
  showLaunch: () => void
  /** Settings entry point: browse the full announcement history. */
  openHistory: () => void
  /** Any close (Got it / X / Esc / outside click) marks the shown entries seen. */
  close: () => void
}

export const useAnnouncementStore = create<AnnouncementStore>((set, get) => ({
  lastSeenDate: loadLastSeen(),
  entries: [],
  open: false,
  launchShowPending: false,

  prepareLaunch: () => {
    const unseen = selectUnseenEntries(ANNOUNCEMENTS, get().lastSeenDate)
    if (unseen.length === 0) return false
    set({ entries: [...ANNOUNCEMENTS], launchShowPending: true })
    return true
  },

  hydrate: async () => {
    if (!HAS_TAURI) return
    try {
      const shared = await invoke<string | null>('get_announcement_seen')
      const own = get().lastSeenDate
      if (shared && (own === null || shared > own)) set({ lastSeenDate: shared })
    } catch {
      /* keep this interface's own record */
    }
  },

  seenElsewhere: (date) => {
    set((s) => ({
      lastSeenDate: s.lastSeenDate === null || date > s.lastSeenDate ? date : s.lastSeenDate,
      // Only a launch showing in a hidden interface can still be open here.
      open: false,
      launchShowPending: false,
    }))
  },

  showLaunch: () => {
    if (get().launchShowPending) set({ open: true })
  },

  openHistory: () => {
    set({ entries: [...ANNOUNCEMENTS], open: true })
  },

  close: () => {
    set((s) => {
      // The newest shown entry's date becomes the baseline. Only ever
      // advance: the history view can show older entries the user has
      // already seen, and closing it must not regress the baseline.
      const newest = s.entries.reduce<string | null>(
        (max, e) => (max === null || e.date > max ? e.date : max),
        null,
      )
      const advance =
        newest !== null && (s.lastSeenDate === null || newest > s.lastSeenDate)
      if (advance) {
        try {
          localStorage.setItem(LAST_SEEN_KEY, newest)
        } catch {
          /* quota — ignore */
        }
        if (HAS_TAURI) {
          void Promise.resolve(invoke('set_announcement_seen', { date: newest })).catch(() => {})
        }
      }
      return {
        open: false,
        launchShowPending: false,
        lastSeenDate: advance ? newest : s.lastSeenDate,
      }
    })
  },
}))
