import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import { fmtScore, relativeKind, bakazeFor } from '@/lib/format'
import type { Breakpoint, TileId } from '@/tiles/defaults'

const SEAT_TO_TILE: Record<number, TileId> = {
  0: 'player-0',
  1: 'player-1',
  2: 'player-2',
  3: 'player-3',
}

const KIND_TKEY: Record<string, string> = {
  self: 'mahjong.self',
  shimocha: 'mahjong.shimocha',
  toimen: 'mahjong.toimen',
  kamicha: 'mahjong.kamicha',
}

export function PlayerTile({ seat, bp }: { seat: number; bp: Breakpoint }) {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)
  const view = useGameStore((s) => s.view)
  const numPlayers = game?.num_players ?? 4
  const player = game?.players[seat]
  const playerView = view?.players[seat]
  const ourSeat = game?.our_seat ?? null
  const kind = relativeKind(seat, ourSeat, numPlayers)
  const isSelf = kind === 'self'
  const id = SEAT_TO_TILE[seat]
  const title = isSelf
    ? t('tile.player_n_self', { n: seat + 1 })
    : t('tile.player_n', { n: seat + 1 })

  // bakaze of this seat (E/S/W rotates from oya in 3p; E/S/W/N in 4p)
  const seatWind = game ? bakazeFor(seat, game.oya, numPlayers) : '—'
  const kitaCount = player?.kita_tiles?.length ?? 0

  return (
    <TileFrame
      id={id}
      title={title}
      bp={bp}
      rightSlot={
        <span className="text-[10px] uppercase tracking-wider text-muted-foreground px-1">
          {t(KIND_TKEY[kind])}
        </span>
      }
      contentClassName="flex flex-col gap-2"
    >
      <div className="flex items-baseline justify-between">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold">{seatWind}</span>
          {player?.riichi_declared && (
            <span className="rounded bg-amber-500/15 text-amber-400 px-1.5 py-0.5 text-[10px] font-semibold tracking-wider uppercase">
              {t('mahjong.riichi')}
            </span>
          )}
          {kitaCount > 0 && (
            <span
              title={t('mahjong.kita')}
              className="rounded bg-sky-500/15 text-sky-400 px-1.5 py-0.5 text-[10px] font-semibold tracking-wider uppercase"
            >
              北×{kitaCount}
            </span>
          )}
        </div>
        <span className="font-mono text-base font-semibold">{fmtScore(player?.score)}</span>
      </div>

      {playerView && (
        <>
          {playerView.melds.length > 0 && (
            <div className="flex flex-wrap items-end gap-1">
              {playerView.melds.map((meld, i) => (
                <Mahgen key={i} seq={meld} kind="melds" />
              ))}
            </div>
          )}

          {playerView.river && (
            <div className="mt-auto">
              <Mahgen seq={playerView.river} kind="river" riverMode />
            </div>
          )}
        </>
      )}
    </TileFrame>
  )
}
