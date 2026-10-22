// One reviewed game: how far the play was from the model, per kind of action,
// then every decision with what was played next to the model's top three.
//
// "Low model support" is only a filter on the model's probability for what was
// played — a play the model rarely makes, not a mistake and not an expected-
// value loss. The threshold is the player's to move; nothing saved depends on
// it.

import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
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
import { invoke } from '@/lib/tauri'
import { nativeBotShortName } from '@/lib/nativeBots'
import {
  LOW_SUPPORT_DEFAULT,
  REVIEW_KINDS,
  actionTiles,
  kindRows,
  lowSupport,
  pct,
  rate,
} from '@/lib/review'
import type { ReviewAction, ReviewDecision, ReviewReport } from '@/types'

const WIND: Record<string, string> = { E: '東', S: '南', W: '西', N: '北' }

export function ReviewDetail({
  target,
  onClose,
}: {
  target: { id: string; model: string } | null
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [report, setReport] = useState<ReviewReport | null>(null)
  const [failed, setFailed] = useState(false)
  const [percent, setPercent] = useState(String(LOW_SUPPORT_DEFAULT * 100))
  const [kind, setKind] = useState('all')
  const [onlyLow, setOnlyLow] = useState(false)

  useEffect(() => {
    if (!target) return
    let cancelled = false
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setReport(null)
    setFailed(false)
    invoke<ReviewReport | null>('review_get', { id: target.id, model: target.model })
      .then((r) => {
        if (cancelled) return
        if (r) setReport(r)
        else setFailed(true)
      })
      .catch(() => !cancelled && setFailed(true))
    return () => {
      cancelled = true
    }
  }, [target])

  // The threshold as a fraction; a field being typed in falls back to the default.
  const threshold = useMemo(() => {
    const n = Number(percent)
    return Number.isFinite(n) && n >= 0 && n <= 100 ? n / 100 : LOW_SUPPORT_DEFAULT
  }, [percent])

  const rows = useMemo(
    () => (report ? kindRows(report.decisions, threshold) : []),
    [report, threshold],
  )
  const listed = useMemo(
    () =>
      (report?.decisions ?? []).filter(
        (d) =>
          (kind === 'all' || d.actual.kind === kind) &&
          (!onlyLow || lowSupport(d, threshold)),
      ),
    [report, kind, onlyLow, threshold],
  )

  const name = (m: string | undefined) => (m ? (nativeBotShortName(m) ?? m) : '—')
  const total = rows.reduce(
    (acc, r) => ({ scored: acc.scored + r.scored, matched: acc.matched + r.matched, low: acc.low + r.low }),
    { scored: 0, matched: 0, low: 0 },
  )

  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-5xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t('review.detail_title')}</DialogTitle>
          <DialogDescription>{t('review.disclaimer')}</DialogDescription>
        </DialogHeader>
        {failed ? (
          <div className="text-sm text-muted-foreground py-6 text-center">
            {t('review.detail_missing')}
          </div>
        ) : !report ? (
          <div className="text-sm text-muted-foreground py-6 text-center">{t('common.loading')}</div>
        ) : (
          <div className="grid gap-4">
            <div className="grid grid-cols-2 gap-x-6 gap-y-1 text-sm">
              <div>
                <span className="text-muted-foreground">{t('review.model_live')}：</span>
                {name(report.live_bot)}
              </div>
              <div>
                <span className="text-muted-foreground">{t('review.model_label')}：</span>
                {name(report.model.name)}{' '}
                <span className="font-mono text-xs text-muted-foreground">
                  {t('review.weights', { hash: report.model.weights_hash.slice(0, 8) })}
                </span>
              </div>
              <div>
                <span className="text-muted-foreground">{t('review.settings_label')}：</span>
                {t('review.settings_value', {
                  length: t(`review.length_${report.settings.game_length}`),
                  k: report.settings.top_k,
                })}
              </div>
              <div>
                <span className="text-muted-foreground">{t('review.agreement_label')}：</span>
                {rate(total.matched, total.scored)} ({total.matched}/{total.scored})
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-3 text-sm">
              <label className="flex items-center gap-2">
                {t('review.threshold_label')}
                <Input
                  className="w-20"
                  inputMode="decimal"
                  value={percent}
                  onChange={(e) => setPercent(e.target.value)}
                  aria-label={t('review.threshold_label')}
                />
                %
              </label>
              <span className="text-xs text-muted-foreground">{t('review.threshold_hint')}</span>
            </div>

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('review.col_kind')}</TableHead>
                  <TableHead className="text-right">{t('review.col_scored')}</TableHead>
                  <TableHead className="text-right">{t('review.col_matched')}</TableHead>
                  <TableHead className="text-right">{t('review.col_rate')}</TableHead>
                  <TableHead className="text-right">{t('review.col_mean_prob')}</TableHead>
                  <TableHead className="text-right">{t('review.col_low')}</TableHead>
                  <TableHead className="text-right">{t('review.col_forced')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r) => (
                  <TableRow key={r.kind}>
                    <TableCell>{t(`review.kind.${r.kind}`)}</TableCell>
                    <TableCell className="text-right font-mono tabular-nums">{r.scored}</TableCell>
                    <TableCell className="text-right font-mono tabular-nums">{r.matched}</TableCell>
                    <TableCell className="text-right font-mono tabular-nums">
                      {rate(r.matched, r.scored)}
                    </TableCell>
                    <TableCell className="text-right font-mono tabular-nums">
                      {r.scored ? pct(r.meanProb) : '—'}
                    </TableCell>
                    <TableCell className="text-right font-mono tabular-nums">{r.low}</TableCell>
                    <TableCell className="text-right font-mono tabular-nums text-muted-foreground">
                      {r.forced}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <p className="text-xs text-muted-foreground">{t('review.stats_hint')}</p>

            <div className="flex flex-wrap items-center gap-4">
              <Select value={kind} onValueChange={setKind}>
                <SelectTrigger className="w-44" aria-label={t('review.col_kind')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t('review.kind_all')}</SelectItem>
                  {REVIEW_KINDS.map((k) => (
                    <SelectItem key={k} value={k}>
                      {t(`review.kind.${k}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <label className="flex items-center gap-2 text-sm">
                <Switch checked={onlyLow} onCheckedChange={setOnlyLow} />
                {t('review.only_low')}
              </label>
              <span className="text-xs text-muted-foreground">
                {t('review.listed', { n: listed.length, total: report.decisions.length })}
              </span>
            </div>

            <div className="grid gap-1">
              {listed.map((d) => (
                <DecisionRow key={d.idx} d={d} low={lowSupport(d, threshold)} />
              ))}
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

function actionLabel(a: ReviewAction, t: (k: string) => string): string {
  const tiles = actionTiles(a)
  return tiles ? `${t(`review.kind.${a.kind}`)} ${tiles}` : t(`review.kind.${a.kind}`)
}

function DecisionRow({ d, low }: { d: ReviewDecision; low: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="rounded-md border border-border/60 px-3 py-2 text-sm grid gap-1">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="text-muted-foreground">
          {t('review.round', {
            wind: WIND[d.bakaze] ?? d.bakaze,
            kyoku: d.kyoku,
            honba: d.honba,
            turn: d.turn,
          })}
        </span>
        <span className="text-xs text-muted-foreground">
          {d.window ? t('review.window') : t('review.own_turn')}
        </span>
        {low && (
          <Badge variant="outline" className="border-amber-500/60 text-amber-500">
            {t('review.tag_low')}
          </Badge>
        )}
        {d.forced && <Badge variant="secondary">{t('review.tag_forced')}</Badge>}
        {d.actual_prob == null && <Badge variant="secondary">{t('review.tag_not_ranked')}</Badge>}
        {d.missed_win && (
          <Badge variant="outline" className="border-red-500/60 text-red-400">
            {t('review.tag_missed_win')}
          </Badge>
        )}
      </div>
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
        <span>
          <span className="text-muted-foreground">{t('review.actual')}：</span>
          <span className="font-medium">{actionLabel(d.actual, t)}</span>{' '}
          <span className="font-mono text-xs text-muted-foreground">{pct(d.actual_prob)}</span>
        </span>
        <span className="text-muted-foreground">
          {t('review.ai')}：
          {d.top.map((o, i) => (
            <span key={i} className="mr-3">
              <span className={i === 0 ? 'text-foreground' : undefined}>{actionLabel(o.action, t)}</span>{' '}
              <span className="font-mono text-xs">{pct(o.prob)}</span>
            </span>
          ))}
        </span>
      </div>
    </div>
  )
}
