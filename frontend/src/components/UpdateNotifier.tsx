import { useEffect } from 'react'

import { PERIODIC_CHECK_MS, useUpdaterStore } from '@/stores/updaterStore'
import { HAS_TAURI } from '@/lib/tauri'
import { UpdateDialog } from '@/components/UpdateDialog'

/// Invisible coordinator. Mount it once near the root of the tree.
///
/// Two checks, each switched in Settings → Updates:
/// - on launch: once, ~3 seconds in;
/// - periodic: every five minutes. The Rust side answers nothing while a game
///   is played or anything plays in full auto, and a version cancelled in this
///   run is not asked about again.
/// A newer release opens the prompt. "Don't remind me again" is answered on
/// the Rust side, which then reports nothing until a newer version is
/// installed.
export function UpdateNotifier() {
  const autoCheckEnabled = useUpdaterStore((s) => s.autoCheckEnabled)
  const periodicCheckEnabled = useUpdaterStore((s) => s.periodicCheckEnabled)

  // Deferred so startup stays quick and the rest of the app has mounted.
  useEffect(() => {
    if (!HAS_TAURI || !autoCheckEnabled) return
    const id = window.setTimeout(() => {
      void useUpdaterStore.getState().checkOnLaunch()
    }, 3000)
    return () => window.clearTimeout(id)
  }, [autoCheckEnabled])

  useEffect(() => {
    if (!HAS_TAURI || !periodicCheckEnabled) return
    const id = window.setInterval(() => {
      void useUpdaterStore.getState().checkNow(false, true)
    }, PERIODIC_CHECK_MS)
    return () => window.clearInterval(id)
  }, [periodicCheckEnabled])

  return <UpdateDialog />
}
