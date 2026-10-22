import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { useNotifyStore } from '@/stores/notifyStore'
import { fmtTime } from '@/lib/format'
import type { Breakpoint } from '@/tiles/defaults'

export function BotResponsesTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const responses = useNotifyStore((s) => s.responses)
  const recent = responses.slice().reverse().slice(0, 30)

  return (
    <TileFrame id="bot-responses" title={t('tile.bot_responses')} bp={bp} contentClassName="p-0">
      <ul className="flex flex-col text-xs font-mono divide-y divide-border">
        {recent.length === 0 ? (
          <li className="px-3 py-2 text-muted-foreground">{t('tile.bot_responses_empty')}</li>
        ) : recent.map((r) => (
          <li key={r._seq} className="flex flex-col gap-0.5 px-3 py-1.5">
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground text-[10px]">{fmtTime(new Date(r._ts))}</span>
              <span className="font-medium">{r.type}</span>
              {'pai' in r && typeof r.pai === 'string' && (
                <span className="text-emerald-400">{r.pai}</span>
              )}
            </div>
            {r.meta && Object.keys(r.meta).length > 0 && (
              <span className="text-muted-foreground text-[10px] truncate">
                {Object.keys(r.meta).slice(0, 4).join(', ')}
              </span>
            )}
          </li>
        ))}
      </ul>
    </TileFrame>
  )
}
