import { create } from 'zustand'
import type { LogEntry, LogSessionInfo } from '@/types'

/**
 * Log viewer store.
 *
 * Holds a bounded ring buffer of `LogEntry` rows for the currently-viewed
 * session, plus the filter state and metadata about which session is
 * loaded. Live-tail arrivals (from `useLogStream`) and initial-load reads
 * (from `read_log_session`) both append into the same buffer — the UI
 * never has to merge two streams.
 *
 * The ring is capped at `MAX_ENTRIES`. Once full, older entries are
 * discarded oldest-first. This bounds memory under a `RUST_LOG=trace`
 * storm; the matching session-side cap on the broadcast channel is 1024
 * (in `LogStreamLayer`), so a slow UI causes drops at the channel layer
 * before this buffer overruns. Drops are surfaced as synthetic WARN
 * entries injected by `subscribe_log_events`.
 */
const MAX_ENTRIES = 5000

export type LogFilter = {
  levels: Set<string>
  targets: Set<string>
  search: string
  /** When false, TRACE rows are excluded from the view (default). */
  showTrace: boolean
}

type LogsStore = {
  entries: LogEntry[]
  sessions: LogSessionInfo[]
  /** Session name currently displayed. May be the active session or a past one. */
  currentSession: string | null
  /** Name of the session this process is writing — for live-tail gating. */
  activeSession: string | null
  /** Pause the live-tail subscription. The store keeps existing entries. */
  isLive: boolean
  /** Auto-scroll the list to the newest entry on append. */
  autoScroll: boolean

  filter: LogFilter

  setSessions: (s: LogSessionInfo[]) => void
  setCurrentSession: (name: string | null) => void
  setActiveSession: (name: string | null) => void
  setIsLive: (v: boolean) => void
  setAutoScroll: (v: boolean) => void
  setFilter: (next: Partial<LogFilter>) => void
  toggleLevel: (level: string) => void
  toggleTarget: (target: string) => void

  /** Replace the buffer wholesale — used after `read_log_session`. */
  setEntries: (e: LogEntry[]) => void
  /** Append a batch (rAF-coalesced from `useLogStream`). Drops oldest past cap. */
  appendBatch: (batch: LogEntry[]) => void
  clearEntries: () => void
}

export const useLogsStore = create<LogsStore>((set) => ({
  entries: [],
  sessions: [],
  currentSession: null,
  activeSession: null,
  isLive: true,
  autoScroll: true,

  filter: {
    levels: new Set<string>(['DEBUG', 'INFO', 'WARN', 'ERROR']),
    targets: new Set<string>(),
    search: '',
    showTrace: false,
  },

  setSessions: (sessions) => set({ sessions }),
  setCurrentSession: (currentSession) => set({ currentSession }),
  setActiveSession: (activeSession) => set({ activeSession }),
  setIsLive: (isLive) => set({ isLive }),
  setAutoScroll: (autoScroll) => set({ autoScroll }),

  setFilter: (next) =>
    set((s) => ({
      filter: { ...s.filter, ...next },
    })),
  toggleLevel: (level) =>
    set((s) => {
      const levels = new Set(s.filter.levels)
      if (levels.has(level)) levels.delete(level)
      else levels.add(level)
      return { filter: { ...s.filter, levels } }
    }),
  toggleTarget: (target) =>
    set((s) => {
      const targets = new Set(s.filter.targets)
      if (targets.has(target)) targets.delete(target)
      else targets.add(target)
      return { filter: { ...s.filter, targets } }
    }),

  setEntries: (entries) =>
    set({
      entries: entries.length > MAX_ENTRIES ? entries.slice(-MAX_ENTRIES) : entries,
    }),
  appendBatch: (batch) =>
    set((s) => {
      if (batch.length === 0) return s
      const next = s.entries.concat(batch)
      if (next.length > MAX_ENTRIES) {
        next.splice(0, next.length - MAX_ENTRIES)
      }
      return { entries: next }
    }),
  clearEntries: () => set({ entries: [] }),
}))

export const LOG_BUFFER_CAP = MAX_ENTRIES
