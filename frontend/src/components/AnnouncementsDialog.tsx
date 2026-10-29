import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, Megaphone } from 'lucide-react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { DiscordMark } from '@/components/DiscordMark'
import { MOKORA_DISCORD_URL, openExternal } from '@/lib/external'
import { cn } from '@/lib/utils'
import { NOTE_SECTIONS, type AnnouncementEntry } from '@/announcements/entries'
import { getAppVersion } from '@/lib/appVersion'
import { HAS_TAURI, listen } from '@/lib/tauri'
import { useAnnouncementStore } from '@/stores/announcementStore'

// Delay so the announcements don't pop over the very first paint; keeps
// them clear of the UpdateNotifier toast (3s), which stacks fine next to
// a dialog.
const OPEN_DELAY_MS = 1500

/**
 * In-app announcements dialog: one collapsible row per entry (release
 * or product news), pinned entries (the welcome card) on top and the rest
 * newest first. Only the newest release starts expanded. The dialog keeps
 * one size and the list scrolls inside it; opening a row scrolls down to
 * the end of what it opened. The running version is shown here (and only
 * here). Two ways in:
 *  - launch: a new bundled entry opens the complete history after a short
 *    delay. Any close records the newest entry as seen, so it opens once
 *    per new announcement.
 *  - history: Settings → Updates → "Announcements" opens that same list.
 */
export function AnnouncementsDialog() {
  const { t } = useTranslation()
  const open = useAnnouncementStore((s) => s.open)
  const entries = useAnnouncementStore((s) => s.entries)
  const close = useAnnouncementStore((s) => s.close)
  const [version, setVersion] = useState<string | null>(null)
  useEffect(() => {
    void getAppVersion().then(setVersion)
  }, [])
  // One-shot per app launch (the component remounts on route changes but
  // the store's armed/seen state is global, so the ref is just belt and
  // braces against double-arming the timer under StrictMode).
  const fired = useRef(false)

  useEffect(() => {
    if (fired.current) return
    fired.current = true
    let live = true
    let timer = 0
    // What another account already showed counts: read it before deciding.
    void useAnnouncementStore
      .getState()
      .hydrate()
      .then(() => {
        if (!live || !useAnnouncementStore.getState().prepareLaunch()) return
        timer = window.setTimeout(() => {
          useAnnouncementStore.getState().showLaunch()
        }, OPEN_DELAY_MS)
      })
    return () => {
      live = false
      window.clearTimeout(timer)
    }
  }, [])

  // A copy opened in a hidden account interface closes when any account
  // closes the announcements.
  useEffect(() => {
    if (!HAS_TAURI) return
    let live = true
    let stop: (() => void) | undefined
    void Promise.resolve(
      listen<string>('announcement-seen', (date) =>
        useAnnouncementStore.getState().seenElsewhere(date),
      ),
    )
      .then((unlisten) => {
        if (live) stop = unlisten
        else unlisten?.()
      })
      .catch(() => {})
    return () => {
      live = false
      stop?.()
    }
  }, [])

  return (
    <Dialog open={open} onOpenChange={(v) => !v && close()}>
      {/* Wider than the standard dialogs to fit screenshots and two-column
          feature grids. One fixed height whatever is open: the middle grid
          row scrolls, and the dialog never grows past the window. */}
      <DialogContent
        data-native-blur
        data-native-passive
        className="sm:max-w-4xl h-[min(28rem,85vh)] grid-rows-[auto_minmax(0,1fr)_auto]"
      >
        <DialogHeader>
          <DialogTitle className="flex items-baseline gap-3 pr-8">
            {t('announcements.dialog.title')}
            {version && (
              <span className="text-xs font-normal text-muted-foreground">
                {t('announcements.dialog.current_version', { version })}
              </span>
            )}
          </DialogTitle>
          {/* For screen readers only: the header shows just the title. */}
          <DialogDescription className="sr-only">
            {t('announcements.dialog.intro')}
          </DialogDescription>
        </DialogHeader>

        {/* Keyed by the shown id set so a changed bundled history remounts
            the list and resets expansion to "newest only" without
            effect-driven state syncing. */}
        <AnnouncementList key={entries.map((e) => e.id).join('|')} entries={entries} />

        <DialogFooter className="bg-transparent p-0 border-0 mx-0 mb-0 flex-row items-center justify-between gap-2 sm:justify-between">
          <button
            type="button"
            onClick={() => openExternal(MOKORA_DISCORD_URL)}
            className="inline-flex items-center gap-2 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground"
          >
            <DiscordMark className="size-4 shrink-0" />
            <span>{t('announcements.dialog.discord_purchase')}</span>
          </button>
          <Button size="sm" onClick={close}>
            {t('announcements.dialog.got_it')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function AnnouncementList({ entries }: { entries: AnnouncementEntry[] }) {
  // Pinned first; the sort is stable, so the rest keep their newest-first order.
  const shown = [...entries].sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned))
  // Explicit toggles; the newest release starts expanded, everything else
  // (the pinned welcome card, older releases) collapsed.
  const [expanded, setExpanded] = useState<Record<string, boolean>>(() => {
    const newest = shown.find((e) => !e.pinned)
    return newest ? { [newest.id]: true } : {}
  })
  return (
    <ul className="grid gap-2 content-start overflow-y-auto min-h-0 pr-1">
      {shown.map((entry) => (
        <AnnouncementRow
          key={entry.id}
          entry={entry}
          expanded={!!expanded[entry.id]}
          onToggle={() =>
            setExpanded((prev) => ({ ...prev, [entry.id]: !prev[entry.id] }))
          }
        />
      ))}
    </ul>
  )
}

function AnnouncementRow({
  entry,
  expanded,
  onToggle,
}: {
  entry: AnnouncementEntry
  expanded: boolean
  onToggle: () => void
}) {
  const { t, i18n } = useTranslation()
  // The ISO date renders in the viewer's locale; parse as local midnight
  // (plain YYYY-MM-DD strings would otherwise be treated as UTC and can
  // display one day off west of Greenwich).
  const date = new Date(`${entry.date}T00:00:00`).toLocaleDateString(i18n.language)
  // Opened by a press (not on the first showing): bring the end of what
  // opened into view.
  const row = useRef<HTMLLIElement>(null)
  const pressed = useRef(false)
  useEffect(() => {
    if (!expanded || !pressed.current) return
    row.current?.scrollIntoView({ block: 'end', behavior: 'smooth' })
  }, [expanded])
  return (
    <li ref={row} className="rounded-lg border border-border">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => {
          pressed.current = true
          onToggle()
        }}
        className="flex w-full items-center gap-3 rounded-lg p-3 text-left hover:bg-muted/50"
      >
        {entry.version ? (
          <span className="shrink-0 rounded-md border border-border bg-muted px-1.5 py-0.5 font-mono text-xs font-semibold">
            v{entry.version}
          </span>
        ) : (
          <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-muted">
            <Megaphone className="size-3.5" />
          </span>
        )}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {t(`announcements.entries.${entry.id}.title`)}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">{date}</span>
        <ChevronDown
          className={cn(
            'size-4 shrink-0 text-muted-foreground transition-transform',
            expanded && 'rotate-180',
          )}
        />
      </button>

      {expanded && (
        <div className="grid gap-4 border-t border-border p-3">
          {entry.image && (
            <img
              src={entry.image}
              alt={t(`announcements.entries.${entry.id}.image_alt`)}
              className="w-full rounded-md border border-border"
            />
          )}
          {entry.features && (
          <ul className="grid gap-3 sm:grid-cols-2">
            {entry.features.map((f) => {
              const Icon = f.icon
              return (
                <li key={f.key} className="flex items-start gap-3">
                  <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-foreground">
                    <Icon className="size-4" />
                  </span>
                  <div className="min-w-0">
                    <div className="text-sm font-medium">
                      {t(`announcements.entries.${entry.id}.${f.key}_title`)}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {t(`announcements.entries.${entry.id}.${f.key}_desc`)}
                    </div>
                  </div>
                </li>
              )
            })}
          </ul>
          )}
          {entry.notes &&
            NOTE_SECTIONS.map((section) => {
              const items = entry.notes?.[section]
              if (!items?.length) return null
              return (
                <section key={section} className="grid gap-1.5">
                  <div className="text-sm font-medium">
                    {t(`announcements.dialog.section_${section}`)}
                  </div>
                  <ul className="grid list-disc gap-1 pl-5 text-xs text-muted-foreground">
                    {items.map((item) => (
                      <li key={item}>{t(`announcements.entries.${entry.id}.${item}`)}</li>
                    ))}
                  </ul>
                </section>
              )
            })}
          {entry.link && (
            <Button
              size="sm"
              className="justify-self-start"
              onClick={() => openExternal(entry.link!)}
            >
              {t(`announcements.entries.${entry.id}.link_label`)}
            </Button>
          )}
        </div>
      )}
    </li>
  )
}
