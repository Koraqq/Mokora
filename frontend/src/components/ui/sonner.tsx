import { Toaster as SonnerToaster, toast as sonnerToast } from 'sonner'

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

function notify(severity: ToastSeverity, message: string, opts: NotifyOpts = {}) {
  return sonnerToast(message, {
    ...opts,
    className: SEVERITY_BORDER[severity],
  })
}

export const toast = {
  info: (msg: string, opts?: NotifyOpts) => notify('info', msg, opts),
  success: (msg: string, opts?: NotifyOpts) => notify('success', msg, opts),
  warning: (msg: string, opts?: NotifyOpts) => notify('warning', msg, opts),
  error: (msg: string, opts?: NotifyOpts) => notify('error', msg, opts),
  dismiss: sonnerToast.dismiss,
}
