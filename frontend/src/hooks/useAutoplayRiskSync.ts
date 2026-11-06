import { useEffect } from 'react'

import { forgetAutoplayRisk, syncAutoplayRisk } from '@/lib/autoplayRisk'
import { LICENSE_CHANGED, type LicenseStatus } from '@/lib/license'
import { HAS_TAURI, listen } from '@/lib/tauri'

/** Keep this account window's "autoplay notice seen" in step with the
 *  membership: taken from the other accounts when the window opens or a
 *  membership signs in, and forgotten when it signs out. */
export function useAutoplayRiskSync(): void {
  useEffect(() => {
    if (!HAS_TAURI) return
    void syncAutoplayRisk()
    let unlisten: (() => void) | undefined
    let gone = false
    void listen<LicenseStatus>(LICENSE_CHANGED, (status) => {
      if (status.signed_in) void syncAutoplayRisk()
      else forgetAutoplayRisk()
    }).then((u) => {
      if (gone) u()
      else unlisten = u
    })
    return () => {
      gone = true
      unlisten?.()
    }
  }, [])
}
