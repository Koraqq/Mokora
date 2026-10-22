// Detail-stats table — mirrors the layout of `libriichi/src/stat.rs`'s
// Display impl, but reads from the in-memory aggregate over the
// frontend-filtered record set.
//
// Four rows come from the games' event logs instead (`against`, see
// `useAgainstStats`): how often the player's discards were called on (pon,
// chi — a dash in sanma, which has none — and open kan), side by side on one
// row, and how many yakuman they paid.
//
// Every figure is one cell of the same three-column grid, the game and round
// counts and the average placement included; the placements themselves are
// in the chart above.

import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type AggregateStats,
  avgRank,
  formatDuration,
  rate,
} from '@/lib/historyStats'
import type { AgainstStats } from '@/types'

export function StatsTable({
  stats,
  against = null,
  numPlayers = 4,
  action,
}: {
  stats: AggregateStats
  /** Calls on the player's discards and yakuman paid; null while unknown. */
  against?: AgainstStats | null
  numPlayers?: 3 | 4
  /** Header control on the right, e.g. the per-bot view switch. */
  action?: ReactNode
}) {
  const { t } = useTranslation()

  const fmtPct = (v: number | null) => (v == null ? '—' : `${(v * 100).toFixed(2)}%`)
  const fmtNum = (v: number | null) => (v == null ? '—' : v.toFixed(1))
  const fmtTime = (ms: number | null) => (ms == null ? '—' : formatDuration(ms))

  const winRate = rate(stats.agari, stats.rounds)
  const houjuuRate = rate(stats.houjuu, stats.rounds)
  const callRate = rate(stats.fuuro, stats.rounds)
  const riichiRate = rate(stats.riichi, stats.rounds)
  const ryukyokuRate = rate(stats.ryukyoku, stats.rounds)

  const avgWinning = rate(
    stats.agari_point_oya + stats.agari_point_ko,
    stats.agari,
  )
  const avgRiichiWinning = rate(stats.riichi_agari_point, stats.riichi_agari)
  const avgOpenWinning = rate(stats.fuuro_agari_point, stats.fuuro_agari)
  const avgDamaWinning = rate(stats.dama_agari_point, stats.dama_agari)
  const avgWinningTurn = rate(stats.agari_jun, stats.agari)
  const avgDealIn = rate(
    stats.houjuu_point_to_oya + stats.houjuu_point_to_ko,
    stats.houjuu,
  )

  const agariAfterRiichi = rate(stats.riichi_agari, stats.riichi)
  const houjuuAfterRiichi = rate(stats.riichi_houjuu, stats.riichi)
  const agariAfterCall = rate(stats.fuuro_agari, stats.fuuro)
  const houjuuAfterCall = rate(stats.fuuro_houjuu, stats.fuuro)

  // In threes: each line of the grid is one row here.
  const rows: Array<[string, string]> = [
    [t('history.stat.games'), `${stats.games}`],
    [t('history.stat.rounds'), `${stats.rounds}`],
    [t('history.stat.avg_rank'), fmtNum(avgRank(stats))],
    [t('history.stat.win_rate'), fmtPct(winRate)],
    [t('history.stat.deal_in_rate'), fmtPct(houjuuRate)],
    [t('history.stat.call_rate'), fmtPct(callRate)],
    [t('history.stat.riichi_rate'), fmtPct(riichiRate)],
    [t('history.stat.ryukyoku_rate'), fmtPct(ryukyokuRate)],
    [t('history.stat.avg_winning'), fmtNum(avgWinning)],
    [t('history.stat.avg_riichi_winning'), fmtNum(avgRiichiWinning)],
    [t('history.stat.avg_open_winning'), fmtNum(avgOpenWinning)],
    [t('history.stat.avg_dama_winning'), fmtNum(avgDamaWinning)],
    [t('history.stat.avg_winning_turn'), fmtNum(avgWinningTurn)],
    [t('history.stat.avg_deal_in'), fmtNum(avgDealIn)],
    [t('history.stat.agari_after_riichi'), fmtPct(agariAfterRiichi)],
    [t('history.stat.houjuu_after_riichi'), fmtPct(houjuuAfterRiichi)],
    [t('history.stat.agari_after_call'), fmtPct(agariAfterCall)],
    [t('history.stat.houjuu_after_call'), fmtPct(houjuuAfterCall)],
    // From the logs when they have been read: the recorded count missed
    // sanma tsumo yakuman in games saved before it was fixed.
    [t('history.stat.yakuman'), `${against ? against.won_yakuman : stats.yakuman}`],
    [t('history.stat.nagashi_mangan'), `${stats.nagashi_mangan}`],
    [t('history.stat.yakuman_against'), against ? `${against.yakuman}` : '—'],
    [t('history.stat.called_pon'), against ? `${against.pon}` : '—'],
    [t('history.stat.called_chi'), against && numPlayers === 4 ? `${against.chi}` : '—'],
    [t('history.stat.called_kan'), against ? `${against.kan}` : '—'],
    [
      t('history.stat.total_time'),
      fmtTime(stats.timed_games > 0 ? stats.duration_total_ms : null),
    ],
    [
      t('history.stat.avg_time'),
      fmtTime(rate(stats.duration_total_ms, stats.timed_games)),
    ],
    [t('history.stat.min_time'), fmtTime(stats.duration_min_ms)],
    [t('history.stat.max_time'), fmtTime(stats.duration_max_ms)],
    [
      t('history.stat.avg_round_time'),
      fmtTime(rate(stats.round_total_ms, stats.round_count)),
    ],
    [t('history.stat.min_round_time'), fmtTime(stats.round_min_ms)],
    [t('history.stat.max_round_time'), fmtTime(stats.round_max_ms)],
  ]

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-sm uppercase tracking-wider">
            {t('history.stats')}
          </CardTitle>
          {action}
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-x-6 gap-y-1 text-sm">
          {rows.map(([label, value]) => (
            <div
              key={label}
              className="flex items-baseline justify-between py-1 border-b border-border/40"
            >
              <span className="text-muted-foreground">{label}</span>
              <span className="font-mono">{value}</span>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  )
}
