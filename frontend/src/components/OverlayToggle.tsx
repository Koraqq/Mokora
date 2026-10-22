import { useTranslation } from 'react-i18next'
import { PictureInPicture2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useOverlayToggle } from '@/hooks/useOverlayToggle'

// Show/hide the always-on-top suggestion overlay, from the Game page toolbar.
// The logic is shared with the play screen's top-bar button (`useOverlayToggle`).
export function OverlayToggle() {
  const { t } = useTranslation()
  const { available, enabled, busy, toggle, label } = useOverlayToggle()
  if (!available) return null

  // The label stays put and the variant carries the state: a button whose text
  // flips between "Show" and "Hide" makes you read it before every click.
  return (
    <Button
      variant={enabled ? 'secondary' : 'ghost'}
      size="sm"
      onClick={toggle}
      disabled={busy}
      className="text-xs"
      title={label}
      aria-label={label}
      aria-pressed={enabled}
    >
      <PictureInPicture2 className="size-4" />
      {t('game.overlay')}
    </Button>
  )
}
