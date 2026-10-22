// Most-dealt-in tiles (最常放銃牌) — every ron across the games the History
// page is showing, tsumo excluded, as two cards: the tiles the recorded
// player dealt in (自家) and the tiles opponents dealt in, to anyone (他家).
// Counting needs each game's event log, which only the backend has, so the
// page's filtered ids are sent to `history_deal_in_tiles` once and both cards
// read the answer.
//
// Statistics only: nothing here feeds back into a bot's decision.

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Mahgen } from '@/components/Mahgen'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { HAS_TAURI, invoke } from '@/lib/tauri'
import { mjaiToMahgen } from '@/lib/tileIdx'
import type { DealInStats, DealInTally, GameRecord } from '@/types'

/** Entries shown before "show more". */
const TOP_N = 20

export function DealInTiles({ records }: { records: GameRecord[] }) {
  const [stats, setStats] = useState<DealInStats | null>(null)
  const [error, setError] = useState<string | null>(null)

  // The id set is the whole input; a new filter (player, 3p/4p, dates) or a
  // newly recorded game changes it.
  const key = records.map((r) => r.id).join(',')

  useEffect(() => {
    if (!HAS_TAURI) return
    let cancelled = false
    invoke<DealInStats>('history_deal_in_tiles', { ids: key ? key.split(',') : [] })
      .then((s) => {
        if (cancelled) return
        setStats(s)
        setError(null)
      })
      .catch((e) => {
        if (!cancelled) setError(String(e))
      })
    return () => {
      cancelled = true
    }
  }, [key])

  return (
    <>
      <DealInCard
        titleKey="history.deal_in.mine_title"
        tally={stats?.mine ?? null}
        games={stats?.games ?? 0}
        error={error}
      />
      <DealInCard
        titleKey="history.deal_in.others_title"
        tally={stats?.others ?? null}
        games={stats?.games ?? 0}
        error={error}
      />
    </>
  )
}

function DealInCard({
  titleKey,
  tally,
  games,
  error,
}: {
  titleKey: string
  tally: DealInTally | null
  games: number
  error: string | null
}) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  const rowRef = useRef<HTMLDivElement>(null)

  const shown = tally ? (expanded ? tally.tiles : tally.tiles.slice(0, TOP_N)) : []

  return (
    <Card>
      <CardHeader>
        <div className="flex items-baseline justify-between gap-2">
          <CardTitle className="text-sm uppercase tracking-wider">{t(titleKey)}</CardTitle>
          {tally && tally.deal_ins > 0 && (
            <span className="text-xs text-muted-foreground">
              {t('history.deal_in.summary', { deal_ins: tally.deal_ins, games })}
            </span>
          )}
        </div>
      </CardHeader>
      <CardContent>
        {error ? (
          <div className="text-sm text-red-400">{error}</div>
        ) : !tally || tally.tiles.length === 0 ? (
          <div className="text-sm text-muted-foreground py-6 text-center">
            {t('history.no_data')}
          </div>
        ) : (
          <>
            <div
              ref={rowRef}
              className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-x-4 gap-y-3"
            >
              {shown.map((entry, i) => (
                <div key={entry.tile} className="flex items-center gap-2">
                  <span className="w-6 text-right font-mono text-xs text-muted-foreground tabular-nums">
                    {i + 1}.
                  </span>
                  <div className="flex leading-[0]">
                    <Mahgen
                      seq={mjaiToMahgen([entry.tile])}
                      kind="deal-in"
                      containerRef={rowRef}
                      className="leading-[0]"
                    />
                  </div>
                  <div className="flex flex-col leading-tight">
                    <span className="font-mono text-sm tabular-nums">×{entry.count}</span>
                    <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
                      {((entry.count / tally.deal_ins) * 100).toFixed(1)}%
                    </span>
                  </div>
                </div>
              ))}
            </div>
            {tally.tiles.length > TOP_N && (
              <div className="mt-4 flex justify-center">
                <Button variant="outline" size="sm" onClick={() => setExpanded((v) => !v)}>
                  {expanded
                    ? t('history.deal_in.show_less')
                    : t('history.deal_in.show_more', { count: tally.tiles.length - TOP_N })}
                </Button>
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  )
}
