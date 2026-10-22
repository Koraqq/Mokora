import type { BotStatus, CaptureStatus } from '@/types'

/**
 * Visual tone for a status-bar indicator LED.
 * - `live`    → green  (subsystem healthy / running)
 * - `pending` → amber  (starting up, not yet live)
 * - `off`     → gray   (cleanly stopped / never started)
 * - `fault`   → red    (errored)
 */
export type IndicatorTone = 'live' | 'pending' | 'off' | 'fault'

/** Tailwind dot background per tone. */
export const TONE_DOT: Record<IndicatorTone, string> = {
  live: 'bg-emerald-400',
  pending: 'bg-amber-400',
  off: 'bg-zinc-400',
  fault: 'bg-red-400',
}

/**
 * Capture-pipeline tone — "is Mokora reading the game?". Driven by the capture
 * supervisor (`src/ipc/capture_supervisor.rs`). `starting` is reserved but not
 * emitted today; both backends start synchronously and jump straight to
 * `running`.
 */
export function captureTone(state: CaptureStatus['state']): IndicatorTone {
  switch (state) {
    case 'running':
      return 'live'
    case 'starting':
      return 'pending'
    case 'error':
      return 'fault'
    case 'stopped':
      return 'off'
    default: {
      // Exhaustiveness guard: a new `CaptureStatus` state that isn't mapped
      // fails `tsc -b` (which CI runs) — the regression net for "a capture
      // state that no longer drives the LED".
      const unreachable: never = state
      return unreachable
    }
  }
}

/**
 * Bot tone — "is the recommendation engine up?". A separate signal from
 * capture: the bot can error (broken env, failed spawn) while capture is
 * happily running, so it gets its own LED.
 */
export function botTone(state: BotStatus['state']): IndicatorTone {
  switch (state) {
    case 'ready':
      return 'live'
    case 'loading':
      return 'pending'
    case 'error':
      return 'fault'
    case 'idle':
    case 'stopped':
      return 'off'
    default: {
      const unreachable: never = state
      return unreachable
    }
  }
}

/** i18n key for the capture LED's label, keyed by tone. */
export const CAPTURE_LABEL: Record<IndicatorTone, string> = {
  live: 'status.connected',
  pending: 'status.connecting',
  off: 'status.disconnected',
  fault: 'status.capture_error',
}

/** i18n key for the bot LED's label, keyed by the raw bot state (idle and
 * stopped share the `off` tone but read differently). */
export const BOT_LABEL: Record<BotStatus['state'], string> = {
  idle: 'status.bot_idle',
  loading: 'status.bot_loading',
  ready: 'status.bot_ready',
  error: 'status.bot_error',
  stopped: 'status.bot_stopped',
}
