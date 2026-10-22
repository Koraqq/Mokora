// Which bot's games a History card shows: every game, or one bot family's —
// a dropdown of the bots installed now (the built-in ones and any added
// under Bots), in the card header beside the day and kind switches.

import { useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { botFamilies, type BotView } from '@/lib/historyBots'
import { invoke } from '@/lib/tauri'
import { useBotStore } from '@/stores/botStore'
import type { BotInfo } from '@/types'

export function BotTabs({
  value,
  onChange,
}: {
  value: BotView
  onChange: (v: BotView) => void
}) {
  const { t } = useTranslation()
  const list = useBotStore((s) => s.list)
  const setList = useBotStore((s) => s.setList)
  // The list is filled by the Bots page and the bot switcher; read it here
  // too when neither has run yet.
  useEffect(() => {
    if (list.length > 0) return
    invoke<BotInfo[]>('list_bots')
      .then(setList)
      .catch(() => {
        /* only "all" then */
      })
  }, [list.length, setList])
  const families = useMemo(() => botFamilies(list), [list])
  // A bot that is no longer installed falls back to every game.
  const offered = value === 'all' || families.some((f) => f.key === value)
  const current = offered ? value : 'all'
  useEffect(() => {
    if (!offered && list.length > 0) onChange('all')
  }, [offered, list.length, onChange])

  return (
    <Select value={current} onValueChange={onChange}>
      <SelectTrigger
        size="sm"
        className="w-32 text-xs"
        aria-label={t('history.bot_view.label')}
        title={t('history.bot_view.label')}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper" align="end">
        <SelectItem value="all" className="text-xs">
          {t('history.bot_view.all')}
        </SelectItem>
        {families.map((f) => (
          <SelectItem key={f.key} value={f.key} className="text-xs">
            {f.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
