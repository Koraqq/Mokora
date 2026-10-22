import { useEffect, useRef } from 'react'
import { Channel } from '@tauri-apps/api/core'
import { invoke, HAS_TAURI } from '@/lib/tauri'
import { useLogsStore } from '@/stores/logsStore'
import type { LogEntry } from '@/types'

/**
 * Live-tail subscription to the active session's tracing stream.
 *
 * Active only when (a) Tauri is present, (b) we're viewing the active
 * session, and (c) the user hasn't paused via the toggle. Arrivals are
 * buffered in a ref and flushed via `requestAnimationFrame` (~60 Hz cap)
 * so React never re-renders at event rate — at `RUST_LOG=trace` under
 * proxy load that can be thousands of events per second, and a naive
 * `setEntries([...prev, ev])` would lock up the page.
 *
 * Lifecycle: one `tauri::ipc::Channel<LogEntry>` per subscribed render.
 * Tauri 2's Channel doesn't currently expose an explicit "stop" — the
 * backend forwarder task lives until the broadcast closes (process
 * shutdown). Re-subscribing on session/pause changes is therefore
 * cheap on the JS side but does spawn a fresh forwarder per call;
 * acceptable since users only flip these toggles by hand.
 */
export function useLogStream(enabled: boolean = true): void {
  const isLive = useLogsStore((s) => s.isLive)
  const currentSession = useLogsStore((s) => s.currentSession)
  const activeSession = useLogsStore((s) => s.activeSession)
  const appendBatch = useLogsStore((s) => s.appendBatch)

  const bufferRef = useRef<LogEntry[]>([])
  const rafRef = useRef<number | null>(null)

  useEffect(() => {
    if (!HAS_TAURI) return
    if (!enabled) return
    if (!isLive) return
    if (!currentSession || !activeSession) return
    if (currentSession !== activeSession) return

    let cancelled = false

    const flush = () => {
      rafRef.current = null
      const buf = bufferRef.current
      if (buf.length === 0) return
      bufferRef.current = []
      appendBatch(buf)
    }

    const channel = new Channel<LogEntry>()
    channel.onmessage = (entry) => {
      if (cancelled) return
      bufferRef.current.push(entry)
      if (rafRef.current == null) {
        rafRef.current = requestAnimationFrame(flush)
      }
    }

    invoke<void>('subscribe_log_events', { onEvent: channel }).catch((err) => {
      // Surface in console — no toast: the user can see "no live entries"
      // in the viewer itself if the subscription fails.
      console.warn('subscribe_log_events failed:', err)
    })

    return () => {
      cancelled = true
      if (rafRef.current != null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
      }
      bufferRef.current = []
    }
  }, [enabled, isLive, currentSession, activeSession, appendBatch])
}
