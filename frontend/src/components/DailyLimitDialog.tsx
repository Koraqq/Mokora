// The warning that full auto will start no more games today: shown when
// settings are saved (Settings, the play screen's mode switch) in full auto
// with the account's daily limit already played. The settings are kept; the
// backend's next-game flow is what holds back (`history::daily`), and a game
// started by hand is still played.

import { useCallback, useState, type ReactNode } from 'react'
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
import { useAccountId } from '@/hooks/useAccountId'
import { modeOf } from '@/lib/autoplayMode'
import { clampDailyLimit, todaysGames } from '@/lib/dailyStats'
import { useHistoryStore } from '@/stores/historyStore'
import type { AppConfig } from '@/types'

/** `check(config)` after saving `config`; render `dialog`. */
export function useDailyLimitWarning(): {
  check: (config: AppConfig) => void
  dialog: ReactNode
} {
  const { t } = useTranslation()
  const records = useHistoryStore((s) => s.records)
  const accountId = useAccountId()
  const [shown, setShown] = useState<{ played: number; limit: number } | null>(null)

  const check = useCallback(
    (config: AppConfig) => {
      const limit = clampDailyLimit(config.autoplay?.next_game?.max_games_per_day ?? 0)
      if (limit === 0 || modeOf(config) !== 'full') return
      const played = todaysGames(records, accountId, new Date()).length
      if (played >= limit) setShown({ played, limit })
    },
    [records, accountId],
  )

  const dialog = (
    <Dialog open={shown !== null} onOpenChange={(o) => !o && setShown(null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('settings.autoplay.daily_limit_title')}</DialogTitle>
          <DialogDescription>
            {t('settings.autoplay.daily_limit_desc', {
              played: shown?.played ?? 0,
              limit: shown?.limit ?? 0,
            })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="mx-0 mb-0 border-0 bg-transparent p-0">
          <Button size="sm" onClick={() => setShown(null)}>
            {t('settings.autoplay.daily_limit_ok')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )

  return { check, dialog }
}
