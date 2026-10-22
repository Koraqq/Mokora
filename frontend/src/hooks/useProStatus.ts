import { useEffect, useState } from 'react'

import { PRO_STATUS, type ProStatus } from '@/lib/pro'
import { HAS_TAURI, invoke, listen } from '@/lib/tauri'

/** Where the paid module stands: read once, then kept current by the Rust
 *  side's `pro-status`. Null until the first answer. */
export function useProStatus(): ProStatus | null {
  const [status, setStatus] = useState<ProStatus | null>(null)
  useEffect(() => {
    if (!HAS_TAURI) return
    invoke<ProStatus>('pro_status')
      .then(setStatus)
      .catch(() => {})
    let unlisten: (() => void) | undefined
    let gone = false
    listen<ProStatus>(PRO_STATUS, setStatus).then((u) => {
      if (gone) u()
      else unlisten = u
    })
    return () => {
      gone = true
      unlisten?.()
    }
  }, [])
  return status
}
