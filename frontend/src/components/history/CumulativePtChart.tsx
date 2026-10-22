// Cumulative PT line chart. X axis = game ordinal (oldest → newest);
// Y axis = running total PT under the active rule. Hover tooltip shows
// the date + per-game delta.
//
// Each game's dot shows which bot played it by *shape*, not colour, so it
// reads the same under every theme: a triangle for Mokora
// (`mokora-3p`), a diamond for Mora (`mora-3p`), the usual circle
// for the original base bot (which played every game recorded before the bot
// was stored).
//
// Ctrl + wheel zooms the x axis around the game under the cursor, which
// stays where it is (`lib/chartZoom`); zoomed in, dragging pans and the y
// axis fits the games in view. Double-click, or the reset button, shows the
// whole series again, as does any new filter or game.

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { Button } from '@/components/ui/button'
import {
  panWindow,
  visibleRange,
  type XWindow,
  ZOOM_STEP,
  zoomWindow,
} from '@/lib/chartZoom'
import { botOfName } from '@/lib/historyBots'
import type { CumulativePoint } from '@/lib/ptCalc'
import { cn } from '@/lib/utils'

const STROKE = 'var(--color-chart-1)'

/** One game's dot: a circle, a triangle for Mokora, a diamond for Mora, a
 *  square for Yakuman. `active` is the enlarged hover dot. */
function BotDot({
  cx,
  cy,
  payload,
  active = false,
}: {
  cx?: number
  cy?: number
  payload?: { bot: string | null }
  active?: boolean
}) {
  if (cx == null || cy == null) return null
  const r = active ? 5 : 2
  const bot = botOfName(payload?.bot)
  if (bot === 'yakuman') {
    // Square with the same visual weight as the circle.
    const h = r * 1.3
    return <rect x={cx - h} y={cy - h} width={2 * h} height={2 * h} fill={STROKE} stroke={STROKE} />
  }
  if (bot === 'mora') {
    // Diamond with the same visual weight as the circle.
    const h = r * 1.7
    return (
      <polygon
        points={`${cx},${cy - h} ${cx + h},${cy} ${cx},${cy + h} ${cx - h},${cy}`}
        fill={STROKE}
        stroke={STROKE}
      />
    )
  }
  if (bot === 'mokora') {
    // Equilateral-ish triangle with the same visual weight as the circle.
    const h = r * 1.9
    return (
      <polygon
        points={`${cx},${cy - h} ${cx - h * 0.95},${cy + h * 0.6} ${cx + h * 0.95},${cy + h * 0.6}`}
        fill={STROKE}
        stroke={STROKE}
      />
    )
  }
  return <circle cx={cx} cy={cy} r={r} fill={active ? STROKE : 'var(--color-background)'} stroke={STROKE} strokeWidth={2} />
}

/** The plot area inside the chart (the grid), in client pixels. */
function plotRect(el: HTMLElement): DOMRect {
  const grid = el.querySelector('.recharts-cartesian-grid')
  return (grid ?? el).getBoundingClientRect()
}

export function CumulativePtChart({ series }: { series: CumulativePoint[] }) {
  const { t } = useTranslation()
  const n = series.length

  // The window belongs to the series it was set on: a new filter or a newly
  // recorded game shows the whole of the new one.
  const seriesKey = `${n}:${series[0]?.record.id ?? ''}:${series[n - 1]?.record.id ?? ''}`
  const [zoom, setZoom] = useState<{ key: string; w: XWindow | null }>({ key: '', w: null })
  const win = zoom.key === seriesKey ? zoom.w : null
  const [dragging, setDragging] = useState(false)

  // Latest window for the wheel listener, which is attached once.
  const live = useRef({ win, n, seriesKey })
  useEffect(() => {
    live.current = { win, n, seriesKey }
  }, [win, n, seriesKey])

  const plotRef = useRef<HTMLDivElement>(null)
  const hasData = n > 0
  // Native and non-passive, so Ctrl + wheel can stop the page from zooming.
  useEffect(() => {
    const el = plotRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey || e.deltaY === 0) return
      e.preventDefault()
      const { win, n, seriesKey } = live.current
      const rect = plotRect(el)
      const anchor = (e.clientX - rect.left) / rect.width
      const factor = e.deltaY < 0 ? 1 / ZOOM_STEP : ZOOM_STEP
      setZoom({ key: seriesKey, w: zoomWindow(win, n, anchor, factor) })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [hasData])

  // Drag to pan: the window moves by as many games as the pointer crossed.
  const drag = useRef<{ x: number; w: XWindow } | null>(null)
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!win || e.button !== 0) return
    drag.current = { x: e.clientX, w: win }
    e.currentTarget.setPointerCapture(e.pointerId)
    setDragging(true)
  }
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d) return
    const rect = plotRect(e.currentTarget)
    const shift = (-(e.clientX - d.x) / rect.width) * (d.w.hi - d.w.lo)
    setZoom({ key: seriesKey, w: panWindow(d.w, n, shift) })
  }
  const endDrag = () => {
    drag.current = null
    setDragging(false)
  }
  const reset = () => setZoom({ key: seriesKey, w: null })

  if (series.length === 0) {
    return (
      <div className="text-sm text-muted-foreground py-12 text-center">
        {t('history.no_data')}
      </div>
    )
  }

  const all = series.map((p) => ({
    index: p.index + 1,
    cumulative: p.cumulative,
    delta: p.delta,
    started_at: p.record.started_at,
    rank: p.record.our_rank,
    bot: p.record.bot ?? null,
    player: (p.record.our_seat == null ? '' : p.record.names[p.record.our_seat]) ?? '',
  }))
  // The legend names only the bots whose games are on the chart.
  const hasBase = all.some((d) => botOfName(d.bot) === 'base')
  const hasMokora = all.some((d) => botOfName(d.bot) === 'mokora')
  const hasMora = all.some((d) => botOfName(d.bot) === 'mora')
  const hasYakuman = all.some((d) => botOfName(d.bot) === 'yakuman')
  const [start, end] = visibleRange(win, n)
  const data = win ? all.slice(start, end) : all

  return (
    <div className="w-full">
      <div className="flex items-center justify-between gap-4 pb-1 text-xs text-muted-foreground">
        {win ? (
          <span className="flex items-center gap-2">
            {t('history.zoom.range', { from: start + 1, to: end, total: n })}
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-6 px-2 text-xs"
              onClick={reset}
            >
              {t('history.zoom.reset')}
            </Button>
          </span>
        ) : (
          <span />
        )}
        {(hasMokora || hasMora || hasYakuman) && (
          <div className="flex gap-4">
            {hasBase && (
              <span className="flex items-center gap-1">
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                  <circle cx="5" cy="5" r="3" fill="var(--color-background)" stroke={STROKE} strokeWidth="2" />
                </svg>
                {t('history.pt_legend.base')}
              </span>
            )}
            {hasMokora && (
              <span className="flex items-center gap-1">
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                  <polygon points="5,0.5 0.5,9 9.5,9" fill={STROKE} />
                </svg>
                {t('history.pt_legend.mokora')}
              </span>
            )}
            {hasMora && (
              <span className="flex items-center gap-1">
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                  <polygon points="5,0.5 9.5,5 5,9.5 0.5,5" fill={STROKE} />
                </svg>
                {t('history.pt_legend.mora')}
              </span>
            )}
            {hasYakuman && (
              <span className="flex items-center gap-1">
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                  <rect x="1.5" y="1.5" width="7" height="7" fill={STROKE} />
                </svg>
                {t('history.pt_legend.yakuman')}
              </span>
            )}
          </div>
        )}
      </div>
      <div
        ref={plotRef}
        className={cn(
          // No focus ring on the chart when a drag or click lands on it.
          'h-64 w-full select-none [&_*:focus]:outline-none',
          win && (dragging ? 'cursor-grabbing' : 'cursor-grab'),
        )}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={reset}
      >
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 0 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="var(--color-border)" />
            <XAxis dataKey="index" stroke="var(--color-muted-foreground)" fontSize={12} />
            {/* Zoomed in, the y axis fits the games in view rather than 0. */}
            <YAxis
              stroke="var(--color-muted-foreground)"
              fontSize={12}
              domain={win ? ['auto', 'auto'] : undefined}
            />
            <ReferenceLine y={0} stroke="var(--color-muted-foreground)" strokeDasharray="2 2" />
            <Tooltip
              content={({ active, payload }) => {
                const p = active ? payload?.[0] : undefined
                const d = p?.payload as
                  | {
                      started_at: string
                      rank: number | null
                      bot: string | null
                      player: string
                    }
                  | undefined
                if (!p || !d) return null
                const date = new Date(d.started_at).toLocaleString()
                const bot = t(`history.pt_legend.${botOfName(d.bot)}`)
                return (
                  <div
                    className="rounded-lg border px-3 py-2 text-sm shadow-md"
                    style={{
                      background: 'var(--color-popover)',
                      borderColor: 'var(--color-border)',
                      color: 'var(--color-popover-foreground)',
                    }}
                  >
                    <div>{`${date} · ${t('history.table.rank')} ${d.rank ?? '—'} · ${bot}`}</div>
                    {/* Who played the game on the left, the running total at
                        the right, with room between. */}
                    <div className="mt-1 flex items-baseline justify-between gap-8">
                      <span>{d.player}</span>
                      <span style={{ color: STROKE }}>
                        {t('history.cumulative_pt')} : {Number(p.value).toFixed(1)}
                      </span>
                    </div>
                  </div>
                )
              }}
            />
            <Line
              type="monotone"
              dataKey="cumulative"
              stroke={STROKE}
              strokeWidth={2}
              dot={<BotDot />}
              activeDot={<BotDot active />}
              // Redrawing on every wheel notch or drag step would only lag.
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}
