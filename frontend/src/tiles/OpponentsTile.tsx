import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { useAnalysisStore } from '@/stores/analysisStore'
import { useGameStore } from '@/stores/gameStore'
import { pct } from '@/lib/format'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import type { OpponentRisk } from '@/types'
import type { Breakpoint } from '@/tiles/defaults'

// Stable empty fallback — selector must not allocate a new array per render.
// Zustand v5 uses Object.is on the selector output and would loop forever.
const NO_OPPONENTS: readonly OpponentRisk[] = []

export function OpponentsTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const opponents = useAnalysisStore((s) => s.result?.opponents ?? NO_OPPONENTS)
  const ourSeat = useGameStore((s) => s.game?.our_seat ?? null)
  const numPlayers = useGameStore((s) => s.game?.num_players ?? 4)

  return (
    <TileFrame id="opponents" title={t('tile.opponents')} bp={bp} contentClassName="p-0">
      {opponents.length === 0 ? (
        <span className="text-muted-foreground text-sm p-3 block">{t('tile.opponents_empty')}</span>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="text-[10px] uppercase">{t('tile.opponents_seat')}</TableHead>
              <TableHead className="text-[10px] uppercase">{t('tile.opponents_tenpai')}</TableHead>
              <TableHead className="text-[10px] uppercase">{t('tile.opponents_riichi')}</TableHead>
              <TableHead className="text-[10px] uppercase text-right">{t('tile.opponents_max_risk')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {opponents.map((o) => {
              const maxRisk = o.risk.length ? Math.max(...o.risk) : 0
              return (
                <TableRow key={o.seat}>
                  <TableCell className="font-mono">{seatLabel(o.seat, ourSeat, numPlayers)}</TableCell>
                  <TableCell className="font-mono">{pct(o.tenpai_rate)}</TableCell>
                  <TableCell>{o.is_riichi ? '●' : '—'}</TableCell>
                  <TableCell className="font-mono text-right">{pct(maxRisk)}</TableCell>
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      )}
    </TileFrame>
  )
}

function seatLabel(seat: number, ourSeat: number | null, numPlayers: number): string {
  if (ourSeat == null) return String(seat)
  const n = Math.max(1, numPlayers)
  const d = (seat - ourSeat + n) % n
  if (n === 3) {
    // 3p: only kamicha (上) and shimocha (下) — no toimen (対).
    return d === 0 ? '自' : d === 1 ? '下' : '上'
  }
  return d === 1 ? '下' : d === 2 ? '対' : d === 3 ? '上' : '自'
}
