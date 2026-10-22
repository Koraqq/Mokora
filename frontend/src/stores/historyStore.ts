// Game-history Zustand store + persisted PT-rule preference.
//
// Shape: a denormalised cache of `GameRecord[]` plus the active filter
// and the user's chosen PT rule. Records are loaded once on bridge
// startup via `invoke('list_game_history')` and updated incrementally
// via the `history-recorded` Tauri event. Filtering happens in-memory
// (the page is filter-driven and the dataset is small enough to scan).
//
// The PT rule and filter live in the same store so a re-render of the
// History page can subscribe to a single hook and recalculate. The
// rule is persisted to localStorage; the filter is session-only.
//
// 3p and 4p keep a rule each (rooms, ranks and PT tables all differ per
// mode): `rule` is always the one for the mode the filter shows.

import { create } from 'zustand'

import type { GameRecord, HistoryFilter } from '@/types'
import { DEFAULT_CUSTOM_RULE, type PtRule } from '@/lib/ptCalc'

/** Before rules were kept per mode: one rule for both. Read once, as the
 *  starting point for each. */
const LEGACY_PT_RULE_KEY = 'mokora.history.ptRule'
const PT_RULES_KEY = 'mokora.history.ptRules'

export { ALL_PLAYERS } from '@/lib/historyPlayers'

const DEFAULT_RULE: PtRule = {
  kind: 'majsoul',
  lobby: 'jade',
  dan: 'jakketsu_3',
}

type PtRules = Record<3 | 4, PtRule>

function validRule(parsed: unknown): PtRule | null {
  const kind = (parsed as PtRule | null)?.kind
  // A rule saved while Tenhou's was offered falls back to the default.
  return kind === 'majsoul' || kind === 'custom'
    ? (parsed as PtRule)
    : null
}

function loadRules(): PtRules {
  if (typeof localStorage === 'undefined') {
    return { 3: DEFAULT_RULE, 4: DEFAULT_RULE }
  }
  try {
    const raw = localStorage.getItem(PT_RULES_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Record<'3' | '4', unknown>>
      return {
        3: validRule(parsed['3']) ?? DEFAULT_RULE,
        4: validRule(parsed['4']) ?? DEFAULT_RULE,
      }
    }
    const legacy = localStorage.getItem(LEGACY_PT_RULE_KEY)
    const rule = (legacy && validRule(JSON.parse(legacy))) || DEFAULT_RULE
    return { 3: rule, 4: rule }
  } catch {
    return { 3: DEFAULT_RULE, 4: DEFAULT_RULE }
  }
}

function saveRules(rules: PtRules) {
  if (typeof localStorage === 'undefined') return
  try {
    localStorage.setItem(PT_RULES_KEY, JSON.stringify(rules))
  } catch {
    /* quota — ignore */
  }
}

/** The mode a filter shows: the page treats anything but 3p as 4p. */
function modeOf(filter: HistoryFilter): 3 | 4 {
  return filter.num_players === 3 ? 3 : 4
}

const initialRules = loadRules()

type HistoryStore = {
  records: GameRecord[]
  /** True while the initial fetch is in flight. */
  loading: boolean
  filter: HistoryFilter
  /** Whose games to show: a key from `lib/historyPlayers`, `ALL_PLAYERS`,
   *  or `null` for the account this Mokora is logged in as (else whoever
   *  played most recently) — the default, so another account's games never
   *  blend into the stats unasked. Session-only, like `filter`: a pick holds
   *  until Mokora closes, and each account's Mokora keeps its own. */
  player: string | null
  /** The rule for each mode, persisted. */
  rules: PtRules
  /** `rules` for the mode the filter shows. */
  rule: PtRule

  setRecords: (records: GameRecord[]) => void
  setLoading: (loading: boolean) => void
  /** Insert at the top (newest-first ordering by start time). */
  prepend: (record: GameRecord) => void
  /** Drop a record by id; no-op if absent. */
  remove: (id: string) => void
  setFilter: (filter: HistoryFilter) => void
  setPlayer: (player: string | null) => void
  /** Set the rule for the mode the filter shows. */
  setRule: (rule: PtRule) => void
  /** Reset to a sensible default custom rule (full editable). */
  resetCustom: () => void
}

export const useHistoryStore = create<HistoryStore>((set) => ({
  records: [],
  loading: false,
  // Default to 4-player only — 3p / 4p are separate analysis modes (uma,
  // starting score, rank distribution all differ) and must never be
  // averaged together. The Game History page exposes a top-level
  // toggle to switch.
  filter: { num_players: 4 },
  player: null,
  rules: initialRules,
  rule: initialRules[4],

  setRecords: (records) => set({ records }),
  setLoading: (loading) => set({ loading }),
  prepend: (record) =>
    set((s) => {
      // Defensive dedup: if backend re-emits, swap in place rather than duplicate.
      const filtered = s.records.filter((r) => r.id !== record.id)
      return { records: [record, ...filtered] }
    }),
  remove: (id) =>
    set((s) => ({ records: s.records.filter((r) => r.id !== id) })),
  setFilter: (filter) => set((s) => ({ filter, rule: s.rules[modeOf(filter)] })),
  setPlayer: (player) => set({ player }),
  setRule: (rule) =>
    set((s) => {
      const rules = { ...s.rules, [modeOf(s.filter)]: rule }
      saveRules(rules)
      return { rules, rule }
    }),
  resetCustom: () =>
    set((s) => {
      const rules = { ...s.rules, [modeOf(s.filter)]: DEFAULT_CUSTOM_RULE }
      saveRules(rules)
      return { rules, rule: DEFAULT_CUSTOM_RULE }
    }),
}))
