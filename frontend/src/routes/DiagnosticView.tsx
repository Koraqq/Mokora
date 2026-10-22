import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, Search, X } from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useLogsStore } from '@/stores/logsStore'
import { useLogStream } from '@/hooks/useLogStream'
import type { LogEntry } from '@/types'

const LEVEL_ORDER = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'] as const

const LEVEL_BADGE: Record<string, string> = {
  ERROR: 'bg-red-500/15 text-red-600 border-red-500/30 dark:text-red-300',
  WARN: 'bg-amber-500/15 text-amber-600 border-amber-500/30 dark:text-amber-300',
  INFO: 'bg-sky-500/15 text-sky-600 border-sky-500/30 dark:text-sky-300',
  DEBUG: 'bg-slate-500/15 text-slate-600 border-slate-500/30 dark:text-slate-300',
  TRACE: 'bg-violet-500/15 text-violet-600 border-violet-500/30 dark:text-violet-300',
}

function formatTime(ms: number): string {
  const d = new Date(ms)
  const pad = (n: number, w = 2) => n.toString().padStart(w, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`
}

type DiagnosticViewProps = {
  /** Live tail enabled — true only when this tab is visible AND the
   *  user is on the active session AND tail isn't paused. */
  liveEnabled: boolean
  autoScroll: boolean
  onUserScrolledAway: () => void
}

export function DiagnosticView({ liveEnabled, autoScroll, onUserScrolledAway }: DiagnosticViewProps) {
  const { t } = useTranslation()

  const filter = useLogsStore((s) => s.filter)
  const toggleLevel = useLogsStore((s) => s.toggleLevel)
  const toggleTarget = useLogsStore((s) => s.toggleTarget)
  const setFilter = useLogsStore((s) => s.setFilter)
  const entries = useLogsStore((s) => s.entries)

  const [selectedIdx, setSelectedIdx] = useState<number | null>(null)
  const listRef = useRef<HTMLDivElement>(null)

  // Live tail subscription. Hook gates on liveEnabled.
  useLogStream(liveEnabled)

  const observedTargets = useMemo(() => {
    const set = new Set<string>()
    for (const e of entries) set.add(e.target)
    return Array.from(set).sort()
  }, [entries])

  const filtered = useMemo(() => {
    const search = filter.search.trim().toLowerCase()
    const targetsActive = filter.targets.size > 0
    return entries.filter((e) => {
      if (!filter.showTrace && e.level === 'TRACE') return false
      if (filter.levels.size > 0 && !filter.levels.has(e.level)) return false
      if (targetsActive && !filter.targets.has(e.target)) return false
      if (search.length > 0 && !e.message.toLowerCase().includes(search)) return false
      return true
    })
  }, [entries, filter])

  useEffect(() => {
    if (!autoScroll) return
    const el = listRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
  }, [filtered.length, autoScroll])

  const selectedEntry: LogEntry | null =
    selectedIdx != null && selectedIdx < filtered.length ? filtered[selectedIdx] : null

  return (
    <div className="flex flex-col gap-3 flex-1 min-h-0">
      <Card>
        <CardContent className="flex items-center gap-3 flex-wrap py-3">
          <div className="flex items-center gap-1.5">
            <span className="text-sm text-muted-foreground whitespace-nowrap">
              {t('logs.level')}
            </span>
            {LEVEL_ORDER.map((lv) => {
              if (lv === 'TRACE' && !filter.showTrace) return null
              const active = filter.levels.has(lv)
              return (
                <button
                  key={lv}
                  type="button"
                  onClick={() => toggleLevel(lv)}
                  className={`px-2 py-0.5 text-xs font-mono rounded border transition-colors cursor-pointer ${
                    active ? LEVEL_BADGE[lv] : 'border-muted text-muted-foreground hover:bg-muted/40'
                  }`}
                >
                  {lv}
                </button>
              )
            })}
            <label className="flex items-center gap-1 text-xs text-muted-foreground ml-1 select-none">
              <input
                type="checkbox"
                checked={filter.showTrace}
                onChange={(e) => setFilter({ showTrace: e.target.checked })}
                className="h-3.5 w-3.5 cursor-pointer"
              />
              {t('logs.show_trace')}
            </label>
          </div>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm" className="gap-1.5">
                {t('logs.module')}
                {filter.targets.size > 0 && (
                  <Badge variant="secondary" className="ml-1 h-5 px-1.5">
                    {filter.targets.size}
                  </Badge>
                )}
                <ChevronDown className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="max-h-[60vh] overflow-y-auto">
              <DropdownMenuLabel>{t('logs.module_filter_label')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              {observedTargets.length === 0 && (
                <div className="px-2 py-1.5 text-xs text-muted-foreground">
                  {t('logs.no_modules')}
                </div>
              )}
              {observedTargets.map((tgt) => (
                <DropdownMenuCheckboxItem
                  key={tgt}
                  checked={filter.targets.has(tgt)}
                  onCheckedChange={() => toggleTarget(tgt)}
                  onSelect={(e) => e.preventDefault()}
                >
                  <span className="font-mono text-xs">{tgt}</span>
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>

          <div className="flex items-center gap-1.5 flex-1 min-w-[200px]">
            <Search className="h-4 w-4 text-muted-foreground shrink-0" />
            <Input
              type="search"
              value={filter.search}
              onChange={(e) => setFilter({ search: e.target.value })}
              placeholder={t('logs.search_placeholder')}
              className="h-8"
            />
          </div>

          <div className="text-xs text-muted-foreground whitespace-nowrap">
            {t('logs.entry_count', { shown: filtered.length, total: entries.length })}
          </div>
        </CardContent>
      </Card>

      <div className="flex flex-1 min-h-0 gap-3">
        <Card className="flex-1 min-h-0 flex flex-col overflow-hidden">
          <div
            ref={listRef}
            className="flex-1 overflow-y-auto font-mono text-xs"
            onWheel={() => {
              const el = listRef.current
              if (!el || !autoScroll) return
              const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 8
              if (!atBottom) onUserScrolledAway()
            }}
          >
            {filtered.length === 0 ? (
              <div className="p-8 text-center text-muted-foreground text-sm">
                {entries.length === 0 ? t('logs.empty') : t('logs.empty_filtered')}
              </div>
            ) : (
              filtered.map((e, i) => {
                const isSelected = i === selectedIdx
                return (
                  <div
                    key={i}
                    onClick={() => setSelectedIdx(i === selectedIdx ? null : i)}
                    className={`flex items-baseline gap-2 px-3 py-1 border-b border-border/40 cursor-pointer hover:bg-muted/40 ${
                      isSelected ? 'bg-muted/60' : ''
                    }`}
                  >
                    <span className="text-muted-foreground shrink-0 tabular-nums">
                      {formatTime(e.ts_ms)}
                    </span>
                    <span
                      className={`shrink-0 px-1.5 py-0.5 rounded border text-[10px] leading-none ${
                        LEVEL_BADGE[e.level] ?? 'border-muted text-muted-foreground'
                      }`}
                    >
                      {e.level}
                    </span>
                    <span className="text-muted-foreground shrink-0 truncate max-w-[260px]">
                      {e.target}
                    </span>
                    <span className="flex-1 break-all whitespace-pre-wrap">
                      {e.message || <span className="text-muted-foreground">—</span>}
                    </span>
                  </div>
                )
              })
            )}
          </div>
        </Card>

        {selectedEntry && (
          <Card className="w-[420px] shrink-0 flex flex-col overflow-hidden">
            <div className="flex items-center justify-between px-4 py-2 border-b">
              <span className="text-sm font-semibold">{t('logs.detail_title')}</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setSelectedIdx(null)}
                className="h-6 w-6 p-0"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
            <CardContent className="flex-1 overflow-y-auto py-3 space-y-3 text-xs">
              <DetailRow label={t('logs.detail_time')} value={new Date(selectedEntry.ts_ms).toISOString()} />
              <DetailRow label={t('logs.detail_level')} value={selectedEntry.level} />
              <DetailRow label={t('logs.detail_target')} value={selectedEntry.target} mono />
              {selectedEntry.file && (
                <DetailRow
                  label={t('logs.detail_location')}
                  value={`${selectedEntry.file}${selectedEntry.line ? `:${selectedEntry.line}` : ''}`}
                  mono
                />
              )}
              <div>
                <div className="text-muted-foreground mb-1">{t('logs.detail_message')}</div>
                <div className="font-mono whitespace-pre-wrap break-all bg-muted/40 rounded p-2">
                  {selectedEntry.message}
                </div>
              </div>
              {selectedEntry.fields && Object.keys(selectedEntry.fields).length > 0 && (
                <div>
                  <div className="text-muted-foreground mb-1">{t('logs.detail_fields')}</div>
                  <pre className="font-mono whitespace-pre-wrap break-all bg-muted/40 rounded p-2">
                    {JSON.stringify(selectedEntry.fields, null, 2)}
                  </pre>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  )
}

function DetailRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <div className="text-muted-foreground mb-0.5">{label}</div>
      <div className={mono ? 'font-mono break-all' : 'break-all'}>{value}</div>
    </div>
  )
}
