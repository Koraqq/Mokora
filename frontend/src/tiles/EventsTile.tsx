import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { useNotifyStore } from '@/stores/notifyStore'
import { fmtTime } from '@/lib/format'
import type { Breakpoint } from '@/tiles/defaults'

export function EventsTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const events = useNotifyStore((s) => s.events)
  const recent = events.slice().reverse().slice(0, 50)

  return (
    <TileFrame id="events" title={t('tile.events')} bp={bp} contentClassName="p-0">
      <ul className="flex flex-col text-xs font-mono divide-y divide-border">
        {recent.length === 0 ? (
          <li className="px-3 py-2 text-muted-foreground">{t('tile.events_empty')}</li>
        ) : recent.map((e) => (
          <li key={e._seq} className="flex items-center gap-2 px-3 py-1.5">
            <span className="text-muted-foreground text-[10px]">{fmtTime(new Date(e._ts))}</span>
            <span className="font-medium">{e.type}</span>
            {'actor' in e && typeof e.actor === 'number' && (
              <span className="text-muted-foreground">@{e.actor}</span>
            )}
            {'pai' in e && typeof e.pai === 'string' && (
              <span className="text-emerald-400">{e.pai}</span>
            )}
          </li>
        ))}
      </ul>
    </TileFrame>
  )
}
