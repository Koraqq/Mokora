import { useTranslation } from 'react-i18next'
import { Loader2 } from 'lucide-react'
import { toast } from 'sonner'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { openExternal } from '@/lib/external'
import { useAnnouncementStore } from '@/stores/announcementStore'
import { useUpdaterStore } from '@/stores/updaterStore'

/// The update prompt: update to the newer version?
///
/// "Don't remind me again" sits at the far left, apart from the answer
/// itself; "Cancel" and "Update" are at the right. Cancel just closes it;
/// Update downloads the release's package, starts installing it and closes
/// Mokora (the installer starts it again). If that cannot be done, the
/// error offers the release page.
///
/// Waits for the announcements dialog: both can come up at launch, and the
/// prompt should not open over the release notes the user is reading.
export function UpdateDialog() {
  const { t } = useTranslation()
  const open = useUpdaterStore((s) => s.dialogOpen)
  const dismiss = useUpdaterStore((s) => s.dismiss)
  const pending = useUpdaterStore((s) => s.pendingUpdate)
  const muteReminders = useUpdaterStore((s) => s.muteReminders)
  const install = useUpdaterStore((s) => s.install)
  const installing = useUpdaterStore((s) => s.installing)
  const announcementsOpen = useAnnouncementStore((s) => s.open)

  if (!pending) return null

  const update = async () => {
    try {
      await install()
    } catch (e) {
      toast.error(t('updates.dialog.install_failed'), {
        description: String(e),
        action: {
          label: t('updates.dialog.open_release'),
          onClick: () => openExternal(pending.html_url),
        },
      })
    }
  }

  const mute = async () => {
    try {
      await muteReminders()
    } catch (e) {
      toast.error(t('updates.dialog.mute_failed', { message: String(e) }))
    }
  }

  return (
    <Dialog
      open={open && !announcementsOpen}
      onOpenChange={(v) => !v && !installing && dismiss()}
    >
      <DialogContent data-native-passive className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('updates.dialog.title', { version: pending.latest_version })}</DialogTitle>
          <DialogDescription>
            {installing ? t('updates.dialog.installing') : t('updates.dialog.question')}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="bg-transparent p-0 border-0 mx-0 mb-0 flex-row items-center justify-between gap-2 sm:justify-between">
          <Button variant="ghost" size="sm" disabled={installing} onClick={mute}>
            {t('updates.dialog.mute')}
          </Button>
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" disabled={installing} onClick={dismiss}>
              {t('updates.dialog.dismiss')}
            </Button>
            <Button size="sm" disabled={installing} onClick={() => void update()}>
              {installing && <Loader2 className="h-4 w-4 animate-spin" />}
              {t('updates.dialog.go')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
