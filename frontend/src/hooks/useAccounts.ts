import { useEffect } from 'react'
import { listen } from '@/lib/tauri'
import { create } from 'zustand'
import { fetchAccounts, type AccountInfo } from '@/lib/accounts'

/** How often the list is re-read while something shows it: the other
 *  Mokora instances start, stop and log in on their own. */
export const REFRESH_MS = 5000

// One list, one poller, shared by every component that shows accounts (the
// avatar, the tabs, the power button, the quit dialog, the picker, the status
// bar). Each read goes through the backend (files and avatars per account), so
// the poller runs once however many components are mounted: it starts with the
// first and stops with the last.
const useAccountsStore = create<{ accounts: AccountInfo[] }>(() => ({ accounts: [] }))

let users = 0
let timer: ReturnType<typeof setInterval> | undefined
let stopListening: (() => void) | undefined

function load() {
  fetchAccounts()
    .then((accounts) => useAccountsStore.setState({ accounts }))
    .catch(() => {})
}

function acquire() {
  users += 1
  if (users > 1) return
  load()
  timer = setInterval(load, REFRESH_MS)
  void listen('accounts-changed', load).then((unlisten) => {
    // Everyone may have left before the listener was ready.
    if (users === 0) unlisten()
    else stopListening = unlisten
  })
}

function release() {
  users -= 1
  if (users > 0) return
  clearInterval(timer)
  timer = undefined
  stopListening?.()
  stopListening = undefined
}

/**
 * Every account (`list_accounts`), kept current: re-read every few seconds
 * and on `accounts-changed`. `refresh` re-reads at once (after a switch, a
 * log-out…). Errors keep the last list. Shared by the account picker, the
 * tabs, the status bar and the quit flow.
 */
export function useAccounts(): { accounts: AccountInfo[]; refresh: () => void } {
  const accounts = useAccountsStore((s) => s.accounts)
  useEffect(() => {
    acquire()
    return release
  }, [])
  return { accounts, refresh: load }
}
