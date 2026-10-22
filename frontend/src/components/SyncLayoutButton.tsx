// "Sync layout" on the game page: puts this account's layout — tiles,
// positions and sizes, the 3-player and 4-player ones both — on every
// account. The others take it the next time their game page looks (within a
// few seconds, or when their window comes up; `useSharedLayoutSync`).

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Users } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { toast } from '@/components/ui/sonner'
import { shareLayout } from '@/hooks/useSharedLayout'

export function SyncLayoutButton() {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)

  const sync = async () => {
    if (!window.confirm(t('game.sync_layout_confirm'))) return
    setBusy(true)
    try {
      await shareLayout()
      toast.success(t('game.sync_layout_done'))
    } catch (e) {
      toast.error(t('game.sync_layout_failed', { error: String(e) }))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      className="gap-1.5 text-xs"
      disabled={busy}
      onClick={() => void sync()}
      title={t('game.sync_layout_hint')}
    >
      <Users className="size-4" />
      {t('game.sync_layout')}
    </Button>
  )
}
