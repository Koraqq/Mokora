import { useMemo } from 'react'
import { create } from 'zustand'

import { DEFAULT_MENU_ORDER, LEGACY_MENU_ORDER, normalizeMenuOrder } from '@/lib/menuList'
import { persist, createJSONStorage } from 'zustand/middleware'

// Ported from https://github.com/salimi-my/shadcn-ui-sidebar (MIT).
// Differences from the upstream registry:
//   * Plain object spread in `setSettings` instead of immer.produce — keeps
//     us off another runtime dep for a single shallow merge.
//   * `isHoverOpen` defaults to true — that's the whole reason we adopted
//     this sidebar (peek-on-hover is the headline UX).
//   * Persist key `mokora.sidebar` (was `sidebar`) — namespaced like the
//     other Mokora localStorage entries so clearing app state stays scoped.
//   * Dropped the `useStore` hydration shim — Vite is client-only so the
//     persist middleware reads localStorage synchronously on first render.

export type SidebarSettings = { disabled: boolean; isHoverOpen: boolean }

type SidebarStore = {
  isOpen: boolean
  isHover: boolean
  /**
   * Overlay-drawer visibility, used only below the `lg` breakpoint where the
   * sidebar can't be docked. Independent of `isOpen`, which is the *pinned*
   * width of the docked sidebar.
   */
  isDrawerOpen: boolean
  settings: SidebarSettings
  /**
   * Settings shown while the appearance page is being edited: on top of
   * `settings`, never stored, gone with the page (or the next reload).
   */
  preview: Partial<SidebarSettings> | null
  setPreview: (preview: Partial<SidebarSettings> | null) => void
  /** The order of the pages (their ids), as kept; see `lib/menuList`. */
  order: string[]
  setOrder: (order: string[]) => void
  /** An order shown while the appearance page is edited, kept by no one. */
  previewOrder: string[] | null
  setPreviewOrder: (order: string[] | null) => void
  /** The order is being set: the sidebar is open and its pages can be picked
   *  and moved. Never stored. */
  ordering: boolean
  setOrdering: (ordering: boolean) => void
  /** The page picked to move while ordering. */
  picked: string | null
  setPicked: (id: string | null) => void
  toggleOpen: () => void
  setIsOpen: (isOpen: boolean) => void
  setIsHover: (isHover: boolean) => void
  setDrawerOpen: (isDrawerOpen: boolean) => void
  getOpenState: () => boolean
  setSettings: (settings: Partial<SidebarSettings>) => void
}

// `isHover` and `isDrawerOpen` are transient view state, not preferences.
// Persisting the drawer would reopen it — backdrop and all — on every launch.
type PersistedSidebar = Pick<SidebarStore, 'isOpen' | 'settings' | 'order'>

/** The sidebar's settings as they show: the saved ones with a preview over them. */
function effectiveSettings(s: Pick<SidebarStore, 'settings' | 'preview'>): SidebarSettings {
  return { ...s.settings, ...s.preview }
}

/** What the sidebar, the top bar and the layout follow. */
export function useSidebarSettings(): SidebarSettings {
  const settings = useSidebar((s) => s.settings)
  const preview = useSidebar((s) => s.preview)
  return useMemo(() => ({ ...settings, ...preview }), [settings, preview])
}

/** The order the sidebar's pages show in: the preview over the kept one. */
export function useMenuOrder(): string[] {
  const order = useSidebar((s) => s.order)
  const preview = useSidebar((s) => s.previewOrder)
  return useMemo(() => normalizeMenuOrder(preview ?? order), [order, preview])
}

export const useSidebar = create(
  persist<SidebarStore, [], [], PersistedSidebar>(
    (set, get) => ({
      isOpen: true,
      isHover: false,
      isDrawerOpen: false,
      settings: { disabled: false, isHoverOpen: true },
      preview: null,
      setPreview: (preview) => set({ preview }),
      order: [...DEFAULT_MENU_ORDER],
      setOrder: (order) => set({ order: normalizeMenuOrder(order) }),
      previewOrder: null,
      setPreviewOrder: (previewOrder) => set({ previewOrder }),
      ordering: false,
      setOrdering: (ordering) => set({ ordering, picked: null }),
      picked: null,
      setPicked: (picked) => set({ picked }),
      toggleOpen: () => set({ isOpen: !get().isOpen }),
      setIsOpen: (isOpen) => set({ isOpen }),
      setIsHover: (isHover) => set({ isHover }),
      setDrawerOpen: (isDrawerOpen) => set({ isDrawerOpen }),
      getOpenState: () => {
        const s = get()
        return s.isOpen || (effectiveSettings(s).isHoverOpen && s.isHover)
      },
      setSettings: (patch) =>
        set({ settings: { ...get().settings, ...patch } }),
    }),
    {
      name: 'mokora.sidebar',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ isOpen: s.isOpen, settings: s.settings, order: s.order }),
      // Version 1: the page order has a new default. An order that is still the
      // old default was never picked by anyone, so it follows the new one.
      version: 1,
      migrate: (stored, version) => {
        const state = stored as PersistedSidebar
        if (version < 1 && JSON.stringify(state.order) === JSON.stringify(LEGACY_MENU_ORDER)) {
          return { ...state, order: [...DEFAULT_MENU_ORDER] }
        }
        return state
      },
    },
  ),
)
