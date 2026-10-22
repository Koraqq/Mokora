import { describe, expect, it } from 'vitest'
import { Sparkles } from 'lucide-react'

import type { AnnouncementEntry } from './entries'
import { MAX_FRESH_ENTRIES, selectUnseenEntries } from './select'

function entry(id: string, date: string, version?: string): AnnouncementEntry {
  return { id, date, version, features: [{ icon: Sparkles, key: 'x' }] }
}

// Newest first, like the real data. `news` has no version (product news,
// e.g. the MokoraMS announcement); selection is purely by date, so a
// version-less entry is treated like any other.
const ENTRIES: AnnouncementEntry[] = [
  entry('v3_6_0', '2026-09-01', '3.6.0'),
  entry('v3_5_0', '2026-08-12', '3.5.0'),
  entry('news', '2026-08-09'),
  entry('v3_4_0', '2026-07-01', '3.4.0'),
  entry('v3_3_0', '2026-06-01', '3.3.0'),
]

const ids = (xs: AnnouncementEntry[]) => xs.map((e) => e.id)

describe('selectUnseenEntries', () => {
  it('replays every entry the user skipped over', () => {
    expect(ids(selectUnseenEntries(ENTRIES, '2026-07-01'))).toEqual([
      'v3_6_0',
      'v3_5_0',
      'news',
    ])
  })

  it('shows nothing when the baseline is current', () => {
    expect(selectUnseenEntries(ENTRIES, '2026-09-01')).toEqual([])
  })

  it('caps the no-baseline case at the newest few entries', () => {
    const fresh = selectUnseenEntries(ENTRIES, null)
    expect(fresh.length).toBe(MAX_FRESH_ENTRIES)
    expect(ids(fresh)).toEqual(['v3_6_0', 'v3_5_0', 'news'])
  })
})
