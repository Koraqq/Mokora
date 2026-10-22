import { useEffect, useState } from 'react'

import { HAS_TAURI, invoke, listen } from '@/lib/tauri'
import type { AccountProfile } from '@/types'

/**
 * This account's profile (`get_profile`): coins, ranks, today's revive
 * coins and the game's own statistics. Read again whenever the backend says
 * it changed (`profile-changed`) — opening the page also asks the game for
 * fresh statistics (`refresh_profile_stats`). `undefined` while loading, `null` before
 * the account has ever logged in.
 */
export function useProfile(): AccountProfile | null | undefined {
  const [profile, setProfile] = useState<AccountProfile | null | undefined>(undefined)

  useEffect(() => {
    if (!HAS_TAURI) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setProfile(null)
      return
    }
    const load = () => {
      invoke<AccountProfile | null>('get_profile')
        .then(setProfile)
        .catch(() => setProfile(null))
    }
    load()
    // Ask the game for fresh statistics too (it opens the profile window from
    // the lobby, while autoplay is on); the answer arrives as a change.
    invoke('refresh_profile_stats').catch(() => {})
    let unlisten: (() => void) | undefined
    listen('profile-changed', load).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [])

  return profile
}
