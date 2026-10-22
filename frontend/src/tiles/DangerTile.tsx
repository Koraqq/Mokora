import { useMemo, useRef, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import { useNotifyStore } from '@/stores/notifyStore'
import { useBotStore } from '@/stores/botStore'
import { combinedDanger, currentDanger, dangerRows } from '@/lib/danger'
import { nativeBotShortName } from '@/lib/nativeBots'
import { mjaiToMahgen } from '@/lib/tileIdx'
import { riskColor } from '@/lib/riskColor'
import type { Breakpoint } from '@/tiles/defaults'

// The AI risk table — the model's danger read (the built-in bot's danger
// head, sent with each of its replies; this round's only): for *every* tile,
// how likely dealing it is to be won on by
// anyone — 1 − Π(1 − p) over the opponents — coloured on the Risk tile's
// scale, with each opponent's own share on hover. Tiles we hold are marked.
// The rule-based Risk tile stays as it is beside it; the tiles are its size
// (the same Mahgen kind against a row as wide), centred like it, in rows
// from `dangerRows` (sanma: terminals and honours, pinzu, souzu).
export function DangerTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const responses = useNotifyStore((s) => s.responses)
  const events = useNotifyStore((s) => s.events)
  const game = useGameStore((s) => s.game)
  const status = useBotStore((s) => s.status)
  const rowRef = useRef<HTMLDivElement>(null)

  const danger = useMemo(() => currentDanger(responses, events), [responses, events])
  // "AI risk table - Mora": the model on now, by its short name.
  const bot = 'bot' in status ? status.bot : null
  const title = bot
    ? t('tile.model_danger_of', { model: nativeBotShortName(bot) ?? bot })
    : t('tile.model_danger')
  const held = useMemo(() => {
    const seat = game?.our_seat
    const tehai = seat != null ? (game?.players[seat]?.tehai ?? []) : []
    return new Set(tehai.map((x) => x.replace(/r$/, '')))
  }, [game])

  // Opponents in the order the head sends them: shimocha first.
  const seatLabels =
    danger?.opponents.length === 2
      ? [t('tile.danger_shimocha'), t('tile.danger_kamicha')]
      : [t('tile.danger_shimocha'), t('tile.danger_toimen'), t('tile.danger_kamicha')]

  const rows = useMemo(() => {
    if (!danger) return []
    const all = combinedDanger(danger)
    return dangerRows(danger.tiles).map((row) =>
      row.map((i) => ({ tile: danger.tiles[i], i, p: all[i] })),
    )
  }, [danger])

  return (
    <TileFrame id="model-danger" title={title} bp={bp}>
      {!danger ? (
        <div className="flex h-full min-h-40 items-center justify-center px-4 text-center">
          <span className="text-muted-foreground text-sm">{t('tile.model_danger_empty')}</span>
        </div>
      ) : (
        <div ref={rowRef} className="flex flex-col gap-3">
          {rows.map((row, r) => (
            <div key={r} className="flex flex-wrap items-start justify-center gap-x-2 gap-y-3">
              {row.map(({ tile, i, p }) => (
                <DangerCell
                  key={tile}
                  tile={tile}
                  percent={p * 100}
                  held={held.has(tile)}
                  detail={danger.opponents
                    .map((o, k) => `${seatLabels[k] ?? k + 1} ${(o[i] * 100).toFixed(1)}%`)
                    .join(' · ')}
                  rowRef={rowRef}
                />
              ))}
            </div>
          ))}
        </div>
      )}
    </TileFrame>
  )
}

function DangerCell({
  tile,
  percent,
  held,
  detail,
  rowRef,
}: {
  tile: string
  percent: number
  held: boolean
  detail: string
  rowRef: RefObject<HTMLDivElement | null>
}) {
  const { t } = useTranslation()
  const c = riskColor(percent)
  return (
    <div className="flex flex-col items-center gap-1" title={detail}>
      <div
        className="flex rounded-[3px] leading-[0]"
        style={{ boxShadow: `0 0 0 2px ${c.ring}${held ? `, 0 0 8px 2px ${c.glow}` : ''}` }}
      >
        <Mahgen seq={mjaiToMahgen([tile])} kind="hand-risk" containerRef={rowRef} className="leading-[0]" />
      </div>
      <span
        className="min-w-[2.4em] rounded px-1 py-0.5 text-center text-[10px] font-mono tabular-nums leading-none text-white"
        style={{ backgroundColor: c.chip }}
      >
        {percent < 0.05 ? '0' : percent.toFixed(1)}
      </span>
      {/* Tiles we hold: the ones a discard is chosen from. */}
      <span
        className={held ? 'h-1 w-1 rounded-full bg-foreground' : 'h-1 w-1'}
        aria-label={held ? t('tile.model_danger_held') : undefined}
      />
    </div>
  )
}
