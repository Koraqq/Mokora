// The confirmation before Mokora ends while a game is being played — leaving a
// match drops it. It names the accounts that are in a game. Opened by the power
// button and by the backend's `quit-requested`.

import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/components/ui/sonner'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useAccounts } from '@/hooks/useAccounts'
import { accountLabel } from '@/lib/accounts'
import { invoke, listen } from '@/lib/tauri'
import { useQuitStore } from '@/stores/quitStore'

export function QuitDialog() {
  const { t } = useTranslation()
  const { accounts, refresh } = useAccounts()
  const asking = useQuitStore((s) => s.asking)
  const setAsking = useQuitStore((s) => s.setAsking)

  useEffect(() => {
    let unlisten: (() => void) | undefined
    void listen('quit-requested', () => {
      refresh()
      setAsking(true)
    }).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [refresh, setAsking])

  const names = accounts.filter((a) => a.in_game).map((a) => accountLabel(a, t))

  const quit = async () => {
    setAsking(false)
    try {
      await invoke('quit_app')
    } catch (e) {
      toast.error(String(e))
    }
  }

  return (
    <Dialog open={asking} onOpenChange={setAsking}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('play.quit.confirm_title')}</DialogTitle>
          <DialogDescription>{t('play.quit.confirm_body')}</DialogDescription>
        </DialogHeader>
        {names.length > 0 && (
          <p className="text-sm">{t('play.quit.in_game', { names: names.join('、') })}</p>
        )}
        <DialogFooter className="mx-0 mb-0 border-0 bg-transparent p-0">
          <Button variant="outline" size="sm" onClick={() => setAsking(false)}>
            {t('common.cancel')}
          </Button>
          <Button variant="destructive" size="sm" onClick={() => void quit()}>
            {t('play.quit.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
