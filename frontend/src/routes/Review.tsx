// 對局覆盤 (Beta). Replays the recorded games through a built-in model, from
// the player's seat, and lists what was played next to what the model ranked.
// Everything runs on this machine and is read-only: it changes no model and
// nothing a live move reads (see `src/review/`). The page only starts the
// background queue, shows its progress, and reads the saved reviews.
//
// A game already reviewed under the same model and weights is never reviewed
// again; pressing the button only does what is left. The queue waits while a
// live game is on, unless told not to.
//
// Starting a review is a paid feature: without it the controls give way to a
// note on how to unlock it, and the self-check is left out (the Rust side
// refuses both the same way). Reviews done before stay readable.

import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Loader2, Lock, SearchCheck, Square } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { toast } from '@/components/ui/sonner'
import { ReviewDetail } from '@/components/review/ReviewDetail'
import { ReviewVerification } from '@/components/review/ReviewVerification'
import { useLicense } from '@/hooks/useLicense'
import { useProStatus } from '@/hooks/useProStatus'
import { paidAccess, type PaidAccess } from '@/lib/license'
import { moduleReady } from '@/lib/pro'
import { invoke, listen } from '@/lib/tauri'
import { rate } from '@/lib/review'
import { nativeBotShortName } from '@/lib/nativeBots'
import { useHistoryStore } from '@/stores/historyStore'
import type { GameRecord, ReviewModelInfo, ReviewProgress, ReviewRow } from '@/types'

/** Games shown before "show more". */
const PAGE = 100

const IDLE: ReviewProgress = { state: 'idle', total: 0, done: 0 }

export function Review() {
  const [license] = useLicense()
  // Null until the license answers: neither the controls nor the lock show
  // meanwhile, so neither flashes up.
  const pro = useProStatus()
  // The review runs in the paid module: paid, and the module here.
  return <ReviewPage access={paidAccess(license, pro === null ? null : moduleReady(pro))} />
}

function LockedNote({ downloading }: { downloading: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-start gap-3 rounded-md border border-border bg-muted/30 p-3">
      <Lock className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
      <div className="grid gap-0.5">
        <p className="text-sm font-medium">{t('license.pro_title')}</p>
        <p className="text-sm text-muted-foreground">
          {downloading ? t('pro.not_ready') : t('license.pro_review')}
        </p>
      </div>
    </div>
  )
}

function ReviewPage({ access }: { access: PaidAccess }) {
  const { t } = useTranslation()
  const records = useHistoryStore((s) => s.records)
  const setRecords = useHistoryStore((s) => s.setRecords)

  const [models, setModels] = useState<ReviewModelInfo[]>([])
  // `auto` is each game's own default: the strongest built-in model of its
  // player count.
  const [model, setModel] = useState<string>('auto')
  const [rows, setRows] = useState<Record<string, ReviewRow>>({})
  const [progress, setProgress] = useState<ReviewProgress>(IDLE)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [pauseInGame, setPauseInGame] = useState(true)
  const [shown, setShown] = useState(PAGE)
  const [open, setOpen] = useState<{ id: string; model: string } | null>(null)

  // The history is shared by every account's Mokora, so read it again when the
  // page opens or the window comes back, as the History page does.
  useEffect(() => {
    const reload = () => {
      invoke<GameRecord[]>('list_game_history', { filter: null, limit: 1_000_000, offset: 0 })
        .then(setRecords)
        .catch(() => {
          /* keep what is loaded */
        })
    }
    reload()
    window.addEventListener('focus', reload)
    return () => window.removeEventListener('focus', reload)
  }, [setRecords])

  useEffect(() => {
    invoke<ReviewModelInfo[]>('review_models')
      .then(setModels)
      .catch(() => {})
    invoke<ReviewProgress>('review_progress')
      .then(setProgress)
      .catch(() => {})
  }, [])

  const loadRows = useCallback(() => {
    invoke<ReviewRow[]>('review_list', { model: model === 'auto' ? null : model })
      .then((list) => setRows(Object.fromEntries(list.map((r) => [r.id, r]))))
      .catch(() => {})
  }, [model])

  useEffect(() => {
    loadRows()
  }, [loadRows, records.length])

  // Progress, and the rows again after each game the queue finishes.
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<ReviewProgress>('review-progress', (p) => {
      setProgress(p)
      loadRows()
    }).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [loadRows])

  const mine = useMemo(
    () =>
      records
        .filter((r) => r.our_seat != null)
        .sort((a, b) => (a.started_at < b.started_at ? 1 : a.started_at > b.started_at ? -1 : 0)),
    [records],
  )
  const todo = mine.filter((r) => rows[r.id] && !rows[r.id].reviewed)
  const busy = progress.state === 'running' || progress.state === 'paused'

  const start = async (ids: string[]) => {
    try {
      const queued = await invoke<number>('review_start', {
        ids,
        model: model === 'auto' ? null : model,
        pauseInGame,
      })
      setSelected(new Set())
      toast.info(queued === 0 ? t('review.nothing_to_do') : t('review.queued', { n: queued }))
    } catch (e) {
      toast.error(t('review.start_failed', { error: String(e) }))
    }
  }

  const toggle = (id: string) =>
    setSelected((cur) => {
      const next = new Set(cur)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  return (
    <div className="p-6 flex flex-col gap-6 w-full">
      <header>
        <h1 className="text-2xl font-semibold flex items-center gap-2">
          {t('review.title')}
          <Badge
            variant="outline"
            className="border-amber-500/60 text-amber-500"
            title={t('review.beta_hint')}
          >
            {t('review.beta')}
          </Badge>
        </h1>
        <p className="text-sm text-muted-foreground">{t('review.description')}</p>
      </header>

      <Card>
        <CardContent className="py-4 grid gap-4">
          <p className="text-sm text-muted-foreground">{t('review.disclaimer')}</p>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <div className="flex items-center gap-2">
              <span className="text-sm">{t('review.model_label')}</span>
              <Select value={model} onValueChange={setModel} disabled={busy}>
                <SelectTrigger className="w-64" aria-label={t('review.model_label')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">{t('review.model_auto')}</SelectItem>
                  {models.map((m) => (
                    <SelectItem key={m.name} value={m.name}>
                      {m.name} ({m.num_players}P)
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            {access === 'open' && (
              <label className="flex items-center gap-2 text-sm">
                <Switch checked={pauseInGame} onCheckedChange={setPauseInGame} disabled={busy} />
                {t('review.pause_in_game')}
              </label>
            )}
          </div>
          {(access === 'locked' || access === 'downloading') && (
            <LockedNote downloading={access === 'downloading'} />
          )}
          {access === 'open' && (
            <div className="flex flex-wrap items-center gap-3">
              <Button
                disabled={busy || selected.size === 0}
                onClick={() => void start([...selected])}
              >
                <SearchCheck className="h-4 w-4" />
                {t('review.start_selected', { n: selected.size })}
              </Button>
              <Button
                variant="outline"
                disabled={busy || todo.length === 0}
                onClick={() => void start(todo.map((r) => r.id))}
              >
                {t('review.start_all', { n: todo.length })}
              </Button>
              {busy && (
                <Button variant="outline" onClick={() => void invoke('review_cancel')}>
                  <Square className="h-4 w-4" />
                  {t('review.cancel')}
                </Button>
              )}
            </div>
          )}
          <ProgressLine progress={progress} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm uppercase tracking-wider">{t('review.games')}</CardTitle>
        </CardHeader>
        <CardContent>
          {mine.length === 0 ? (
            <div className="text-sm text-muted-foreground py-6 text-center">
              {t('history.no_data')}
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8" />
                  <TableHead>{t('review.col_time')}</TableHead>
                  <TableHead>{t('review.col_mode')}</TableHead>
                  <TableHead>{t('review.col_rank')}</TableHead>
                  <TableHead>{t('review.col_live')}</TableHead>
                  <TableHead>{t('review.col_review')}</TableHead>
                  <TableHead>{t('review.col_status')}</TableHead>
                  <TableHead className="text-right">{t('review.col_agreement')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {mine.slice(0, shown).map((r) => {
                  const row = rows[r.id]
                  const sum = row?.summary
                  return (
                    <TableRow
                      key={r.id}
                      className={row?.reviewed ? 'cursor-pointer' : undefined}
                      onClick={() => row?.reviewed && setOpen({ id: r.id, model: row.model })}
                    >
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          className="accent-primary"
                          aria-label={t('review.select')}
                          disabled={access !== 'open' || busy || !row || row.reviewed}
                          checked={selected.has(r.id)}
                          onChange={() => toggle(r.id)}
                        />
                      </TableCell>
                      <TableCell className="whitespace-nowrap">
                        {new Date(r.started_at).toLocaleString()}
                      </TableCell>
                      <TableCell>{r.num_players === 3 ? t('review.mode_3') : t('review.mode_4')}</TableCell>
                      <TableCell>{r.our_rank ?? '—'}</TableCell>
                      <TableCell>{r.bot ? (nativeBotShortName(r.bot) ?? r.bot) : '—'}</TableCell>
                      <TableCell>{row ? (nativeBotShortName(row.model) ?? row.model) : '—'}</TableCell>
                      <TableCell>
                        {!row ? (
                          '…'
                        ) : progress.current === r.id && busy ? (
                          <span className="inline-flex items-center gap-1 text-muted-foreground">
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                            {t('review.status_running')}
                          </span>
                        ) : row.reviewed ? (
                          <Badge variant="secondary" className="text-emerald-500">
                            {t('review.status_done')}
                          </Badge>
                        ) : row.stale ? (
                          <Badge variant="outline" title={t('review.status_stale_hint')}>
                            {t('review.status_stale')}
                          </Badge>
                        ) : (
                          <span className="text-muted-foreground">{t('review.status_none')}</span>
                        )}
                      </TableCell>
                      <TableCell className="text-right font-mono tabular-nums">
                        {sum ? rate(sum.matched, sum.scored) : '—'}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          )}
          {mine.length > shown && (
            <div className="mt-4 flex justify-center">
              <Button variant="outline" size="sm" onClick={() => setShown((n) => n + PAGE)}>
                {t('review.show_more', { count: mine.length - shown })}
              </Button>
            </div>
          )}
        </CardContent>
      </Card>

      {access === 'open' && <ReviewVerification busy={busy} />}

      <ReviewDetail target={open} onClose={() => setOpen(null)} />
    </div>
  )
}

function ProgressLine({ progress }: { progress: ReviewProgress }) {
  const { t } = useTranslation()
  if (progress.state === 'idle') return null
  const running = progress.state === 'running' || progress.state === 'paused'
  const text =
    progress.state === 'paused'
      ? t('review.progress_paused', { done: progress.done, total: progress.total })
      : running
        ? t('review.progress_running', { done: progress.done, total: progress.total })
        : progress.state === 'cancelled'
          ? t('review.progress_cancelled', { done: progress.done, total: progress.total })
          : t('review.progress_done', { total: progress.total })
  return (
    <div className="grid gap-1.5">
      <div className="h-1.5 w-full rounded bg-muted overflow-hidden">
        <div
          className="h-full bg-primary transition-all"
          style={{ width: `${progress.total ? (100 * progress.done) / progress.total : 0}%` }}
        />
      </div>
      <div className="text-xs text-muted-foreground">
        {text}
        {progress.message ? ` · ${progress.message}` : ''}
      </div>
    </div>
  )
}
