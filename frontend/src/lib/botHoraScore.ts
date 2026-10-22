import { useEffect, useState } from 'react'
import { invoke, HAS_TAURI } from '@/lib/tauri'
import type { HoraScoreInfo } from '@/types'

type Cached = { seq: number; info: HoraScoreInfo | null }

const EMPTY: Cached = { seq: -1, info: null }

/**
 * Score a hora bot decision via the `compute_bot_hora_score` Tauri command.
 * Re-runs whenever `seq` changes (the tagged-response sequence number from
 * notifyStore) so each new hora response triggers a fresh lookup.
 *
 * Returns `null` while the request is in flight, on error, when the
 * backend reports "not a winning hand", or when the cached result belongs
 * to a different `seq`. Errors are swallowed silently because the calling
 * tile renders fine without a score line.
 */
export function useBotHoraScore(
  active: boolean,
  actor: number,
  isTsumo: boolean,
  seq: number,
): HoraScoreInfo | null {
  const [cached, setCached] = useState<Cached>(EMPTY)

  useEffect(() => {
    if (!active || !HAS_TAURI) return
    let cancelled = false
    invoke<HoraScoreInfo | null>('compute_bot_hora_score', {
      actor,
      isTsumo,
    })
      .then((res) => {
        if (!cancelled) setCached({ seq, info: res ?? null })
      })
      .catch(() => {
        if (!cancelled) setCached({ seq, info: null })
      })
    return () => {
      cancelled = true
    }
  }, [active, actor, isTsumo, seq])

  return active && cached.seq === seq ? cached.info : null
}
