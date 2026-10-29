import { Check, ListOrdered, Monitor, Moon, Paintbrush, Plus, RotateCcw, Sun, Users, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { CustomEditor } from '@/components/settings/CustomThemeEditor'
import type { AppearanceDraft } from '@/components/settings/draft'
import { Toggle } from '@/components/settings/fields'
import { LiquidSlider } from '@/components/settings/LiquidSlider'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { cn } from '@/lib/utils'
import {
  BUILTIN_SCALE_PRESETS,
  MAX_SCALE_PRESETS,
  SCALE_DEFAULT,
  SCALE_MAX,
  SCALE_MIN,
  SCALE_STEP,
} from '@/stores/uiPrefsStore'
import {
  DEFAULT_CUSTOM_DARK,
  DEFAULT_CUSTOM_LIGHT,
  THEME_MODE_DEFAULT,
  THEME_PALETTES,
  THEME_PALETTE_DEFAULT,
  type ThemeMode,
  type ThemePalette,
} from '@/stores/themeStore'

type Setter = (draft: AppearanceDraft) => void
type Swatch = { background: string; card: string; primary: string; text: string; accent: string }

// The built-in palettes as `index.css` defines them, so a card can show a
// palette without applying it.
const SWATCHES: Record<Exclude<ThemePalette, 'custom'>, { light: Swatch; dark: Swatch }> = {
  mokora: {
    light: {
      background: 'oklch(0.99 0.005 110)',
      card: 'oklch(1 0 0)',
      primary: 'oklch(0.57 0.22 27)',
      text: 'oklch(0.21 0.03 150)',
      accent: 'oklch(0.65 0.17 150)',
    },
    dark: {
      background: 'oklch(0.17 0.02 155)',
      card: 'oklch(0.21 0.025 155)',
      primary: 'oklch(0.7 0.21 27)',
      text: 'oklch(0.97 0.008 120)',
      accent: 'oklch(0.72 0.17 150)',
    },
  },
  default: {
    light: {
      background: 'oklch(0.985 0 0)',
      card: 'oklch(1 0 0)',
      primary: 'oklch(0.205 0 0)',
      text: 'oklch(0.145 0 0)',
      accent: 'oklch(0.97 0 0)',
    },
    dark: {
      background: 'oklch(0.145 0 0)',
      card: 'oklch(0.205 0 0)',
      primary: 'oklch(0.922 0 0)',
      text: 'oklch(0.985 0 0)',
      accent: 'oklch(0.371 0 0)',
    },
  },
  crimson: {
    light: {
      background: 'oklch(0.99 0.003 25)',
      card: 'oklch(1 0 0)',
      primary: 'oklch(0.55 0.22 25)',
      text: 'oklch(0.2 0.02 25)',
      accent: 'oklch(0.93 0.05 25)',
    },
    dark: {
      background: 'oklch(0.16 0.02 25)',
      card: 'oklch(0.2 0.025 25)',
      primary: 'oklch(0.72 0.22 25)',
      text: 'oklch(0.97 0.01 25)',
      accent: 'oklch(0.3 0.05 25)',
    },
  },
  slate: {
    light: {
      background: 'oklch(0.99 0.005 250)',
      card: 'oklch(1 0 0)',
      primary: 'oklch(0.32 0.06 260)',
      text: 'oklch(0.2 0.03 260)',
      accent: 'oklch(0.93 0.03 250)',
    },
    dark: {
      background: 'oklch(0.17 0.025 260)',
      card: 'oklch(0.22 0.025 260)',
      primary: 'oklch(0.78 0.1 250)',
      text: 'oklch(0.97 0.01 250)',
      accent: 'oklch(0.3 0.04 255)',
    },
  },
}

// System first, then light, then dark.
const MODES: readonly ThemeMode[] = ['system', 'light', 'dark']
const MODE_ICONS = { light: Sun, dark: Moon, system: Monitor } as const
const GLIDE_MS = 280

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-color-scheme: dark)').matches
}

function resolvedDark(mode: ThemeMode): boolean {
  return mode === 'dark' || (mode === 'system' && systemPrefersDark())
}

function swatchOf(draft: AppearanceDraft, palette: ThemePalette): Swatch {
  const side = resolvedDark(draft.themeMode) ? 'dark' : 'light'
  if (palette !== 'custom') return SWATCHES[palette][side]
  const vars = draft.customTheme[side]
  const base = side === 'dark' ? DEFAULT_CUSTOM_DARK : DEFAULT_CUSTOM_LIGHT
  const background = vars.background ?? base.background ?? '#000000'
  const text = vars.foreground ?? base.foreground ?? '#ffffff'
  const primary = vars.primary ?? base.primary ?? '#000000'
  return {
    background,
    card: `color-mix(in oklch, ${text} 6%, ${background})`,
    primary,
    text,
    accent: `color-mix(in oklch, ${primary} 25%, ${background})`,
  }
}

/** A palette in miniature: a window with a bar, two cards and a button. */
function PaletteThumb({ swatch }: { swatch: Swatch }) {
  return (
    <div
      aria-hidden
      className="h-20 w-full overflow-hidden rounded-lg border border-border/60"
      style={{ background: swatch.background }}
    >
      <div className="flex h-3 items-center gap-1 px-2 pt-1.5">
        <span className="h-1.5 w-6 rounded-full" style={{ background: swatch.primary }} />
        <span className="h-1.5 w-3 rounded-full" style={{ background: swatch.accent }} />
      </div>
      <div className="mt-2 grid grid-cols-2 gap-1.5 px-2">
        {[0, 1].map((i) => (
          <div
            key={i}
            className="grid gap-1 rounded-md p-1.5"
            style={{ background: swatch.card, boxShadow: `0 0 0 1px ${swatch.text}1f` }}
          >
            <span className="h-1 w-8 rounded-full opacity-60" style={{ background: swatch.text }} />
            <span className="h-1 w-5 rounded-full" style={{ background: swatch.accent }} />
          </div>
        ))}
      </div>
      <div className="mt-1.5 flex justify-end px-2">
        <span className="h-2.5 w-8 rounded-full" style={{ background: swatch.primary }} />
      </div>
    </div>
  )
}

function Section({
  title,
  hint,
  children,
}: {
  title: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <Card>
      <CardHeader className="grid gap-1">
        <CardTitle>{title}</CardTitle>
        {hint && <p className="text-xs font-normal text-muted-foreground">{hint}</p>}
      </CardHeader>
      <CardContent className="grid gap-4">{children}</CardContent>
    </Card>
  )
}

/** The three modes as one inset strip: pressed-in cells, light and dark in
 *  turn, the chosen one lit. */
function ModeStrip({ draft, setDraft }: { draft: AppearanceDraft; setDraft: Setter }) {
  const { t } = useTranslation()
  // How many times a mode was picked here: a pick plays its small show (the
  // sun swells and turns once, the moon fills itself in), never on load.
  const [picks, setPicks] = useState(0)
  return (
    <div
      role="radiogroup"
      aria-label={t('settings.mode')}
      className="inline-flex overflow-hidden rounded-lg border border-border bg-black/30 shadow-[inset_0_2px_5px_rgb(0_0_0/0.45)]"
    >
      {MODES.map((mode, i) => {
        const Icon = MODE_ICONS[mode]
        const selected = draft.themeMode === mode
        return (
          <button
            key={mode}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => {
              if (!selected) setPicks((n) => n + 1)
              setDraft({ ...draft, themeMode: mode })
            }}
            className={cn(
              'flex h-9 items-center gap-1.5 px-4 text-sm outline-none transition-colors',
              'focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring',
              i > 0 && 'border-l border-border/70',
              selected
                ? 'bg-primary font-medium text-primary-foreground shadow-[inset_0_2px_4px_rgb(0_0_0/0.3)]'
                : cn(
                    'text-muted-foreground hover:text-foreground',
                    // Dark, light, dark: the cells read as separate slots.
                    i % 2 === 0 ? 'bg-black/20 hover:bg-black/10' : 'bg-white/[0.06] hover:bg-white/10',
                  ),
            )}
          >
            <Icon
              // A new key restarts the sun's turn on every pick.
              key={mode === 'light' && selected ? `sun-${picks}` : mode}
              className={cn(
                'size-4',
                mode === 'light' &&
                  selected &&
                  picks > 0 &&
                  'motion-safe:animate-[mokora-sun-pop_0.65s_ease-in-out]',
                mode === 'dark' && 'transition-[fill] duration-500 ease-out',
              )}
              style={mode === 'dark' ? { fill: selected ? 'currentColor' : 'transparent' } : undefined}
            />
            {t(`settings.mode_${mode}`)}
          </button>
        )
      })}
    </div>
  )
}

export function AppearanceSection({
  draft,
  setDraft,
  ordering = false,
  onStartOrdering,
  onStopOrdering,
  custom,
}: {
  draft: AppearanceDraft
  setDraft: Setter
  /** The sidebar's order is being set; see the sidebar. */
  ordering?: boolean
  onStartOrdering?: () => void
  onStopOrdering?: () => void
  /** A new size button being made: its field is out while `open`. */
  custom?: {
    open: boolean
    text: string
    /** What is typed can become a button. */
    valid: boolean
    onOpen: () => void
    onType: (text: string) => void
  }
}) {
  const { t } = useTranslation()
  const pct = Math.round(draft.uiScale * 100)

  // A preset glides to its size instead of jumping, so the page and the
  // liquid move together. The latest draft is read from a ref, as each frame
  // changes it.
  const latest = useRef(draft)
  useEffect(() => {
    latest.current = draft
  })
  const commitScale = (uiScale: number) => setDraft({ ...latest.current, uiScale })
  const customField = useRef<HTMLInputElement>(null)
  const customOpen = custom?.open ?? false
  useEffect(() => {
    if (customOpen) customField.current?.focus()
  }, [customOpen])
  const typed = custom?.text ?? ''
  const typedInvalid = typed !== '' && !custom?.valid
  const glide = useRef(0)
  useEffect(() => () => cancelAnimationFrame(glide.current), [])
  const stopGlide = () => cancelAnimationFrame(glide.current)
  const glideTo = (target: number) => {
    stopGlide()
    const from = latest.current.uiScale
    const reduced =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (from === target || reduced || typeof requestAnimationFrame !== 'function') {
      commitScale(target)
      return
    }
    let started: number | null = null
    const tick = (now: number) => {
      started ??= now
      const t = Math.min(1, (now - started) / GLIDE_MS)
      const eased = 1 - Math.pow(1 - t, 3)
      const uiScale = t === 1 ? target : Math.round((from + (target - from) * eased) * 1000) / 1000
      commitScale(uiScale)
      if (t < 1) glide.current = requestAnimationFrame(tick)
    }
    glide.current = requestAnimationFrame(tick)
  }
  const isDefault =
    draft.themeMode === THEME_MODE_DEFAULT &&
    draft.themePalette === THEME_PALETTE_DEFAULT &&
    draft.uiScale === SCALE_DEFAULT &&
    draft.scalePresets.length === 0 &&
    draft.sidebarHoverOpen &&
    !draft.sidebarDisabled &&
    !draft.zoomShortcut

  const resetAll = () =>
    setDraft({
      ...draft,
      themeMode: THEME_MODE_DEFAULT,
      themePalette: THEME_PALETTE_DEFAULT,
      customTheme: { light: { ...DEFAULT_CUSTOM_LIGHT }, dark: { ...DEFAULT_CUSTOM_DARK } },
      uiScale: SCALE_DEFAULT,
      scalePresets: [],
      sidebarHoverOpen: true,
      sidebarDisabled: false,
      zoomShortcut: false,
    })

  return (
    <div className="grid gap-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="grid gap-1">
          <h1 className="text-2xl font-semibold">{t('settings.appearance')}</h1>
          <p className="text-sm text-muted-foreground">{t('settings.look.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            className={cn(draft.sharedAppearance && 'border-primary text-primary ring-1 ring-primary/50')}
            aria-pressed={draft.sharedAppearance}
            title={t('settings.look.shared')}
            onClick={() => setDraft({ ...draft, sharedAppearance: !draft.sharedAppearance })}
          >
            <Users className="size-3.5" />
            {t('settings.look.shared')}
          </Button>
          <Button variant="outline" size="sm" onClick={resetAll} disabled={isDefault}>
            <RotateCcw className="size-3.5" />
            {t('settings.look.reset_all')}
          </Button>
        </div>
      </div>

      <Card>
        <CardContent className="flex items-center justify-between gap-4 py-0">
          <span className="text-base font-semibold leading-none">{t('settings.mode')}</span>
          <ModeStrip draft={draft} setDraft={setDraft} />
        </CardContent>
      </Card>

      <Section title={t('settings.theme')} hint={t('settings.look.palette_hint')}>
        <div
          role="radiogroup"
          aria-label={t('settings.theme')}
          className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5"
        >
          {THEME_PALETTES.map((palette) => {
            const selected = draft.themePalette === palette
            return (
              <button
                key={palette}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setDraft({ ...draft, themePalette: palette })}
                className={cn(
                  'group relative grid gap-2 rounded-xl border p-2 text-left outline-none transition',
                  'hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring',
                  selected ? 'border-primary ring-1 ring-primary' : 'border-border',
                )}
              >
                <PaletteThumb swatch={swatchOf(draft, palette)} />
                <span className="flex items-center gap-1.5 px-1 pb-0.5 text-sm font-medium">
                  {palette === 'custom' && <Paintbrush className="size-3.5" />}
                  {t(`settings.theme_${palette}`)}
                </span>
                {selected && (
                  <span className="absolute right-3.5 top-3.5 grid size-5 place-items-center rounded-full bg-primary text-primary-foreground">
                    <Check className="size-3" />
                  </span>
                )}
              </button>
            )
          })}
        </div>
        {draft.themePalette === 'custom' && <CustomEditor draft={draft} setDraft={setDraft} />}
      </Section>

      <Section title={t('settings.ui_scale')}>
        <LiquidSlider
          value={draft.uiScale}
          min={SCALE_MIN}
          max={SCALE_MAX}
          step={SCALE_STEP}
          onChange={(uiScale) => {
            stopGlide()
            commitScale(uiScale)
          }}
          label={t('settings.ui_scale')}
          display={`${pct}%`}
        />
        <div className="flex flex-wrap items-center gap-2">
          {[...BUILTIN_SCALE_PRESETS, ...draft.scalePresets].map((percent) => {
            const added = !BUILTIN_SCALE_PRESETS.includes(percent)
            const selected = Math.round(draft.uiScale * 100) === percent
            return (
              <span
                key={percent}
                className={cn(
                  'group/preset relative inline-flex',
                  added && 'animate-in fade-in zoom-in-90 duration-300',
                )}
              >
                <Button
                  variant={selected ? 'default' : 'outline'}
                  size="sm"
                  onClick={() => glideTo(percent / 100)}
                  aria-pressed={selected}
                >
                  {percent}%
                </Button>
                {added && (
                  <button
                    type="button"
                    aria-label={t('settings.look.preset_remove', { percent })}
                    onClick={() =>
                      setDraft({
                        ...draft,
                        scalePresets: draft.scalePresets.filter((p) => p !== percent),
                      })
                    }
                    className={cn(
                      'absolute -right-1.5 -top-1.5 grid size-4 place-items-center rounded-full',
                      'bg-destructive text-white opacity-0 outline-none transition-opacity',
                      'group-hover/preset:opacity-100 focus-visible:opacity-100',
                    )}
                  >
                    <X className="size-3" />
                  </button>
                )}
              </span>
            )
          })}
          {custom && (
            <>
              {/* The new button slides out of the plus, and back into it. */}
              <div
                aria-hidden={!custom.open}
                className={cn(
                  'overflow-hidden transition-[max-width,opacity,margin] duration-300 ease-out',
                  custom.open ? 'max-w-[12rem] opacity-100' : '-mr-2 max-w-0 opacity-0',
                )}
              >
                <label
                  className={cn(
                    'flex h-8 items-center gap-1 rounded-md border bg-black/20 px-2 text-sm',
                    'focus-within:ring-2 focus-within:ring-ring',
                    typedInvalid ? 'border-destructive' : 'border-border',
                  )}
                >
                  <input
                    ref={customField}
                    type="number"
                    inputMode="numeric"
                    min={Math.round(SCALE_MIN * 100)}
                    max={Math.round(SCALE_MAX * 100)}
                    step={1}
                    tabIndex={custom.open ? 0 : -1}
                    value={custom.text}
                    aria-label={t('settings.look.custom_scale')}
                    aria-invalid={typedInvalid}
                    placeholder={t('settings.look.custom_scale_hold', {
                      min: Math.round(SCALE_MIN * 100),
                      max: Math.round(SCALE_MAX * 100),
                    })}
                    onChange={(e) => custom.onType(e.target.value)}
                    className="w-24 bg-transparent font-mono tabular-nums outline-none placeholder:font-sans placeholder:text-muted-foreground/70"
                  />
                  <span className="text-muted-foreground">%</span>
                </label>
              </div>
              <Button
                variant="outline"
                size="icon"
                className="size-8"
                aria-label={t('settings.look.custom_scale_add')}
                title={
                  draft.scalePresets.length >= MAX_SCALE_PRESETS
                    ? t('settings.look.custom_scale_full', { max: MAX_SCALE_PRESETS })
                    : undefined
                }
                disabled={custom.open || draft.scalePresets.length >= MAX_SCALE_PRESETS}
                onClick={custom.onOpen}
              >
                <Plus className="size-4" />
              </Button>
            </>
          )}
        </div>
      </Section>

      <Section title={t('settings.look.sidebar')} hint={t('settings.appearance_sidebar_hint')}>
        <Toggle
          label={t('settings.appearance_sidebar_peek')}
          value={draft.sidebarHoverOpen}
          onChange={(sidebarHoverOpen) => setDraft({ ...draft, sidebarHoverOpen })}
        />
        <Toggle
          label={t('settings.appearance_hide_sidebar')}
          value={draft.sidebarDisabled}
          onChange={(sidebarDisabled) => setDraft({ ...draft, sidebarDisabled })}
        />
        <Toggle
          label={t('settings.look.zoom_shortcut')}
          hint={t('settings.look.zoom_shortcut_hint')}
          value={draft.zoomShortcut}
          onChange={(zoomShortcut) => setDraft({ ...draft, zoomShortcut })}
        />
        <div className="grid gap-1.5">
          <div className="flex items-center justify-between gap-3">
            <Label>{t('settings.look.menu_order')}</Label>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                aria-pressed={ordering}
                disabled={ordering}
                className={cn(ordering && 'border-primary text-primary ring-1 ring-primary/50')}
                onClick={onStartOrdering}
              >
                <ListOrdered className="size-3.5" />
                {t('settings.look.menu_order_set')}
              </Button>
              {ordering && (
                <Button variant="ghost" size="sm" onClick={onStopOrdering}>
                  {t('common.cancel')}
                </Button>
              )}
            </div>
          </div>
          <span className="text-xs text-muted-foreground">{t('settings.look.menu_order_hint')}</span>
        </div>
      </Section>
    </div>
  )
}
