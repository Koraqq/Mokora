import { useMemo, useRef, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import { useAnalysisStore } from '@/stores/analysisStore'
import { useNotifyStore } from '@/stores/notifyStore'
import { currentDanger, dangerPercentByTile } from '@/lib/danger'
import { tileIdx, mjaiToMahgen, sortMjaiTiles } from '@/lib/tileIdx'
import type { Breakpoint } from '@/tiles/defaults'
import { riskColor } from '@/lib/riskColor'

// Deal-in risk, but only for the tiles in our own hand — laid out like the Self
// Hand panel (suit order) with each tile annotated by its mixed-risk value. The
// full 34-tile chart was noise; what matters when choosing a discard is how
// dangerous each tile *you actually hold* is.
//
// When the bot carries a danger head (Mora), its read of this round is the
// value — learned from every hand of the Phoenix logs, it ranks deal-ins
// better than the rule-based mix — and the header says so; the AI risk table
// tile shows the same read for every tile. Without one (another bot, or
// nothing read yet this round), the rule-based mixed risk as before.
export function RiskChartTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)
  const risk = useAnalysisStore((s) => s.result?.mixed_risk ?? null)
  const responses = useNotifyStore((s) => s.responses)
  const events = useNotifyStore((s) => s.events)
  const model = useMemo(() => {
    const d = currentDanger(responses, events)
    return d ? dangerPercentByTile(d) : null
  }, [responses, events])
  // Mahgen sizes off this ref (the content row), so every tile shares one height
  // and the strip wraps when the panel is narrow.
  const rowRef = useRef<HTMLDivElement>(null)

  const ourSeat = game?.our_seat ?? null
  const sorted = useMemo(() => {
    const tehai = ourSeat != null ? game?.players[ourSeat]?.tehai ?? [] : []
    return sortMjaiTiles(tehai)
  }, [game, ourSeat])

  return (
    <TileFrame
      id="risk-chart"
      title={t('tile.risk_chart')}
      bp={bp}
      rightSlot={
        <span
          className="text-[10px] text-muted-foreground"
          title={t(model ? 'tile.risk_source_model_hint' : 'tile.risk_source_rules_hint')}
        >
          {t(model ? 'tile.risk_source_model' : 'tile.risk_source_rules')}
        </span>
      }
    >
      {sorted.length === 0 ? (
        <div className="flex h-full items-center justify-center">
          <span className="text-muted-foreground text-sm">{t('tile.risk_chart_empty')}</span>
        </div>
      ) : (
        <div
          ref={rowRef}
          className="flex flex-wrap content-start items-start justify-center gap-x-2 gap-y-3"
        >
          {sorted.map((tile, i) => {
            const idx = tileIdx(tile)
            const v =
              model?.get(tile.replace(/r$/, '')) ?? (risk && idx >= 0 ? risk[idx] : null)
            return <RiskTile key={i} tile={tile} value={v} rowRef={rowRef} />
          })}
        </div>
      )}
    </TileFrame>
  )
}

function RiskTile({
  tile,
  value,
  rowRef,
}: {
  tile: string
  value: number | null
  rowRef: RefObject<HTMLDivElement | null>
}) {
  const c = riskColor(value)
  return (
    <div className="flex flex-col items-center gap-1">
      {/* flex + leading-0 so the wrapper shrink-wraps the <mah-gen> tile exactly
          (the host is display:inline, which otherwise reserves line-box space the
          ring would expose as a gap above the tile). */}
      <div
        className="flex rounded-[3px] leading-[0]"
        style={{ boxShadow: `0 0 0 2px ${c.ring}, 0 0 8px 2px ${c.glow}` }}
      >
        <Mahgen seq={mjaiToMahgen([tile])} kind="hand-risk" containerRef={rowRef} className="leading-[0]" />
      </div>
      <span
        className="min-w-[2.4em] rounded px-1 py-0.5 text-center text-[10px] font-mono tabular-nums leading-none text-white"
        style={{ backgroundColor: c.chip }}
      >
        {value == null ? '—' : value.toFixed(1)}
      </span>
    </div>
  )
}
