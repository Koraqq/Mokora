// The power button beside the avatar: closes Mokora — every account's game and
// capture stop, then the window goes. With a game in progress on any account it
// asks first (the shared `QuitDialog`), since leaving a match drops it.

import { useTranslation } from 'react-i18next'
import { Power } from 'lucide-react'
import { toast } from 'sonner'

import { useAccounts } from '@/hooks/useAccounts'
import { invoke } from '@/lib/tauri'
import { useQuitStore } from '@/stores/quitStore'

export function PowerButton() {
  const { t } = useTranslation()
  const { accounts } = useAccounts()
  const setAsking = useQuitStore((s) => s.setAsking)

  const quit = async () => {
    try {
      await invoke('quit_app')
    } catch (e) {
      toast.error(String(e))
    }
  }

  const inGame = accounts.some((a) => a.in_game)

  return (
    <button
      type="button"
      title={t('play.quit.title')}
      aria-label={t('play.quit.title')}
      onClick={() => (inGame ? setAsking(true) : void quit())}
      className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-all duration-200 hover:bg-red-500/15 hover:text-red-400 active:scale-90"
    >
      <Power className="h-4 w-4" />
    </button>
  )
}
