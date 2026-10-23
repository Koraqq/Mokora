// A notice at the bottom right whenever the server takes something away: a
// key stopped (or deleted), the paid time over, the account stopped. The
// status comes from the Rust side, which asks the server at start-up, every
// hour, when the token runs out and when a room shows its player counts (at
// most every 90 seconds); see `crate::license::check_soon`. It stays until
// closed, so it is not missed during a game.
//
// Mount it once near the root of the tree.

import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { useLicense } from '@/hooks/useLicense'
import { licenseLosses, type LicenseLoss, type LicenseStatus } from '@/lib/license'

function idOf(loss: LicenseLoss): string {
  return loss.kind === 'revoked' ? `license-loss-${loss.hint}` : `license-loss-${loss.kind}`
}

export function LicenseLossNotifier() {
  const { t } = useTranslation()
  const [status] = useLicense()
  const previous = useRef<LicenseStatus | null>(null)

  useEffect(() => {
    if (!status) return
    const losses = licenseLosses(previous.current, status, Math.floor(Date.now() / 1000))
    previous.current = status
    for (const loss of losses) {
      const [title, description] =
        loss.kind === 'revoked'
          ? [t('license.lost.revoked'), t('license.lost.revoked_hint', { hint: loss.hint })]
          : loss.kind === 'expired'
            ? [t('license.lost.expired'), t('license.lost.expired_hint')]
            : [t('license.lost.stopped'), t(`license.state_hint.${loss.state}`)]
      toast.warning(title, { id: idOf(loss), description, duration: Infinity, closeButton: true })
    }
  }, [status, t])

  return null
}
