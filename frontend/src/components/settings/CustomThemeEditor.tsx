import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { ColorField } from '@/components/settings/ColorField'
import type { AppearanceDraft } from '@/components/settings/draft'
import {
  CUSTOM_BASE_VARS,
  DEFAULT_CUSTOM_DARK,
  DEFAULT_CUSTOM_LIGHT,
  loadTheme,
  type CustomBaseVar,
} from '@/stores/themeStore'

// The color field takes opaque hex. Use the browser's CSS
// engine to canonicalise any input value (oklch/hsl/rgb/named) to rgb, then
// emit hex — no extra dependency.
function toHex(value: string | undefined): string {
  if (!value) return '#000000'
  const trimmed = value.trim()
  if (/^#[0-9a-f]{6}$/i.test(trimmed)) return trimmed.toLowerCase()
  if (typeof document === 'undefined') return '#000000'
  const probe = document.createElement('div')
  probe.style.color = trimmed
  document.body.appendChild(probe)
  const computed = getComputedStyle(probe).color
  probe.remove()
  const m = computed.match(/^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i)
  if (!m) return '#000000'
  const hex = [m[1], m[2], m[3]]
    .map((n) => parseInt(n, 10).toString(16).padStart(2, '0'))
    .join('')
  return `#${hex}`
}

export function CustomEditor({
  draft,
  setDraft,
}: {
  draft: AppearanceDraft
  setDraft: (draft: AppearanceDraft) => void
}) {
  const { t } = useTranslation()
  const [importInput, setImportInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [importError, setImportError] = useState<string | null>(null)
  const [importSuccess, setImportSuccess] = useState(false)

  const handleImport = async () => {
    if (!importInput.trim()) return
    setImporting(true)
    setImportError(null)
    setImportSuccess(false)
    try {
      const imported = await loadTheme(importInput)
      setDraft({
        ...draft,
        customTheme: {
          light: { ...draft.customTheme.light, ...imported.light },
          dark: { ...draft.customTheme.dark, ...imported.dark },
        },
      })
      setImportInput('')
      setImportSuccess(true)
    } catch (e) {
      setImportError(e instanceof Error ? e.message : String(e))
    } finally {
      setImporting(false)
    }
  }

  return (
    <div className="grid gap-4 rounded-md border border-border/50 p-3">
      <p className="text-xs text-muted-foreground">{t('settings.custom_hint')}</p>
      <CustomColorRow
        which="light"
        vars={draft.customTheme.light}
        onChange={(which, name, value) =>
          setDraft({
            ...draft,
            customTheme: {
              ...draft.customTheme,
              [which]: { ...draft.customTheme[which], [name]: value },
            },
          })
        }
      />
      <CustomColorRow
        which="dark"
        vars={draft.customTheme.dark}
        onChange={(which, name, value) =>
          setDraft({
            ...draft,
            customTheme: {
              ...draft.customTheme,
              [which]: { ...draft.customTheme[which], [name]: value },
            },
          })
        }
      />
      <div className="grid gap-2 rounded-md border border-border/40 p-2">
        <Label className="text-sm">{t('settings.custom_import')}</Label>
        <span className="text-xs text-muted-foreground">
          {t('settings.custom_import_hint')}
        </span>
        <textarea
          value={importInput}
          onChange={(e) => {
            setImportInput(e.target.value)
            setImportError(null)
            setImportSuccess(false)
          }}
          placeholder="https://tweakcn.com/themes/<id>"
          rows={2}
          className="w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-xs focus:outline-none focus:ring-2 focus:ring-ring resize-y"
          spellCheck={false}
        />
        {importError && (
          <span className="text-xs text-red-500">{importError}</span>
        )}
        {importSuccess && (
          <span className="text-xs text-emerald-600 dark:text-emerald-400">
            {t('settings.custom_import_ok')}
          </span>
        )}
        <div className="flex justify-end">
          <Button
            variant="outline"
            size="sm"
            onClick={handleImport}
            disabled={importing || !importInput.trim()}
          >
            {importing
              ? t('settings.custom_importing')
              : t('settings.custom_import_button')}
          </Button>
        </div>
      </div>
      <div className="flex justify-end">
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            setDraft({
              ...draft,
              customTheme: {
                light: { ...DEFAULT_CUSTOM_LIGHT },
                dark: { ...DEFAULT_CUSTOM_DARK },
              },
            })
          }
        >
          {t('settings.custom_reset')}
        </Button>
      </div>
    </div>
  )
}

function CustomColorRow({
  which,
  vars,
  onChange,
}: {
  which: 'light' | 'dark'
  vars: Partial<Record<CustomBaseVar, string>>
  onChange: (which: 'light' | 'dark', name: CustomBaseVar, value: string) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-2">
      <Label className="text-sm">{t(`settings.custom_${which}`)}</Label>
      <div className="grid grid-cols-5 gap-2">
        {CUSTOM_BASE_VARS.map((name) => (
          <div key={name} className="grid gap-1">
            <span className="text-[10px] text-muted-foreground truncate" title={name}>
              {t(`settings.custom_var_${name}`)}
            </span>
            <ColorField
              label={`${which} ${name}`}
              value={toHex(vars[name])}
              onChange={(hex) => onChange(which, name, hex)}
            />
          </div>
        ))}
      </div>
    </div>
  )
}
