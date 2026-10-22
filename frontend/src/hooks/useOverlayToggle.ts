import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/components/ui/sonner'
import { useConfigStore } from '@/stores/configStore'
import { invoke } from '@/lib/tauri'

// Show/hide the always-on-top suggestion overlay (the floating window). Drives
// the persisted `overlay.enabled` flag via `set_overlay_enabled`; the backend
// broadcasts `overlay-config` afterwards, so every button that uses this, the
// Settings card and the overlay's own x can never disagree about whether it is
// open.
export function useOverlayToggle() {
  const { t } = useTranslation()
  const overlay = useConfigStore((s) => s.config?.overlay)
  const setOverlay = useConfigStore((s) => s.setOverlay)
  const [busy, setBusy] = useState(false)

  const toggle = async () => {
    if (!overlay) return
    const next = !overlay.enabled
    setBusy(true)
    // Optimistic: the window takes a moment to appear and a button that does
    // nothing for half a second reads as broken. `overlay-config` confirms.
    setOverlay({ ...overlay, enabled: next })
    try {
      await invoke('set_overlay_enabled', { enabled: next })
    } catch (e) {
      setOverlay({ ...overlay, enabled: !next })
      toast.error(t('game.overlay_toggle_failed'), { description: String(e) })
    } finally {
      setBusy(false)
    }
  }

  return {
    available: !!overlay,
    enabled: overlay?.enabled ?? false,
    busy,
    toggle,
    label: overlay?.enabled ? t('game.overlay_hide') : t('game.overlay_show'),
  }
}
