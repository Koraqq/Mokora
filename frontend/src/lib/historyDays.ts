// The days the game list can narrow to: each local calendar day the user
// played on, newest first, with how many games that day holds.

import type { GameRecord } from '@/types'

export type GameDay = { day: string; count: number }

/** `YYYY/MM/DD` of an ISO timestamp, in local time. */
export function dayKey(iso: string): string {
  const d = new Date(iso)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())}`
}

/** Every day with at least one game, newest first. */
export function gameDays(records: GameRecord[]): GameDay[] {
  const counts = new Map<string, number>()
  for (const r of records) {
    const day = dayKey(r.started_at)
    counts.set(day, (counts.get(day) ?? 0) + 1)
  }
  return [...counts]
    .map(([day, count]) => ({ day, count }))
    .sort((a, b) => (a.day < b.day ? 1 : a.day > b.day ? -1 : 0))
}

/** The games played on `day`; every game when `day` is null. */
export function recordsOfDay(records: GameRecord[], day: string | null): GameRecord[] {
  return day == null ? records : records.filter((r) => dayKey(r.started_at) === day)
}
