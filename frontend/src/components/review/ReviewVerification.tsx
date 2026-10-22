// The review's own checks, run on the games on this machine: nothing hidden
// reaches the model, two runs agree, every pass is found, and the model call's
// timing alone and beside a running review (the cost to a live game). The
// button is the player's; the last result is kept and shown here.

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, Loader2, XCircle } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { toast } from '@/components/ui/sonner'
import { invoke } from '@/lib/tauri'
import { REVIEW_DEV_RESULTS as DEV } from '@/lib/reviewDevResults'
import type { LatencyStats, SelfCheckReport } from '@/types'

function Check({ ok, children }: { ok: boolean; children: React.ReactNode }) {
  return (
    <li className="flex items-start gap-2">
      {ok ? (
        <CheckCircle2 className="h-4 w-4 mt-0.5 shrink-0 text-emerald-500" />
      ) : (
        <XCircle className="h-4 w-4 mt-0.5 shrink-0 text-red-400" />
      )}
      <span>{children}</span>
    </li>
  )
}

const ms = (x: number) => `${x.toFixed(1)} ms`

export function ReviewVerification({ busy }: { busy: boolean }) {
  const { t } = useTranslation()
  const [report, setReport] = useState<SelfCheckReport | null>(null)
  const [running, setRunning] = useState(false)

  useEffect(() => {
    invoke<SelfCheckReport | null>('review_selfcheck_last')
      .then(setReport)
      .catch(() => {})
  }, [])

  const run = async () => {
    setRunning(true)
    try {
      setReport(await invoke<SelfCheckReport>('review_selfcheck'))
    } catch (e) {
      toast.error(t('review.verify_failed', { error: String(e) }))
    } finally {
      setRunning(false)
    }
  }

  const slower = (a: LatencyStats, b: LatencyStats) =>
    a.p50_ms > 0 ? `${((b.p50_ms / a.p50_ms - 1) * 100).toFixed(0)}%` : '—'

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">{t('review.verify_title')}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <p className="text-sm text-muted-foreground">{t('review.verify_desc')}</p>
        <div className="flex items-center gap-3">
          <Button variant="outline" disabled={busy || running} onClick={() => void run()}>
            {running && <Loader2 className="h-4 w-4 animate-spin" />}
            {running ? t('review.verify_running') : t('review.verify_run')}
          </Button>
          {report && (
            <span className="text-xs text-muted-foreground">
              {t('review.verify_at', { at: new Date(report.at_ms).toLocaleString() })}
            </span>
          )}
        </div>
        {report && report.games === 0 && (
          <div className="text-sm text-muted-foreground">{t('review.verify_no_games')}</div>
        )}
        {report && report.games > 0 && (
          <>
            <ul className="grid gap-2 text-sm">
              <Check ok={report.logs_clean}>
                {t('review.verify_logs', { games: report.games, changed: report.logs_changed })}
              </Check>
              <Check ok={report.leak_mismatches === 0}>
                {t('review.verify_leak', {
                  games: report.leak_games,
                  decisions: report.leak_decisions,
                  bad: report.leak_mismatches,
                  would: report.leak_uncensored_mismatches,
                })}
              </Check>
              <Check ok={report.deterministic}>
                {t('review.verify_deterministic', { games: report.determinism_games })}
              </Check>
              <Check ok={report.unresolved === 0 && report.calls === report.calls_in_logs}>
                {t('review.verify_passes', {
                  windows: report.call_windows,
                  passes: report.passes,
                  calls: report.calls,
                  inLogs: report.calls_in_logs,
                  unresolved: report.unresolved,
                  notRanked: report.not_ranked,
                })}
              </Check>
            </ul>
            <div className="text-sm grid gap-1">
              <div className="font-medium">{t('review.verify_latency')}</div>
              <div className="grid grid-cols-[auto_1fr_1fr_1fr_1fr] gap-x-6 gap-y-1 font-mono text-xs tabular-nums">
                <span />
                <span className="text-muted-foreground">p50</span>
                <span className="text-muted-foreground">p95</span>
                <span className="text-muted-foreground">max</span>
                <span className="text-muted-foreground">n</span>
                <span className="font-sans text-sm">{t('review.verify_alone')}</span>
                <span>{ms(report.latency_alone.p50_ms)}</span>
                <span>{ms(report.latency_alone.p95_ms)}</span>
                <span>{ms(report.latency_alone.max_ms)}</span>
                <span>{report.latency_alone.n}</span>
                <span className="font-sans text-sm">{t('review.verify_with_review')}</span>
                <span>{ms(report.latency_with_review.p50_ms)}</span>
                <span>{ms(report.latency_with_review.p95_ms)}</span>
                <span>{ms(report.latency_with_review.max_ms)}</span>
                <span>{report.latency_with_review.n}</span>
              </div>
              <div className="text-xs text-muted-foreground">
                {t('review.verify_latency_note', {
                  change: slower(report.latency_alone, report.latency_with_review),
                  events: report.concurrent_events,
                })}
              </div>
            </div>
          </>
        )}
        <details className="text-sm">
          <summary className="cursor-pointer text-muted-foreground">
            {t('review.dev_title')} · {DEV.date}
          </summary>
          <div className="mt-2 grid gap-2 text-xs text-muted-foreground">
            <p>
              {t('review.dev_tests', {
                backend: DEV.tests.backend,
                nativeBot: DEV.tests.nativeBot,
                frontend: DEV.tests.frontend,
              })}
            </p>
            <ul className="list-disc pl-5">
              {DEV.tests.review.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
            <p>
              {t('review.dev_real', {
                games: DEV.realGames.games,
                decisions: DEV.realGames.decisions,
                windows: DEV.realGames.callWindows,
                passes: DEV.realGames.passes,
                calls: DEV.realGames.calls,
                inLogs: DEV.realGames.callsInLogs,
                leakGames: DEV.realGames.leakGames,
                leakDecisions: DEV.realGames.leakDecisions,
                leakBad: DEV.realGames.leakMismatches,
                queueGames: DEV.realGames.queueGames,
                queueSeconds: DEV.realGames.queueSeconds,
              })}
            </p>
            <p>{t('review.dev_latency', DEV.latency)}</p>
          </div>
        </details>
      </CardContent>
    </Card>
  )
}
