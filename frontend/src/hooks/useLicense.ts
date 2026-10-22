import { useEffect, useState } from 'react'

import { LICENSE_CHANGED, type LicenseStatus } from '@/lib/license'
import { HAS_TAURI, invoke, listen } from '@/lib/tauri'

/** The app's license status: read once, then kept current by the Rust side's
 *  `license-changed`. Null until the first answer. The setter is for answers
 *  of the license commands, which carry the new status. */
export function useLicense(): [LicenseStatus | null, (s: LicenseStatus) => void] {
  const [status, setStatus] = useState<LicenseStatus | null>(null)
  useEffect(() => {
    if (!HAS_TAURI) return
    invoke<LicenseStatus>('license_status')
      .then(setStatus)
      .catch(() => {})
    let unlisten: (() => void) | undefined
    let gone = false
    listen<LicenseStatus>(LICENSE_CHANGED, setStatus).then((u) => {
      if (gone) u()
      else unlisten = u
    })
    return () => {
      gone = true
      unlisten?.()
    }
  }, [])
  return [status, setStatus]
}
