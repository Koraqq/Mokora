import { useEffect, useRef, useState } from 'react'
import { useBlocker } from 'react-router-dom'

import { AppearanceSection } from '@/components/settings/AppearanceSection'
import type { AppearanceDraft } from '@/components/settings/draft'
import { UnsavedBar } from '@/components/settings/UnsavedBar'
import { useSidebar } from '@/hooks/useSidebar'
import {
  applyLook,
  currentLook,
  customScalePercent,
  previewLook,
  reconcileAppearance,
  rememberOwnLook,
  restoreLook,
  useSharedAppearanceStore,
  type Look,
} from '@/lib/appearance'
import { countChangedSettings } from '@/lib/configDiff'
import { canAddScalePreset } from '@/stores/uiPrefsStore'
import { invoke } from '@/lib/tauri'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'
import { useThemeStore } from '@/stores/themeStore'
import { useUiPrefsStore } from '@/stores/uiPrefsStore'

function lookOf(draft: AppearanceDraft): Look {
  return {
    mode: draft.themeMode,
    palette: draft.themePalette,
    custom: draft.customTheme,
    scale: draft.uiScale,
    menu: draft.menuOrder,
    presets: draft.scalePresets,
  }
}

function lookFields(
  look: Look,
): Pick<AppearanceDraft, 'themeMode' | 'themePalette' | 'customTheme' | 'uiScale' | 'menuOrder' | 'scalePresets'> {
  return {
    themeMode: look.mode,
    themePalette: look.palette,
    customTheme: structuredClone(look.custom),
    uiScale: look.scale,
    menuOrder: [...look.menu],
    scalePresets: [...look.presets],
  }
}

function currentAppearance(): AppearanceDraft {
  const sidebar = useSidebar.getState().settings
  return {
    ...lookFields(currentLook()),
    sharedAppearance: useSharedAppearanceStore.getState().shared,
    sidebarHoverOpen: sidebar.isHoverOpen,
    sidebarDisabled: sidebar.disabled,
    zoomShortcut: useUiPrefsStore.getState().zoomShortcut,
  }
}

/** Keep the edited look: for this account, or for all of them while sharing.
 *  Ending sharing gives every account (this one too) its own look back. */
async function saveLook(before: AppearanceDraft, after: AppearanceDraft): Promise<void> {
  if (after.sharedAppearance) {
    // The look this account had is its own; it comes back when sharing ends.
    if (!before.sharedAppearance) rememberOwnLook(lookOf(before))
    applyLook(lookOf(after))
    useSharedAppearanceStore.setState({ shared: true })
    await invoke('set_appearance', { look: { shared: true, ...lookOf(after) } })
    return
  }
  if (before.sharedAppearance) {
    const shared = { shared: false, ...lookOf(before) }
    await invoke('set_appearance', { look: shared })
    reconcileAppearance(shared)
    return
  }
  applyLook(lookOf(after))
}

export function Appearance() {
  const [saved, setSaved] = useState<AppearanceDraft>(currentAppearance)
  const [draft, setDraft] = useState<AppearanceDraft>(currentAppearance)
  const [saving, setSaving] = useState(false)
  // The sidebar's order is being set: it is open and its pages can be moved.
  const [ordering, setOrdering] = useState(false)
  // A size typed in by hand: the field is out, and having it out is a change.
  const [customOpen, setCustomOpen] = useState(false)
  const [customText, setCustomText] = useState('')
  const orderAtStart = useRef<string[]>([])
  const [err, setErr] = useState<string | null>(null)

  const changes = countChangedSettings(draft, saved) + (customOpen ? 1 : 0)
  const dirty = changes > 0
  // The size typed for a new button, if it can become one.
  const typedPercent = customScalePercent(customText)
  const typedPreset =
    typedPercent !== null && canAddScalePreset(typedPercent, draft.scalePresets) ? typedPercent : null

  // Changes waiting lock the rest of the app until they are saved or
  // cancelled, exactly like the settings page.
  const setChanges = useSettingsGuardStore((s) => s.setChanges)
  const nudge = useSettingsGuardStore((s) => s.nudge)
  useEffect(() => {
    setChanges(changes)
  }, [changes, setChanges])
  useEffect(() => () => setChanges(0), [setChanges])

  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty && currentLocation.pathname !== nextLocation.pathname,
  )
  useEffect(() => {
    if (blocker.state !== 'blocked') return
    blocker.reset?.()
    nudge()
  }, [blocker, nudge])

  useEffect(() => {
    if (!dirty) return
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [dirty])

  // The look shows while it is edited and goes back when it is not saved.
  // Nothing is stored until Save: `previewLook` only touches the page.
  useEffect(() => {
    previewLook(lookOf(draft))
  }, [draft])
  useEffect(() => () => restoreLook(), [])
  // The sidebar's two switches show the same way, over what is kept.
  useEffect(() => {
    useSidebar.getState().setPreview({
      isHoverOpen: draft.sidebarHoverOpen,
      disabled: draft.sidebarDisabled,
    })
  }, [draft.sidebarHoverOpen, draft.sidebarDisabled])
  useEffect(() => () => useSidebar.getState().setPreview(null), [])

  // The order shows in the sidebar as it is edited. Moving a page there
  // changes the sidebar's preview; this page's draft follows it, and the
  // other way round, each only when they differ.
  const draftRef = useRef(draft)
  useEffect(() => {
    draftRef.current = draft
  })
  useEffect(() => {
    const shown = useSidebar.getState().previewOrder
    if (JSON.stringify(shown) !== JSON.stringify(draft.menuOrder)) {
      useSidebar.getState().setPreviewOrder(draft.menuOrder)
    }
  }, [draft.menuOrder])
  useEffect(
    () =>
      useSidebar.subscribe((state) => {
        const moved = state.previewOrder
        if (moved && JSON.stringify(moved) !== JSON.stringify(draftRef.current.menuOrder)) {
          setDraft({ ...draftRef.current, menuOrder: moved })
        }
      }),
    [],
  )
  useEffect(() => {
    useSidebar.getState().setOrdering(ordering)
    // A narrow window shows the sidebar as a drawer: open it for the job.
    useSidebar.getState().setDrawerOpen(ordering)
  }, [ordering])
  useEffect(
    () => () => {
      useSidebar.getState().setOrdering(false)
      useSidebar.getState().setPreviewOrder(null)
    },
    [],
  )

  // Another account saved a shared look (or ended sharing) while nothing is
  // being edited here: take it over instead of showing the old values.
  const dirtyRef = useRef(false)
  useEffect(() => {
    dirtyRef.current = dirty
  }, [dirty])
  useEffect(() => {
    const refresh = () => {
      if (dirtyRef.current) return
      const fresh = currentAppearance()
      setSaved(fresh)
      setDraft(structuredClone(fresh))
    }
    const stops = [
      useThemeStore.subscribe(refresh),
      useUiPrefsStore.subscribe(refresh),
      useSharedAppearanceStore.subscribe(refresh),
    ]
    return () => stops.forEach((stop) => stop())
  }, [])

  const save = async () => {
    setSaving(true)
    setErr(null)
    try {
      // The size typed for a new button joins the others when saved.
      const toSave =
        customOpen && typedPreset !== null
          ? { ...draft, scalePresets: [...draft.scalePresets, typedPreset].sort((x, y) => x - y) }
          : draft
      await saveLook(saved, toSave)
      useSidebar.getState().setSettings({
        isHoverOpen: draft.sidebarHoverOpen,
        disabled: draft.sidebarDisabled,
      })
      useUiPrefsStore.getState().setZoomShortcut(draft.zoomShortcut)
      // Ending sharing hands the account its own look back, which may differ
      // from the draft that was on screen.
      const kept = { ...toSave, ...lookFields(currentLook()) }
      setSaved(structuredClone(kept))
      setDraft(structuredClone(kept))
      // Saving ends the setting of the order.
      setOrdering(false)
      setCustomOpen(false)
      setCustomText('')
    } catch (e) {
      setErr(String(e))
    } finally {
      setSaving(false)
    }
  }

  const cancel = () => {
    setErr(null)
    setOrdering(false)
    setCustomOpen(false)
    setCustomText('')
    setDraft(structuredClone(saved))
  }

  const startOrdering = () => {
    orderAtStart.current = draft.menuOrder
    setOrdering(true)
  }
  // Leaving without changing anything: the order is what it was on entry.
  const stopOrdering = () => {
    setDraft({ ...draft, menuOrder: orderAtStart.current })
    setOrdering(false)
  }

  return (
    <div className="flex w-full flex-col gap-6 p-6">
      <AppearanceSection
        draft={draft}
        setDraft={setDraft}
        ordering={ordering}
        onStartOrdering={startOrdering}
        onStopOrdering={stopOrdering}
        custom={{
          open: customOpen,
          text: customText,
          valid: typedPreset !== null,
          onOpen: () => {
            setCustomText('')
            setCustomOpen(true)
          },
          onType: setCustomText,
        }}
      />
      {dirty && (
        <UnsavedBar
          changes={changes}
          saving={saving}
          error={err}
          onCancel={cancel}
          onSave={save}
          saveDisabled={customOpen && !typedPreset}
        />
      )}
    </div>
  )
}
