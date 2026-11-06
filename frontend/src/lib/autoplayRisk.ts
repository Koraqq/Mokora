// Whether the player has seen the autoplay risk notice.
//
// Each account window keeps the answer in its own storage (`KEY`), which is
// what every check reads. With a membership signed in, the answer is also kept
// by the Rust side, once for every account: a new account window takes it from
// there, so the notice is asked once per membership, paid or free. Signing out
// of the membership forgets it. Without a membership each window asks for
// itself, as before.

import { HAS_TAURI, invoke } from '@/lib/tauri'

const KEY = 'mokora.autoplayRiskAck'

export function autoplayRiskAcknowledged(): boolean {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
}

function remember(on: boolean): void {
  try {
    if (on) localStorage.setItem(KEY, '1')
    else localStorage.removeItem(KEY)
  } catch {
    /* asked again next time */
  }
}

/** The player accepted the notice. */
export function acknowledgeAutoplayRisk(): void {
  remember(true)
  if (HAS_TAURI) void invoke('accept_autoplay_notice').catch(() => {})
}

/** Signed out of the membership: the notice is asked again. */
export function forgetAutoplayRisk(): void {
  remember(false)
}

type Notice = 'signed_out' | 'acked' | 'open'

/** Bring this window's answer in line with the membership's: one account's
 *  acceptance covers the rest, and an answer given here before the membership
 *  kept one (an older version's) is handed up to it. */
export async function syncAutoplayRisk(): Promise<void> {
  if (!HAS_TAURI) return
  try {
    const notice = await invoke<Notice>('autoplay_notice')
    if (notice === 'acked') remember(true)
    else if (notice === 'open' && autoplayRiskAcknowledged()) {
      await invoke('accept_autoplay_notice')
    }
  } catch {
    /* keep what this window has */
  }
}
