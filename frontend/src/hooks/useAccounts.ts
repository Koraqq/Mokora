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
//
// Every account's interface stays alive, but only the one on screen shows the
// list: a parked interface skips the timed reads and catches up the moment it
// is shown (`mokora-active`, sent by the backend). `accounts-changed` still
// reaches every interface.
const useAccountsStore = create<{ accounts: AccountInfo[] }>(() => ({ accounts: [] }))

const ACTIVE_EVENT = 'mokora-active'

let users = 0
let timer: ReturnType<typeof setInterval> | undefined
let stopListening: (() => void) | undefined

/** Whether this interface is the one on screen; unknown counts as shown. */
function shown(): boolean {
  return (window as { __MOKORA_ACTIVE__?: boolean }).__MOKORA_ACTIVE__ !== false
}

function sameAccount(a: AccountInfo, b: AccountInfo): boolean {
  const keys = Object.keys(a) as (keyof AccountInfo)[]
  return (
    keys.length === Object.keys(b).length &&
    keys.every((k) => a[k] === b[k] || JSON.stringify(a[k]) === JSON.stringify(b[k]))
  )
}

/** Whether two reads say the same. A new list re-renders every account chip
 *  and decodes every avatar again, so an unchanged read keeps the old one. */
export function sameAccounts(a: AccountInfo[], b: AccountInfo[]): boolean {
  return a.length === b.length && a.every((x, i) => sameAccount(x, b[i]))
}

function load(): Promise<void> {
  return fetchAccounts()
    .then((accounts) => {
      if (!sameAccounts(useAccountsStore.getState().accounts, accounts)) {
        useAccountsStore.setState({ accounts })
      }
    })
    .catch(() => {})
}

function tick() {
  if (shown()) void load()
}

function onActive() {
  if (shown()) void load()
}

function acquire() {
  users += 1
  if (users > 1) return
  load()
  timer = setInterval(tick, REFRESH_MS)
  window.addEventListener(ACTIVE_EVENT, onActive)
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
  window.removeEventListener(ACTIVE_EVENT, onActive)
  stopListening?.()
  stopListening = undefined
}

/**
 * Every account (`list_accounts`), kept current: re-read every few seconds
 * while this interface is shown, the moment it is shown again, and on
 * `accounts-changed`. `refresh` re-reads at once (after a switch, a
 * log-out…). Errors keep the last list. Shared by the account picker, the
 * tabs, the status bar and the quit flow.
 */
export function useAccounts(): { accounts: AccountInfo[]; refresh: () => Promise<void> } {
  const accounts = useAccountsStore((s) => s.accounts)
  useEffect(() => {
    acquire()
    return release
  }, [])
  return { accounts, refresh: load }
}
