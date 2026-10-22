import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useNotifyStore } from '@/stores/notifyStore'

// The next-game flow stopped for want of copper coins: the room's table
// asks more than is held and today's revive coins are claimed already (or
// did not cover it). Said in the middle of the screen — the run will not
// go on until another room is picked — and kept until dismissed or a new
// run starts.
export function NoCoinsDialog() {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const status = useNotifyStore((s) => s.nextGame)
  // The status object itself is the occurrence: a new stop is a new object.
  const [dismissed, setDismissed] = useState<object | null>(null)
  const open = status.state === 'blocked' && dismissed !== status
  if (status.state !== 'blocked') return null
  const room = t(`settings.autoplay.next_game_room_${status.room}`)
  const close = () => setDismissed(status)

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('next_game_blocked.title', { room })}</DialogTitle>
          <DialogDescription>
            {t('next_game_blocked.body', {
              gold: status.gold.toLocaleString(i18n.language),
              need: status.need.toLocaleString(i18n.language),
            })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="bg-transparent p-0 border-0 mx-0 mb-0">
          <Button variant="outline" size="sm" onClick={close}>
            {t('next_game_blocked.ok')}
          </Button>
          <Button
            size="sm"
            onClick={() => {
              close()
              navigate('/settings')
            }}
          >
            {t('next_game_blocked.settings')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
