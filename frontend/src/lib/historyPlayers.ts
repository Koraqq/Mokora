// Who played each recorded game, for the History page's player picker.
//
// A record says whose game it is in two ways: the account id the platform
// sent at the table (`match_info.account_id`, Majsoul only, and only on
// records captured since it was stored) and the nickname in the recorded
// seat (`names[our_seat]`, always). The id is the identity — a nickname can
// change, and two accounts can share one — so it wins wherever it exists.
// A record without one is folded into the account last seen under the same
// nickname on the same platform, so the games recorded before the id was
// captured stay with their account instead of showing up as a second
// player.

import type { GameRecord } from '@/types'

/** One entry of the picker: a stable key, the name to show, how many
 *  games it covers, and when it last played (RFC3339). */
export type HistoryPlayer = {
  key: string
  name: string
  games: number
  lastPlayed: string
}

function nickname(r: GameRecord): string | null {
  if (r.our_seat == null) return null
  const name = r.names[r.our_seat]
  return name ? name : null
}

function accountKey(r: GameRecord): string | null {
  const info = r.match_info
  if (info?.platform === 'majsoul' && info.account_id != null) {
    return `majsoul:${info.account_id}`
  }
  return null
}

/**
 * The player key of every record that has one, by record id. Observer /
 * replay records (no own seat) have no player and are left out — they
 * only show under "all players".
 */
export function playerKeys(records: GameRecord[]): Map<string, string> {
  // Newest first, so a nickname maps to the account that used it last.
  const newestFirst = [...records].sort((a, b) =>
    b.started_at.localeCompare(a.started_at),
  )
  const accountOfName = new Map<string, string>()
  for (const r of newestFirst) {
    const key = accountKey(r)
    const name = nickname(r)
    if (key && name) {
      const nameKey = `${r.platform}:name:${name}`
      if (!accountOfName.has(nameKey)) accountOfName.set(nameKey, key)
    }
  }
  const keys = new Map<string, string>()
  for (const r of records) {
    const key = accountKey(r)
    if (key) {
      keys.set(r.id, key)
      continue
    }
    const name = nickname(r)
    if (!name) continue
    const nameKey = `${r.platform}:name:${name}`
    keys.set(r.id, accountOfName.get(nameKey) ?? nameKey)
  }
  return keys
}

/** `player` value that shows every player's games together. */
export const ALL_PLAYERS = '__all_players__'

/**
 * The records of one player — or all of them for `ALL_PLAYERS`, observer
 * games included. Everything on the History page (stats, the PT chart and
 * its game numbers, the list, the deal-in tiles) reads the result, so they
 * always describe the same games.
 */
export function recordsOfPlayer(
  records: GameRecord[],
  keys: Map<string, string>,
  player: string,
): GameRecord[] {
  if (player === ALL_PLAYERS) return records
  return records.filter((r) => keys.get(r.id) === player)
}

/** Every player in `records`, most recently played first. Each is shown
 *  under the nickname of its latest game. */
export function listPlayers(
  records: GameRecord[],
  keys: Map<string, string> = playerKeys(records),
): HistoryPlayer[] {
  const players = new Map<string, HistoryPlayer>()
  for (const r of records) {
    const key = keys.get(r.id)
    const name = nickname(r)
    if (!key || !name) continue
    const p = players.get(key)
    if (!p) {
      players.set(key, { key, name, games: 1, lastPlayed: r.started_at })
    } else {
      p.games += 1
      if (r.started_at > p.lastPlayed) {
        p.lastPlayed = r.started_at
        p.name = name
      }
    }
  }
  return [...players.values()].sort((a, b) =>
    b.lastPlayed.localeCompare(a.lastPlayed),
  )
}
