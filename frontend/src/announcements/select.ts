import type { AnnouncementEntry } from './entries'

/**
 * With no recorded baseline we can't tell a fresh install from an
 * upgrade out of a pre-announcements build, so both get the same gentle
 * default: the newest few entries instead of the whole history.
 */
export const MAX_FRESH_ENTRIES = 3

/**
 * What the launch dialog should show: every entry dated after the
 * last-seen baseline (so skip-level updates replay the announcements in
 * between), or the newest `MAX_FRESH_ENTRIES` when there is no baseline.
 * ISO dates order lexically, so plain string compare works.
 *
 * No version gate: entries are bundled into the build, so a client that
 * can see an entry at all is necessarily on the matching release (or a
 * newer one). `entries` is the full list, newest-first.
 */
export function selectUnseenEntries(
  entries: readonly AnnouncementEntry[],
  lastSeenDate: string | null,
): AnnouncementEntry[] {
  if (lastSeenDate === null) return entries.slice(0, MAX_FRESH_ENTRIES)
  return entries.filter((e) => e.date > lastSeenDate)
}
