import { type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useLayoutStore } from '@/stores/layoutStore'
import type { Breakpoint, TileId } from '@/tiles/defaults'

type Props = {
  id: TileId
  title: string
  bp: Breakpoint
  rightSlot?: ReactNode
  children: ReactNode
  /** Disable the close button (e.g. for the tile-frame story). */
  closable?: boolean
  contentClassName?: string
}

export function TileFrame({ id, title, bp, rightSlot, children, closable = true, contentClassName }: Props) {
  const { t } = useTranslation()
  const hide = useLayoutStore((s) => s.hide)
  return (
    <Card className="tile-frame h-full overflow-hidden flex flex-col gap-0 py-0">
      {/* `[.border-b]:pb-2`: the card header pads its bottom more when it has a
          bottom border, which would push the title up off centre. */}
      <CardHeader className="tile-drag-handle cursor-move select-none flex flex-row items-center justify-between px-3 py-2 [.border-b]:pb-2 border-b border-border">
        <CardTitle className="text-xs font-medium tracking-wide uppercase text-muted-foreground">
          {title}
        </CardTitle>
        <div className="flex items-center gap-1">
          {rightSlot}
          {closable && (
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onPointerDown={(e) => e.stopPropagation()}
              onClick={() => hide(id, bp)}
              aria-label={t('tile.hide_x', { title })}
            >
              <X className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent className={`flex-1 min-h-0 overflow-auto p-3 ${contentClassName ?? ''}`}>
        {children}
      </CardContent>
    </Card>
  )
}
