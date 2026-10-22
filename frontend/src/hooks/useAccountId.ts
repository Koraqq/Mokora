import { useEffect, useState } from 'react'

import { startOfNextDay } from '@/lib/dailyStats'
import { HAS_TAURI, invoke, listen } from '@/lib/tauri'
import type { AccountProfile } from '@/types'

/**
 * The Mahjong Soul account id this window's account is logged in as, or
 * null before it has logged in. Read from the profile (`get_profile`) and
 * again whenever it changes — unlike `useProfile`, without asking the game
 * to refresh its statistics, so it can sit on screen all the time.
 */
export function useAccountId(): number | null {
  const [id, setId] = useState<number | null>(null)

  useEffect(() => {
    if (!HAS_TAURI) return
    const load = () => {
      invoke<AccountProfile | null>('get_profile')
        .then((p) => setId(p?.account_id ?? null))
        .catch(() => {})
    }
    load()
    let unlisten: (() => void) | undefined
    let gone = false
    listen('profile-changed', load).then((u) => {
      if (gone) u()
      else unlisten = u
    })
    return () => {
      gone = true
      unlisten?.()
    }
  }, [])

  return id
}

/** The current time, moved on as each game day starts (05:00 UTC+8), so what is
 *  "today" follows the clock in a window left open overnight. */
export function useToday(): Date {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    const next = startOfNextDay(now)
    const timer = window.setTimeout(() => setNow(new Date()), next.getTime() - Date.now() + 1000)
    return () => window.clearTimeout(timer)
  }, [now])

  return now
}
