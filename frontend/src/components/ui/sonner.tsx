import { Toaster as SonnerToaster, toast as sonnerToast } from 'sonner'

import type { Notification } from '@/types'
import { HAS_TAURI, invoke } from '@/lib/tauri'

export type ToastSeverity = 'info' | 'success' | 'warning' | 'error'

// Per-toast border color. `!` overrides the universal `* { @apply border-border }`
// rule from index.css (which would otherwise paint border-left-color gray).
const SEVERITY_BORDER: Record<ToastSeverity, string> = {
  info: '!border-l-sky-400',
  success: '!border-l-emerald-400',
  warning: '!border-l-orange-500',
  error: '!border-l-red-500',
}

const DEFAULT_DURATION_MS = 5000

/** `compact` keeps toasts inside the play screen's right-hand column: the game
 *  is a web view that paints above the page, so a wider toast would be cut off
 *  where it reaches over the game. */
export function Toaster({ compact = false }: { compact?: boolean }) {
  return (
    <SonnerToaster
      theme="dark"
      position="bottom-right"
      offset={compact ? 12 : undefined}
      style={compact ? ({ '--width': '19.5rem' } as React.CSSProperties) : undefined}
      richColors={false}
      closeButton
      duration={DEFAULT_DURATION_MS}
      toastOptions={{
        classNames: {
          toast:
            'group !bg-popover !text-popover-foreground !ring-1 !ring-foreground/10 !rounded-lg !shadow-lg !border-l-4 !pl-4',
          title: 'text-sm font-medium',
          description: 'text-xs text-muted-foreground',
        },
      }}
    />
  )
}

type NotifyOpts = {
  description?: string
  duration?: number
  id?: string | number
}

const LEVEL_BY_SEVERITY: Record<ToastSeverity, Notification['level']> = {
  info: 'info',
  success: 'success',
  warning: 'warn',
  error: 'error',
}

const remotelyDismissed = new Set<string>()
let nextLocalId = 1

function idFor(opts: NotifyOpts): string {
  return opts.id === undefined
    ? `ui-notify-${Date.now()}-${nextLocalId++}`
    : String(opts.id)
}

/** Render an already-broadcast notification in this webview only. */
export function showToast(
  severity: ToastSeverity,
  message: string,
  opts: NotifyOpts = {},
) {
  const id = idFor(opts)
  return sonnerToast(message, {
    ...opts,
    id,
    className: SEVERITY_BORDER[severity],
    onDismiss: () => syncToastDismissal(id),
  })
}

/** Broadcast a close-button/swipe dismissal unless it came from another UI. */
export function syncToastDismissal(id: string) {
  if (remotelyDismissed.delete(id) || !HAS_TAURI) return
  void invoke('dismiss_notification', { id })
}

/** Close a toast in this webview without re-broadcasting the close event. */
export function dismissToastLocally(id: string) {
  remotelyDismissed.add(id)
  sonnerToast.dismiss(id)
}

function notify(severity: ToastSeverity, message: string, opts: NotifyOpts = {}) {
  const id = idFor(opts)
  if (!HAS_TAURI) return showToast(severity, message, { ...opts, id })

  const notification: Notification = {
    level: LEVEL_BY_SEVERITY[severity],
    title: message,
    body: opts.description,
    sticky: opts.duration === Infinity,
    id,
  }
  void invoke('publish_notification', { notification }).catch(() => {
    showToast(severity, message, { ...opts, id })
  })
  return id
}

function dismiss(id?: string | number) {
  if (id === undefined) return sonnerToast.dismiss()
  const normalized = String(id)
  if (!HAS_TAURI) return sonnerToast.dismiss(normalized)
  void invoke('dismiss_notification', { id: normalized }).catch(() => {
    sonnerToast.dismiss(normalized)
  })
  return normalized
}

export const toast = {
  info: (msg: string, opts?: NotifyOpts) => notify('info', msg, opts),
  success: (msg: string, opts?: NotifyOpts) => notify('success', msg, opts),
  warning: (msg: string, opts?: NotifyOpts) => notify('warning', msg, opts),
  error: (msg: string, opts?: NotifyOpts) => notify('error', msg, opts),
  dismiss,
}
