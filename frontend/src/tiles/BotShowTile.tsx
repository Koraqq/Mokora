import { useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { TileFrame } from '@/components/TileFrame'
import { BotShowList } from '@/components/BotShowList'
import { pickShow, visibleItems } from '@/lib/botShow'
import { useNotifyStore } from '@/stores/notifyStore'
import type { Breakpoint } from '@/tiles/defaults'

// `limit`, `title` and `row` are for the play screen's top-3 table: the same
// rows, cut to the first few, under a fixed name instead of the bot's own title,
// and laid out side by side.
export function BotShowTile({
  bp,
  limit,
  title,
  row = false,
}: {
  bp: Breakpoint
  limit?: number
  title?: string
  row?: boolean
}) {
  const { t } = useTranslation()
  const responses = useNotifyStore((s) => s.responses)

  const show = useMemo(() => {
    for (let i = responses.length - 1; i >= 0; i--) {
      const s = pickShow(responses[i].meta)
      if (s) return s
    }
    return null
  }, [responses])

  const rowRef = useRef<HTMLOListElement>(null)
  // The dashboard tile has room for the bot's full candidate list; the overlay
  // and the play screen cap it.
  const items = visibleItems(show, limit)

  return (
    <TileFrame
      id="bot-show"
      title={title ?? show?.title ?? t('tile.bot_show_default_title')}
      bp={bp}
      contentClassName="p-2"
    >
      {items.length === 0 ? (
        <span className="text-muted-foreground text-sm px-1">{t('tile.bot_show_empty')}</span>
      ) : (
        <BotShowList items={items} containerRef={rowRef} variant={row ? 'row' : 'tile'} />
      )}
    </TileFrame>
  )
}
