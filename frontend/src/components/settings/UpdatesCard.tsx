import { ExternalLink, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Toggle } from '@/components/settings/fields'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { MOKORA_GITHUB_URL, openExternal } from '@/lib/external'
import { useUpdaterStore } from '@/stores/updaterStore'

type UpdatesDraft = { updateOnLaunch: boolean; periodicUpdates: boolean }

/** What the last press of "check for updates" found. */
type Outcome = { kind: 'latest' } | { kind: 'available'; version: string } | { kind: 'failed'; message: string }

export function UpdatesCard<T extends UpdatesDraft>({
  draft,
  setDraft,
}: {
  draft: T
  setDraft: (draft: T) => void
}) {
  const { t, i18n } = useTranslation()
  const lastChecked = useUpdaterStore((s) => s.lastChecked)
  const checking = useUpdaterStore((s) => s.checking)
  const [outcome, setOutcome] = useState<Outcome | null>(null)

  const lastCheckedLabel = lastChecked
    ? t('updates.settings.last_checked', {
        when: new Date(lastChecked).toLocaleString(i18n.language),
      })
    : t('updates.settings.never_checked')

  // A manual check answers whatever the reminder settings say. A newer
  // release also opens the update prompt, as any check that finds one does.
  const checkNow = async () => {
    setOutcome(null)
    try {
      await useUpdaterStore.getState().checkNow(true)
      const found = useUpdaterStore.getState().pendingUpdate
      setOutcome(found ? { kind: 'available', version: found.latest_version } : { kind: 'latest' })
    } catch (e) {
      setOutcome({ kind: 'failed', message: e instanceof Error ? e.message : String(e) })
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('updates.settings.title')}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <Toggle
          label={t('updates.settings.auto_check')}
          hint={t('updates.settings.auto_check_hint')}
          value={draft.updateOnLaunch}
          onChange={(updateOnLaunch) => setDraft({ ...draft, updateOnLaunch })}
        />
        <Toggle
          label={t('updates.settings.periodic_check')}
          hint={t('updates.settings.periodic_check_hint')}
          value={draft.periodicUpdates}
          onChange={(periodicUpdates) => setDraft({ ...draft, periodicUpdates })}
        />
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" size="sm" onClick={checkNow} disabled={checking}>
            <RefreshCw className={checking ? 'size-3.5 animate-spin' : 'size-3.5'} />
            {checking ? t('updates.settings.checking') : t('updates.settings.check_now')}
          </Button>
          <Button variant="outline" size="sm" onClick={() => openExternal(MOKORA_GITHUB_URL)}>
            <ExternalLink className="size-3.5" />
            {t('updates.settings.go_github')}
          </Button>
          {outcome && (
            <span
              role="status"
              className={
                outcome.kind === 'failed'
                  ? 'text-xs text-destructive'
                  : 'text-xs text-muted-foreground'
              }
            >
              {outcome.kind === 'latest' && t('updates.settings.up_to_date')}
              {outcome.kind === 'available' &&
                t('updates.settings.available', { version: outcome.version })}
              {outcome.kind === 'failed' &&
                t('updates.settings.check_failed', { message: outcome.message })}
            </span>
          )}
        </div>
        <span className="text-xs text-muted-foreground">{lastCheckedLabel}</span>
      </CardContent>
    </Card>
  )
}
