# In-app announcements

Bundled announcement entries shown by `<AnnouncementsDialog />` as a
collapsible list (one row per entry with a one-line title; the newest row
starts expanded). Two kinds of entry share the list:

- **Release announcements** — carry a `version`, shown as a badge on the
  row. The version is cosmetic and does **not** gate visibility: entries
  are bundled into the build, so a client that can see an entry is already
  on the matching release (or newer) — there is nothing to hide.
- **Product news** — no `version` (e.g. the MokoraMS launch).

On launch the dialog compares the entry dates against the persisted
`mokora.announcement.lastSeen` baseline and shows every entry newer than
it (skip-level updates replay all missed announcements); with no baseline
(fresh install / first run of the build that introduced this) it shows
the newest few. Any close records the newest shown entry as seen.
Settings → Updates → "Announcements" reopens the full history at any
time.

## Adding an announcement for a release (pre-release checklist)

Do this **before** tagging the release — the maintainer's release tagging
script (a local script, not part of this repo) refuses to tag a version
that has no committed entry here.

1. **`entries.ts`** — prepend an entry to `ANNOUNCEMENTS` (newest first;
   dates must be unique and strictly descending down the array):

   ```ts
   {
     id: 'v3_6_0',              // i18n slug: version with . / - → _
     date: '2026-08-20',        // planned publish date, ISO
     version: '3.6.0',          // exact version you are about to tag
     image: someScreenshot,     // optional: import from @/assets
     link: 'https://…',         // optional: external action button
     features: [
       { icon: Rocket, key: 'tenhou_autoplay' },
       // 2–4 highlights is the sweet spot; icons come from lucide-react
     ],
   },
   ```

   A release entry normally carries its GitHub release notes instead of
   (or besides) `features`: `notes: { added: [...], removed: [...],
   fixed: [...] }`, each item an i18n leaf under the entry. `pinned: true`
   keeps an entry above all others in the dialog (the welcome card); its
   date still orders the array and the seen baseline.

2. **Locale strings** — under `announcements.entries.<id>` in **all
   four** resources (`en.json`, `ja.json`, `zh-TW.json`, `zh-CN.json`):
   a `title` (the collapsed one-line summary, e.g. "In-app purchases:
   Alipay, Apple Pay, …"), a `<key>_title` + `<key>_desc` pair per
   feature, plus `image_alt` when the entry has an `image` and
   `link_label` when it has a `link`:

   ```jsonc
   "announcements": {
     "entries": {
       "v3_6_0": {
         "title": "…",
         "tenhou_autoplay_title": "…",
         "tenhou_autoplay_desc": "…"
       }
     }
   }
   ```

3. **Commit**, then tag the release as usual.

`entries.test.ts` fails if entries are out of order, duplicated, dated
wrongly, or missing any locale string — run `npm test` in `frontend/` to
validate. Keep highlights user-facing (what changed for the player), not
a commit log; the dialog links to GitHub releases for the full notes.

Product news entries work the same way — just omit `version` and pick a
unique `id`.

## Files

- `entries.ts` — the data: one `AnnouncementEntry` per announcement,
  newest first.
- `select.ts` — pure selection logic (which entries a launch should show).
- Store/UI live in `../stores/announcementStore.ts` and
  `../components/AnnouncementsDialog.tsx`.
