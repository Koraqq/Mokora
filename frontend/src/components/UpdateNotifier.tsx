import { useEffect, useRef } from 'react'

import { PERIODIC_CHECK_MS, useUpdaterStore } from '@/stores/updaterStore'
import { HAS_TAURI } from '@/lib/tauri'
import { UpdateDialog } from '@/components/UpdateDialog'
import { useAccounts } from '@/hooks/useAccounts'

const LAUNCH_CHECK_DELAY_MS = 3000
const schedulerStartedAt = Date.now()

/// Invisible coordinator. Mount it once near the root of the tree.
///
/// Two checks, each switched in Settings → Updates:
/// - on launch: once, ~3 seconds in, after account state is available;
/// - periodic: three minutes after the preceding completed check;
/// - game-aware: no automatic request is made while any account is in a game,
///   and the transition from the last live game to the lobby checks at once.
/// The Rust side repeats the in-game guard to close the race between the UI
/// seeing the lobby and the request reaching the backend. A version cancelled
/// in this run is not asked about again.
/// A newer release opens the prompt. "Don't remind me again" is answered on
/// the Rust side, which then reports nothing until a newer version is
/// installed.
export function UpdateNotifier() {
  const autoCheckEnabled = useUpdaterStore((s) => s.autoCheckEnabled)
  const periodicCheckEnabled = useUpdaterStore((s) => s.periodicCheckEnabled)
  const lastChecked = useUpdaterStore((s) => s.lastChecked)
  const launchChecked = useUpdaterStore((s) => s.launchChecked)
  const checking = useUpdaterStore((s) => s.checking)
  const { accounts } = useAccounts()
  const inGame = accounts.some((account) => account.in_game)
  const wasInGame = useRef(inGame)

  // Deferred so startup stays quick and the rest of the app has mounted.
  useEffect(() => {
    if (!HAS_TAURI || !autoCheckEnabled || launchChecked || inGame) return
    const id = window.setTimeout(() => {
      void useUpdaterStore.getState().checkOnLaunch()
    }, LAUNCH_CHECK_DELAY_MS)
    return () => window.clearTimeout(id)
  }, [autoCheckEnabled, inGame, launchChecked])

  useEffect(() => {
    if (!HAS_TAURI || !periodicCheckEnabled || inGame || checking) return
    const dueAt = (lastChecked ?? schedulerStartedAt) + PERIODIC_CHECK_MS
    const id = window.setTimeout(() => {
      void useUpdaterStore.getState().checkNow(false, true)
    }, Math.max(0, dueAt - Date.now()))
    return () => window.clearTimeout(id)
  }, [checking, inGame, lastChecked, periodicCheckEnabled])

  useEffect(() => {
    const gameJustEnded = wasInGame.current && !inGame
    wasInGame.current = inGame
    if (!HAS_TAURI || !gameJustEnded || checking) return

    const updater = useUpdaterStore.getState()
    if (autoCheckEnabled && !updater.launchChecked) void updater.checkOnLaunch()
    else if (periodicCheckEnabled) void updater.checkNow(false, true)
  }, [autoCheckEnabled, checking, inGame, periodicCheckEnabled])

  return <UpdateDialog />
}
