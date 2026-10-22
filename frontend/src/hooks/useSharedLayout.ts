import { useEffect } from 'react'
import { HAS_TAURI, invoke } from '@/lib/tauri'
import { useLayoutStore } from '@/stores/layoutStore'

/** The stamp of the last shared layout this account took (or shared). */
const APPLIED_KEY = 'mokora.dashboard.shared_stamp'

/** How often the game page looks for a layout another account shared. */
const POLL_MS = 5000

type SharedLayout = { stamp_ms: number; layout: unknown }

function appliedStamp(): number {
  try {
    return Number(localStorage.getItem(APPLIED_KEY) ?? 0) || 0
  } catch {
    return 0
  }
}

function markApplied(stamp: number) {
  try {
    localStorage.setItem(APPLIED_KEY, String(stamp))
  } catch {
    /* quota exceeded — it is applied again next time, harmlessly */
  }
}

/**
 * Share this account's game page layout (both modes) with every account.
 * Its own stamp is marked taken so the page does not apply it to itself.
 */
export async function shareLayout(): Promise<void> {
  const stamp = await invoke<number>('share_layout', {
    layout: useLayoutStore.getState().exportShared(),
  })
  markApplied(stamp)
}

/**
 * While the game page is open: take a layout another account shared, once —
 * on opening, when the window comes to the front, and every few seconds.
 * Each account keeps its own layout otherwise (its own webview storage).
 */
export function useSharedLayoutSync() {
  useEffect(() => {
    if (!HAS_TAURI) return
    let cancelled = false
    const check = () => {
      invoke<SharedLayout | null>('get_shared_layout')
        .then((shared) => {
          if (cancelled || !shared || shared.stamp_ms <= appliedStamp()) return
          if (useLayoutStore.getState().applyShared(shared.layout)) markApplied(shared.stamp_ms)
        })
        .catch(() => {
          /* tried again on the next tick */
        })
    }
    check()
    const timer = window.setInterval(check, POLL_MS)
    window.addEventListener('focus', check)
    return () => {
      cancelled = true
      window.clearInterval(timer)
      window.removeEventListener('focus', check)
    }
  }, [])
}
