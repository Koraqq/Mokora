import { create } from 'zustand'

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
  /** Entries the open dialog is showing, newest first. */
  entries: AnnouncementEntry[]
  open: boolean
  /** True while an armed launch showing hasn't been opened yet. */
  launchShowPending: boolean
  /**
   * Launch path: pick the unseen entries and arm the dialog. Returns
   * whether there is anything to show; the caller opens it after its own
   * grace delay.
   */
  prepareLaunch: () => boolean
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
    set({ entries: unseen, launchShowPending: true })
    return true
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
      }
      return {
        open: false,
        launchShowPending: false,
        lastSeenDate: advance ? newest : s.lastSeenDate,
      }
    })
  },
}))
