import { create } from 'zustand'
import type { InspectorEntry, InspectorKind } from '@/types'

/**
 * Inspector tab store.
 *
 * Bounded ring buffer of timeline rows. Live arrivals (from
 * `useInspectorStream`) and initial-load reads (from `read_inspector`)
 * append into the same buffer — same shape on both paths.
 *
 * Filter is applied client-side over the buffer so toggles are instant.
 * Server-side filter is only used at initial-load time to keep the wire
 * payload small for sessions with hundreds of thousands of events.
 */
const MAX_ENTRIES = 5000

export type InspectorFilter = {
  kinds: Set<InspectorKind>
  /** 0/1/2/3 for individual seats; null = all. */
  actor: number | null
  search: string
}

type InspectorStore = {
  entries: InspectorEntry[]
  filter: InspectorFilter
  /** Index of the row whose detail panel is open. */
  selectedKey: string | null

  setFilter: (next: Partial<InspectorFilter>) => void
  toggleKind: (kind: InspectorKind) => void
  setSelectedKey: (k: string | null) => void

  setEntries: (e: InspectorEntry[]) => void
  appendBatch: (batch: InspectorEntry[]) => void
  clearEntries: () => void
}

export const useInspectorStore = create<InspectorStore>((set) => ({
  entries: [],
  filter: {
    kinds: new Set<InspectorKind>(['ws_frame', 'mjai_event', 'bot_reaction', 'http']),
    actor: null,
    search: '',
  },
  selectedKey: null,

  setFilter: (next) => set((s) => ({ filter: { ...s.filter, ...next } })),
  toggleKind: (kind) =>
    set((s) => {
      const kinds = new Set(s.filter.kinds)
      if (kinds.has(kind)) kinds.delete(kind)
      else kinds.add(kind)
      return { filter: { ...s.filter, kinds } }
    }),
  setSelectedKey: (selectedKey) => set({ selectedKey }),

  setEntries: (entries) =>
    set({
      entries: entries.length > MAX_ENTRIES ? entries.slice(-MAX_ENTRIES) : entries,
      selectedKey: null,
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
  clearEntries: () => set({ entries: [], selectedKey: null }),
}))

export const INSPECTOR_BUFFER_CAP = MAX_ENTRIES
