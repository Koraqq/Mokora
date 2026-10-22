// Filtered + sorted (newest-first) list of game records. Click any row
// to open the detail dialog. Delete button on each row asks for
// confirmation before invoking the backend command.
//
// The header's day picker narrows the table to one day of play (newest day
// first, with that day's game count); its select-all button shows every day
// again.

import { useMemo, useState } from 'react'
import { Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { invoke } from '@/lib/tauri'
import { gameDays, recordsOfDay } from '@/lib/historyDays'
import { roomLabelKey } from '@/lib/matchInfo'
import { computePt, type PtRule } from '@/lib/ptCalc'
import { useHistoryStore } from '@/stores/historyStore'
import type { GameRecord } from '@/types'

import { DayPicker } from './DayPicker'
import { GameDetailDialog } from './GameDetailDialog'

function modeLabel(record: GameRecord, t: (k: string) => string): string {
  const players = record.num_players === 3 ? '3p' : '4p'
  const mode =
    record.kyoku_mode === 'east_only'
      ? t('history.filter.east_only')
      : record.kyoku_mode === 'east_south'
        ? t('history.filter.east_south')
        : t('history.filter.other')
  return `${players} · ${mode}`
}

export function GameList({
  records,
  rule,
}: {
  records: GameRecord[]
  rule: PtRule
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState<GameRecord | null>(null)
  const [pickedDay, setPickedDay] = useState<string | null>(null)
  const remove = useHistoryStore((s) => s.remove)

  const days = useMemo(() => gameDays(records), [records])
  // A day the page filters no longer leave in the list falls back to all.
  const day = days.some((d) => d.day === pickedDay) ? pickedDay : null
  const shown = useMemo(() => recordsOfDay(records, day), [records, day])

  const onDelete = async (id: string) => {
    if (!window.confirm(t('history.delete_confirm'))) return
    try {
      const removed = await invoke<boolean>('delete_game_history_entry', { id })
      if (removed) {
        remove(id)
        toast.success(t('history.deleted'))
      }
    } catch (e) {
      toast.error(String(e))
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-sm uppercase tracking-wider">
            {t('history.game_list')}
          </CardTitle>
          <DayPicker days={days} value={day} onChange={setPickedDay} />
        </div>
      </CardHeader>
      <CardContent className="px-0">
        {shown.length === 0 ? (
          <div className="text-sm text-muted-foreground py-8 text-center">
            {t('history.no_data')}
          </div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('history.table.date')}</TableHead>
                <TableHead>{t('history.table.platform')}</TableHead>
                <TableHead>{t('history.table.room')}</TableHead>
                <TableHead>{t('history.table.mode')}</TableHead>
                <TableHead>{t('history.table.rank')}</TableHead>
                <TableHead className="text-right">
                  {t('history.table.end_score')}
                </TableHead>
                <TableHead className="text-right">{t('history.table.pt')}</TableHead>
                <TableHead className="w-20" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.map((r) => {
                const pt = computePt(r, rule)
                const score =
                  r.our_seat == null ? null : r.final_scores[r.our_seat]
                const room = roomLabelKey(r.match_info)
                return (
                  <TableRow
                    key={r.id}
                    className="cursor-pointer"
                    onClick={() => setOpen(r)}
                  >
                    <TableCell className="font-mono text-xs">
                      {new Date(r.started_at).toLocaleString()}
                    </TableCell>
                    <TableCell>
                      {t(`platform.${r.platform}`)}
                    </TableCell>
                    <TableCell>
                      {room ? t(room.key, room.params) : '—'}
                    </TableCell>
                    <TableCell>{modeLabel(r, t)}</TableCell>
                    <TableCell>{r.our_rank ?? '—'}</TableCell>
                    <TableCell className="text-right font-mono">
                      {score == null ? '—' : score.toLocaleString()}
                    </TableCell>
                    <TableCell
                      className={
                        'text-right font-mono ' +
                        (pt > 0
                          ? 'text-emerald-500'
                          : pt < 0
                            ? 'text-red-500'
                            : '')
                      }
                    >
                      {r.our_rank == null ? '—' : pt.toFixed(1)}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="inline-flex gap-0.5">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation()
                            void onDelete(r.id)
                          }}
                          aria-label={t('history.table.delete')}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </CardContent>
      <GameDetailDialog record={open} onOpenChange={(v) => !v && setOpen(null)} />
    </Card>
  )
}
