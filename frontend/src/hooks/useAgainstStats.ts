import { useEffect, useState } from 'react'
import { HAS_TAURI, invoke } from '@/lib/tauri'
import type { AgainstStats, GameRecord } from '@/types'

/**
 * What opponents did to the recorded player over `records` — calls on their
 * discards and yakuman they paid for — or `null` while unknown.
 *
 * Counting needs each game's event log, which only the backend has, so the
 * ids are sent to `history_against_stats`, like the deal-in tiles. The
 * answer is tagged with the id set it was asked for and filtered on read,
 * so a stale one for the previous filter reads as `null`.
 */
export function useAgainstStats(records: GameRecord[]): AgainstStats | null {
  const [answer, setAnswer] = useState<{ key: string; stats: AgainstStats } | null>(null)

  // The id set is the whole input; a new filter or a newly recorded game
  // changes it.
  const key = records.map((r) => r.id).join(',')

  useEffect(() => {
    if (!HAS_TAURI) return
    let cancelled = false
    invoke<AgainstStats>('history_against_stats', { ids: key ? key.split(',') : [] })
      .then((stats) => {
        if (!cancelled) setAnswer({ key, stats })
      })
      .catch(() => {
        /* the rows show "—" */
      })
    return () => {
      cancelled = true
    }
  }, [key])

  return answer?.key === key ? answer.stats : null
}
