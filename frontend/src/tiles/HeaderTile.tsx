import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import { kyokuLabel } from '@/lib/format'
import type { Breakpoint } from '@/tiles/defaults'

export function HeaderTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)
  const view = useGameStore((s) => s.view)

  return (
    <TileFrame id="header" title={t('tile.header')} bp={bp} contentClassName="flex items-center gap-6 px-4">
      <div className="flex items-center gap-2">
        <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/15 text-emerald-400 px-2 py-0.5 text-[10px] font-semibold tracking-wider uppercase">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
          {t('common.live')}
        </span>
        <h2 className="text-lg font-semibold">
          {game ? kyokuLabel(game.bakaze, game.kyoku) : '—'}
        </h2>
      </div>

      {view?.dora_indicators && (
        <div className="flex items-center gap-2 rounded-md border border-border px-2 py-1">
          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">{t('tile.dora')}</span>
          <Mahgen seq={view.dora_indicators} kind="dora" />
        </div>
      )}

      {game && (
        <div className="flex items-center gap-3 rounded-md border border-border px-2 py-1">
          {/* Icons sized to match the Dora pill (mahgen `dora` kind = 30px). */}
          <div className="flex items-center gap-1.5">
            <img src="/1000_mini.svg" alt="kyotaku" className="h-[30px]" />
            <span className="font-mono text-base font-medium">×{game.kyotaku}</span>
          </div>
          <span className="text-muted-foreground">|</span>
          <div className="flex items-center gap-1.5">
            <img src="/100_mini.svg" alt="honba" className="h-[30px]" />
            <span className="font-mono text-base font-medium">×{game.honba}</span>
          </div>
        </div>
      )}

      {game && (
        <Stat label={t('tile.phase')} value={game.phase} mono />
      )}
    </TileFrame>
  )
}

function Stat({ label, value, mono }: { label: string; value: number | string; mono?: boolean }) {
  return (
    <div className="flex flex-col">
      <span className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</span>
      <span className={mono ? 'font-mono text-sm' : 'font-medium text-sm'}>{value}</span>
    </div>
  )
}
