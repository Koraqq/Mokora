// The paid module's download, as one toast at the bottom right: the progress
// bar while it downloads, then "installed" (or "used after a restart" when a
// newer one replaces the one in use), or the failure with a retry. Nothing
// shows for a module that was already there when Mokora started.
//
// Mount it once near the root of the tree.

import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { useProStatus } from '@/hooks/useProStatus'
import { percent, size } from '@/lib/pro'
import { invoke } from '@/lib/tauri'

const ID = 'pro-module'

export function ProModuleNotifier() {
  const { t } = useTranslation()
  const status = useProStatus()
  // "Installed" is only news after a download this run.
  const downloaded = useRef(false)

  useEffect(() => {
    if (!status) return
    switch (status.state) {
      case 'downloading': {
        downloaded.current = true
        const p = percent(status.received, status.total)
        toast.loading(t('pro.downloading', { percent: p }), {
          id: ID,
          duration: Infinity,
          description: (
            <div className="grid gap-1.5 pt-1">
              <div className="h-1.5 w-full overflow-hidden rounded bg-muted">
                <div className="h-full bg-primary transition-[width] duration-150" style={{ width: `${p}%` }} />
              </div>
              <span className="tabular-nums">
                {size(status.received)} / {size(status.total)}
              </span>
            </div>
          ),
        })
        break
      }
      case 'ready':
        if (downloaded.current) {
          toast.success(t('pro.ready'), { id: ID, duration: 5000, description: t('pro.ready_hint') })
        }
        break
      case 'restart':
        toast.info(t('pro.restart'), { id: ID, duration: Infinity })
        break
      case 'failed':
        toast.error(t('pro.failed'), {
          id: ID,
          duration: Infinity,
          description: t(`pro.error.${status.error}`, { defaultValue: t('pro.error.unknown') }),
          action: { label: t('pro.retry'), onClick: () => void invoke('pro_retry') },
        })
        break
      case 'none':
        toast.dismiss(ID)
        break
    }
  }, [status, t])

  return null
}
