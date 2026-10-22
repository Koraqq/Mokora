import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useAnalysisStore } from '@/stores/analysisStore'
import { mjaiToMahgen } from '@/lib/tileIdx'
import { pct } from '@/lib/format'
import type { Breakpoint } from '@/tiles/defaults'

export function RecommendationsTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const result = useAnalysisStore((s) => s.result)
  const top = result?.hand14?.maintain.slice(0, 3) ?? []
  const shanten = result?.shanten

  return (
    <TileFrame
      id="recommendations"
      title={t('tile.recommendations')}
      bp={bp}
      rightSlot={
        shanten != null && (
          <span className="rounded-full border border-border px-2 py-0.5 text-[10px] tracking-wider uppercase">
            {t('mahjong.shanten_value', { n: shanten })}
          </span>
        )
      }
      contentClassName="flex flex-col gap-2"
    >
      {top.length === 0 ? (
        <span className="text-muted-foreground text-sm">{t('tile.recommendations_empty')}</span>
      ) : (
        <ol className="flex flex-col gap-2">
          {top.map((c, i) => (
            <li key={i} className="flex items-center gap-3 rounded-md border border-border bg-muted/20 p-2">
              <span className="text-xs font-mono text-muted-foreground w-4">{i + 1}</span>
              <Mahgen seq={mjaiToMahgen([c.discard])} kind="rec" />
              <div className="flex-1 flex flex-col text-xs">
                <span>EV: <span className="font-mono">{c.result.mixed_round_point.toFixed(0)}</span></span>
                <span className="text-muted-foreground">{t('mahjong.agari')}: {pct(c.result.avg_agari_rate)}</span>
              </div>
              <span className="text-xs font-mono">{c.result.waits_total}枚</span>
            </li>
          ))}
        </ol>
      )}
    </TileFrame>
  )
}
