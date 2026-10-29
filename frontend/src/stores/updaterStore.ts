import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'
import { invoke } from '@/lib/tauri'

/// Mirrors `crate::updater::check::UpdateInfo` on the Rust side. The
/// shape is fixed by the IPC contract — keep both ends in sync.
export type UpdateInfo = {
  current: string
  latest_tag: string
  latest_version: string
  body: string
  /** The latest-release page; built from the repository name on the Rust side. */
  html_url: string
}

/// How often the periodic check asks, when it is on.
export const PERIODIC_CHECK_MS = 3 * 60 * 1000

/// Frontend-only persisted state. "Don't remind me again" is not here: it
/// is one choice for every account, so the Rust side keeps it
/// (`accounts/update.json`, see `updater::reminders`).
type Persisted = {
  lastChecked: number | null
  /// Check when Mokora starts ("check for updates on launch").
  autoCheckEnabled: boolean
  /// Check every few minutes while no game is played and nothing plays in
  /// full auto (the Rust side holds those back).
  periodicCheckEnabled: boolean
}

type Ephemeral = {
  /// Result of the most recent check. Not persisted — every launch asks
  /// again, so a release that was pulled stops being offered.
  pendingUpdate: UpdateInfo | null
  /// The update prompt. Opened whenever a check finds a newer release.
  dialogOpen: boolean
  /// `true` while a check is in flight.
  checking: boolean
  /// `true` while "Update" downloads and starts the installer.
  installing: boolean
  /// The launch check has run. The notifier remounts whenever the play
  /// screen is entered or left; without this, every switch would ask again
  /// and reopen a prompt the user just dismissed.
  launchChecked: boolean
  /// The version the user answered "Cancel" for in this run: the periodic
  /// check does not ask about it again until the next launch.
  dismissedVersion: string | null
}

type Actions = {
  /// Ask for a newer release. `manual` answers even after "don't remind me
  /// again"; `periodic` is the every-few-minutes check, which keeps quiet
  /// about a version already cancelled.
  checkNow: (manual: boolean, periodic?: boolean) => Promise<void>
  /// The launch check: `checkNow(false)`, once per app run.
  checkOnLaunch: () => Promise<void>
  /// "Don't remind me again" until a newer version is installed.
  muteReminders: () => Promise<void>
  /// "Update": download and run the installer; Mokora closes on success.
  install: () => Promise<void>
  setAutoCheckEnabled: (enabled: boolean) => void
  setPeriodicCheckEnabled: (enabled: boolean) => void
  openDialog: () => void
  /// "Cancel": close, and let the periodic check skip this version.
  dismiss: () => void
  /// Another account interface answered the prompt (cancelled or muted it).
  answeredElsewhere: (version: string) => void
  closeDialog: () => void
}

export type UpdaterStore = Persisted & Ephemeral & Actions

export const useUpdaterStore = create<UpdaterStore>()(
  persist(
    (set, get) => ({
      lastChecked: null,
      autoCheckEnabled: true,
      periodicCheckEnabled: true,
      pendingUpdate: null,
      dialogOpen: false,
      checking: false,
      installing: false,
      launchChecked: false,
      dismissedVersion: null,

      checkOnLaunch: async () => {
        if (get().launchChecked) return
        set({ launchChecked: true })
        await get().checkNow(false)
      },

      checkNow: async (manual, periodic = false) => {
        if (get().checking || get().installing) return
        set({ checking: true })
        try {
          const info = await invoke<UpdateInfo | null>(
            'check_for_update',
            periodic ? { manual, periodic } : { manual },
          )
          const skip =
            periodic &&
            (info === null ||
              get().dialogOpen ||
              info.latest_version === get().dismissedVersion)
          set({ lastChecked: Date.now() })
          if (!skip) set({ pendingUpdate: info, dialogOpen: info !== null })
        } catch (e) {
          // Network / parse failures: automatic checks stay quiet.
          console.warn('check_for_update failed:', e)
          if (manual) throw e
        } finally {
          set({ checking: false })
        }
      },

      muteReminders: async () => {
        await invoke<void>('mute_update_reminders')
        set({ pendingUpdate: null, dialogOpen: false })
      },

      install: async () => {
        if (get().installing) return
        set({ installing: true })
        try {
          await invoke<void>('install_update')
        } finally {
          set({ installing: false })
        }
      },

      setAutoCheckEnabled: (enabled: boolean) => set({ autoCheckEnabled: enabled }),
      setPeriodicCheckEnabled: (enabled: boolean) => set({ periodicCheckEnabled: enabled }),
      openDialog: () => set({ dialogOpen: true }),
      dismiss: () => {
        const version = get().pendingUpdate?.latest_version
        set((s) => ({
          dialogOpen: false,
          dismissedVersion: version ?? s.dismissedVersion,
        }))
        // One answer for every account interface (the backend tells the rest).
        if (version) void Promise.resolve(invoke('dismiss_update', { version })).catch(() => {})
      },
      answeredElsewhere: (version: string) =>
        set((s) => ({
          dialogOpen: false,
          dismissedVersion: version || s.dismissedVersion,
        })),
      closeDialog: () => set({ dialogOpen: false }),
    }),
    {
      name: 'mokora.updater',
      storage: createJSONStorage(() => localStorage),
      partialize: (state): Persisted => ({
        lastChecked: state.lastChecked,
        autoCheckEnabled: state.autoCheckEnabled,
        periodicCheckEnabled: state.periodicCheckEnabled,
      }),
    },
  ),
)
