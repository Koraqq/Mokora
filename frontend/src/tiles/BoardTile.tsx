import { useEffect, useRef, useState, type CSSProperties, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import { fmtScore, bakazeFor, kyokuLabel, relativeKind, type RelativeKind } from '@/lib/format'
import type { Breakpoint } from '@/tiles/defaults'
import './BoardTile.css'

// Screen edge each seat is drawn on, and the rotation applied to its tile
// layer (paint-only — see BoardTile.css for why sizing survives rotation).
type Edge = 'bottom' | 'right' | 'top' | 'left'
const EDGE_ROT: Record<Edge, number> = { bottom: 0, right: 270, top: 180, left: 90 }
const KIND_EDGE: Record<RelativeKind, Edge> = {
  self: 'bottom',
  shimocha: 'right',
  toimen: 'top',
  kamicha: 'left',
}
// Spectator fallback: with no hero seat, relativeKind() collapses everyone to
// 'self', so place seats by absolute index instead.
const IDENTITY_EDGE: Edge[] = ['bottom', 'right', 'top', 'left']

function seatEdge(seat: number, ourSeat: number | null, numPlayers: number): Edge {
  if (ourSeat == null) return IDENTITY_EDGE[seat] ?? 'bottom'
  return KIND_EDGE[relativeKind(seat, ourSeat, numPlayers)]
}

// Largest square that fits the (possibly non-square) content box. The pixel
// size is set on `.board-square` so every CSS percentage inside resolves
// against a known square and mahgen containers report stable widths.
function useSquareSize(ref: RefObject<HTMLElement | null>): number {
  const [size, setSize] = useState(0)
  useEffect(() => {
    const el = ref.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect()
      setSize(Math.max(0, Math.floor(Math.min(r.width, r.height))))
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [ref])
  return size
}

export function BoardTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)
  const view = useGameStore((s) => s.view)
  const rootRef = useRef<HTMLDivElement>(null)
  const size = useSquareSize(rootRef)

  const numPlayers = game?.num_players ?? view?.num_players ?? 4
  const ourSeat = game?.our_seat ?? null
  // Scales board chrome (dora panel, etc.) with the board so it doesn't look
  // empty when the tile is enlarged. The tiles themselves scale via their
  // container widths (fractions of the square); this covers the fixed-size bits.
  const boardScale = Math.min(2.4, Math.max(0.8, size / 460))

  return (
    <TileFrame id="board" title={t('tile.board')} bp={bp} contentClassName="p-0">
      <div ref={rootRef} className="board-root">
        {game && view && size > 0 ? (
          <div
            className="board-square"
            style={{ width: size, height: size, '--board-scale': boardScale } as CSSProperties}
          >
            {/* dora indicators, top-left */}
            {view.dora_indicators && (
              <div className="board-dora">
                <span className="board-dora-label">{t('tile.dora')}</span>
                <Mahgen seq={view.dora_indicators} kind="dora" />
              </div>
            )}

            {/* rotated tile layer per seat (hand / river / melds) */}
            {Array.from({ length: numPlayers }, (_, seat) => {
              const pview = view.players[seat]
              if (!pview) return null
              const edge = seatEdge(seat, ourSeat, numPlayers)
              return (
                <div
                  key={seat}
                  className="board-seat"
                  style={{ transform: `rotate(${EDGE_ROT[edge]}deg)` }}
                >
                  {pview.river && (
                    <div className="board-seat-river">
                      <Mahgen seq={pview.river} kind="board-river" riverMode />
                    </div>
                  )}
                  {pview.melds.length > 0 && (
                    <div className="board-seat-melds">
                      {pview.melds.map((m, i) => (
                        <Mahgen key={i} seq={m} kind="board-meld" />
                      ))}
                    </div>
                  )}
                  {pview.hand && (
                    <div className="board-seat-hand">
                      <Mahgen seq={pview.hand} kind="board-hand" />
                    </div>
                  )}
                </div>
              )
            })}

            {/* central panel: each seat's score around it + round info */}
            <div className="board-center">
              {Array.from({ length: numPlayers }, (_, seat) => {
                const player = game.players[seat]
                if (!player) return null
                const edge = seatEdge(seat, ourSeat, numPlayers)
                return (
                  <div key={seat} className={`board-score board-score--${edge}`}>
                    <span className="board-score-wind">
                      {bakazeFor(seat, game.oya, numPlayers)}
                      {player.riichi_declared && <span className="board-score-riichi">●</span>}
                    </span>
                    <span className="board-score-pts">{fmtScore(player.score)}</span>
                  </div>
                )
              })}
              {/* current-turn indicator: a slow-blinking bar on the active seat's side */}
              {game.players[game.current_player] && (
                <div
                  className={`board-turn-bar board-turn-bar--${seatEdge(game.current_player, ourSeat, numPlayers)}`}
                />
              )}
              <div className="board-center-mid">
                <div className="board-center-round">{kyokuLabel(game.bakaze, game.kyoku)}</div>
                <div className="board-center-sub">
                  <span className="board-stick">
                    <img src="/100_mini.svg" alt="honba" />×{game.honba}
                  </span>
                  <span className="board-stick">
                    <img src="/1000_mini.svg" alt="kyotaku" />×{game.kyotaku}
                  </span>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <span className="board-empty">{t('tile.board_empty')}</span>
        )}
      </div>
    </TileFrame>
  )
}
