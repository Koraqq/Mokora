import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { Mahgen } from '@/components/Mahgen'
import { useGameStore } from '@/stores/gameStore'
import type { Breakpoint } from '@/tiles/defaults'

export function SelfHandTile({ bp }: { bp: Breakpoint }) {
  const { t } = useTranslation()
  const game = useGameStore((s) => s.game)
  const view = useGameStore((s) => s.view)
  const ourSeat = game?.our_seat ?? null
  const hand = ourSeat != null ? view?.players[ourSeat].hand ?? '' : ''

  return (
    <TileFrame id="self-hand" title={t('tile.self_hand')} bp={bp} contentClassName="flex items-center justify-center px-4">
      {hand ? (
        <Mahgen seq={hand} kind="hand" />
      ) : (
        <span className="text-muted-foreground text-sm">{t('tile.self_hand_empty')}</span>
      )}
    </TileFrame>
  )
}
