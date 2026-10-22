// The status bar, all on the left: this account's login and whether its game
// is connected ("● K_o 已連線"), the bot each player count uses ("3P bot:
// Mora  4P bot: Mokora"), and the bot only when it has failed — while it
// works, the names say what is running.

import { useTranslation } from 'react-i18next'
import { useAccounts } from '@/hooks/useAccounts'
import { useConfigStore } from '@/stores/configStore'
import { useCaptureStore } from '@/stores/captureStore'
import { useBotStore } from '@/stores/botStore'
import {
  BOT_LABEL,
  botTone,
  CAPTURE_LABEL,
  captureTone,
  TONE_DOT,
  type IndicatorTone,
} from '@/lib/statusIndicators'
import { nativeBotShortName } from '@/lib/nativeBots'

function IndicatorDot({
  tone,
  label,
  title,
}: {
  tone: IndicatorTone
  label: string
  title?: string
}) {
  return (
    <span className="flex items-center gap-1.5" title={title}>
      <span className={`h-1.5 w-1.5 rounded-full ${TONE_DOT[tone]}`} />
      {label}
    </span>
  )
}

export function Statusbar() {
  const { t } = useTranslation()
  const config = useConfigStore((s) => s.config)
  const capture = useCaptureStore((s) => s.status)
  const bot = useBotStore((s) => s.status)
  const { accounts } = useAccounts()
  const nickname = accounts.find((a) => a.current)?.identity?.nickname

  // Capture LED — "is Mokora reading the game?" (proxy / capture transport).
  // Tooltip surfaces the capture descriptor, or the error message on failure,
  // so the user can see *why* it's down.
  const capTone = captureTone(capture.state)
  const captureTitle =
    capture.state === 'error'
      ? capture.error
      : capture.state === 'running' || capture.state === 'starting'
        ? capture.descriptor
        : undefined

  // The bot set for each player count, by its short name (another bot by its
  // own); "off" with the bot switched off, a dash with none set.
  const botName = (name: string | undefined) => {
    if (config && !config.bot.enabled) return t('status.bot_off')
    if (!name) return '—'
    return nativeBotShortName(name) ?? name
  }

  return (
    <footer className="flex items-center gap-6 border-t border-border px-4 py-1.5 text-xs text-muted-foreground bg-muted/30">
      <IndicatorDot
        tone={capTone}
        label={nickname ? `${nickname} ${t(CAPTURE_LABEL[capTone])}` : t(CAPTURE_LABEL[capTone])}
        title={captureTitle}
      />
      <span>{t('status.bot_3p', { name: botName(config?.bot.active_3p) })}</span>
      <span>{t('status.bot_4p', { name: botName(config?.bot.active_4p) })}</span>
      {/* The bot's own light only when it has failed (a broken env, a spawn
          that did not start): otherwise the names above are enough. */}
      {bot.state === 'error' && (
        <IndicatorDot tone={botTone(bot.state)} label={t(BOT_LABEL[bot.state])} title={bot.error} />
      )}
    </footer>
  )
}
