// The accounts this machine runs Mokora for (see `src/accounts/`), as
// `list_accounts` reports them, and the History player each one logs in as.

import { invoke } from '@tauri-apps/api/core'

export type Identity = { account_id: number; nickname: string }

export type AccountInfo = {
  id: string
  config_path: string
  identity: Identity | null
  running: boolean
  /** Running with its game connected (capture up, game tab open). */
  online: boolean
  /** Running with its game browser open — Mokora closes after the last one. */
  browser: boolean
  current: boolean
  /** The account the window shows when Mokora starts. */
  startup: boolean
  duplicate: boolean
  /** When its game last came online (ms since the epoch); null if never
   *  recorded. */
  last_login: number | null
  /** The picture the player chose, as a `data:` URL. */
  avatar: string | null
  /** A game is being played on it (it is in the lobby otherwise). */
  in_game: boolean
  /** Its player's main character: the id of an icon in `public/characters/`. */
  character: number | null
  /** It cannot be opened under the license now (past the free limit). */
  locked: boolean
}

/** How many accounts a license without the paid feature may have (the Rust
 *  side's `accounts::FREE_ACCOUNTS`, which also refuses the rest). */
export const FREE_ACCOUNTS = 2

/** Whether another account may be added: always when paid. */
export function canAddAccount(accounts: AccountInfo[], paid: boolean): boolean {
  return paid || accounts.length < FREE_ACCOUNTS
}

export const fetchAccounts = () => invoke<AccountInfo[]>('list_accounts')

// `t` is i18next's TFunction, typed loosely so this module doesn't pull in
// i18next types just to declare a signature.
type Translate = (key: string, options?: Record<string, unknown>) => string

/** How an account is named to the player: its Mahjong Soul nickname, or
 *  "main account" / "account N" until it has logged in. */
export function accountLabel(a: AccountInfo, t: Translate): string {
  return (
    a.identity?.nickname ||
    (a.id === 'main' ? t('sidebar.account.main') : t('sidebar.account.numbered', { n: a.id }))
  )
}

/** The picker's order: the most recent login first, then accounts never
 *  recorded logging in, in their own order. */
export function byRecentLogin(accounts: AccountInfo[]): AccountInfo[] {
  return accounts
    .map((a, i) => ({ a, i }))
    .sort((x, y) => {
      const lx = x.a.last_login
      const ly = y.a.last_login
      if (lx != null && ly != null && lx !== ly) return ly - lx
      if (lx != null && ly == null) return -1
      if (lx == null && ly != null) return 1
      return x.i - y.i
    })
    .map(({ a }) => a)
}

/** The `lib/historyPlayers` key of the Majsoul login of this Mokora's own
 *  account; null before it has ever logged in. */
export function ownPlayerKey(accounts: AccountInfo[]): string | null {
  const identity = accounts.find((a) => a.current)?.identity
  return identity ? `majsoul:${identity.account_id}` : null
}

/** What `delete_account` removed (`crate::accounts::Removed`). */
export type RemovedAccount = {
  /** Game records (with their logs and reviews) removed. */
  records: number
  /** The records stayed: another entry is logged in as the same account. */
  records_kept: boolean
  /** Things that could not be removed. */
  left: string[]
}
