import type { LucideIcon } from 'lucide-react'
import { Bot, Gamepad2 } from 'lucide-react'

/** One feature highlight inside an announcement's expanded view. */
export type AnnouncementFeature = {
  icon: LucideIcon
  /**
   * i18n leaf name: the dialog reads
   * `announcements.entries.<id>.<key>_title` and `…_desc`.
   */
  key: string
}

/** The sections of a release's notes, in the order they are shown;
 *  `planned` is what the next releases are to bring. */
export const NOTE_SECTIONS = ['added', 'removed', 'fixed', 'planned'] as const
export type NoteSection = (typeof NOTE_SECTIONS)[number]

/**
 * A release's notes, the text published with it on GitHub: per section,
 * i18n leaf names read from `announcements.entries.<id>.<item>`.
 */
export type ReleaseNotes = Partial<Record<NoteSection, string[]>>

export type AnnouncementEntry = {
  /** Stable i18n slug: strings live under `announcements.entries.<id>`. */
  id: string
  /**
   * Publish date, ISO `YYYY-MM-DD`. Must be unique and strictly
   * descending down the array — it doubles as the "which announcements
   * has the user seen" ordering.
   */
  date: string
  /**
   * For release announcements: the exact Cargo.toml package version,
   * rendered as a badge on the row. Product news entries omit this.
   * Purely cosmetic — it does not gate visibility. Entries are bundled
   * into the build, so a client that can see an entry is already on the
   * matching release (or newer); there is nothing to hide.
   */
  version?: string
  /** Optional bundled image shown expanded; requires `<id>.image_alt`. */
  image?: string
  /** Optional external action URL; requires `<id>.link_label`. */
  link?: string
  /**
   * Shown above every other entry whatever its date (the welcome card).
   * The date still orders the array and the seen baseline.
   */
  pinned?: boolean
  /** Highlights in a two-column grid. An entry has these, notes, or both. */
  features?: AnnouncementFeature[]
  /** For release announcements: the release notes, section by section. */
  notes?: ReleaseNotes
}

/**
 * All in-app announcements, newest first. Add an entry (plus locale
 * strings in all four i18n resources) for every release BEFORE tagging
 * it — the release tagging script refuses to tag a version that has no
 * committed entry here. See README.md in this directory for the workflow.
 */
export const ANNOUNCEMENTS: AnnouncementEntry[] = [
  {
    id: 'v0_1_0',
    date: '2026-10-23',
    version: '0.1.0',
    notes: {
      added: ['live_info', 'builtin_models', 'autoplay', 'multi_account', 'history', 'languages'],
      planned: ['planned_tenhou', 'planned_split_view'],
    },
  },
  {
    id: 'welcome',
    date: '2026-10-19',
    pinned: true,
    features: [
      { icon: Bot, key: 'bot' },
      { icon: Gamepad2, key: 'majsoul' },
    ],
  },
]
