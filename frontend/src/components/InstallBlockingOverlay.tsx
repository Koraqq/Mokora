import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { Loader2 } from 'lucide-react'
import { useInstallStore } from '@/stores/installStore'

// Full-screen, non-dismissible overlay shown while a bot env install/sync is
// in progress. Sits above the sidebar (z-20) and dialogs (z-50) at z-[100]
// and swallows pointer/keyboard events, so in-app navigation and starting a
// game are impossible until the install completes. There is intentionally no
// close button or backdrop-click handler.
export function InstallBlockingOverlay() {
  const { t } = useTranslation()
  const active = useInstallStore((s) => s.count > 0)
  const title = useInstallStore((s) => s.title)
  const body = useInstallStore((s) => s.body)

  // Block keyboard-driven navigation/shortcuts while the overlay is up.
  useEffect(() => {
    if (!active) return
    const swallow = (e: KeyboardEvent) => {
      e.stopPropagation()
      e.preventDefault()
    }
    window.addEventListener('keydown', swallow, true)
    return () => window.removeEventListener('keydown', swallow, true)
  }, [active])

  if (!active) return null

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-background/85 backdrop-blur-sm"
      role="alertdialog"
      aria-modal="true"
      aria-busy="true"
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div className="mx-4 flex max-w-md flex-col items-center gap-4 rounded-lg border border-border bg-card p-8 text-center shadow-xl">
        <Loader2 className="h-10 w-10 animate-spin text-primary" />
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-semibold">{title ?? t('install_overlay.title')}</h2>
          {body && <p className="text-sm text-muted-foreground">{body}</p>}
        </div>
        <p className="text-xs font-medium text-amber-300">{t('install_overlay.warning')}</p>
      </div>
    </div>
  )
}
