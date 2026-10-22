import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'

const TAURI_GLOBAL = (globalThis as unknown as { __TAURI__?: unknown }).__TAURI__
export const HAS_TAURI = TAURI_GLOBAL !== undefined

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!HAS_TAURI) throw new Error(`tauri not available: ${cmd}`)
  return await tauriInvoke<T>(cmd, args)
}

/** Listen to `name` as this web view receives it: what the backend sends to
 *  this view (`emit_to` its label) and what it sends to everyone (`emit`).
 *  Not `@tauri-apps/api/event`'s `listen`, which by default takes every
 *  target's copy: the backend sends an account's events to its interface
 *  and, for the shown account, a second copy to the overlay window — the
 *  interface would get each one twice. */
export async function listen<T>(
  name: string,
  cb: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!HAS_TAURI) return () => {}
  return await getCurrentWebview().listen<T>(name, (e) => cb(e.payload))
}
