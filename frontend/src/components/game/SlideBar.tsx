// A row of buttons you can press and slide across, like the tab bars of
// recent iPhone apps (Threads): pressing shows a soft highlight under the
// pointer, the highlight glides from button to button as the pointer moves
// and the button under it grows a little; letting go presses that button. A
// plain click still works as before (it is a slide that never left its
// button). Keyboard use is untouched.
//
// While pressed the bar holds the pointer (pointer capture), so the buttons
// see no click of their own: the bar presses the one it ends on. A button
// that opens on pointer-down instead (a Radix menu trigger, marked
// aria-haspopup) is opened with a pointer-down, and not again when the press
// both began and ended on it (it already opened). Everything the slide shows
// is cleared when it ends, however it ends.

import { useRef, type ReactNode } from 'react'

import { cn } from '@/lib/utils'

const ACTIVE = 'data-slide-active'

/** The enabled button of the bar's own row at a point, if any. Panels that
 *  open from a button (bots, membership) hang below the row inside the bar;
 *  their buttons are left alone, so they work as plain buttons. */
function buttonAt(bar: HTMLElement, x: number, y: number): HTMLButtonElement | null {
  const hit = document.elementFromPoint(x, y)?.closest('button')
  if (!hit || !bar.contains(hit) || hit.disabled) return null
  // By the button's centre: the highlighted button is scaled up past the
  // row's edges, but scaling keeps its centre where it was.
  const row = bar.getBoundingClientRect()
  const r = hit.getBoundingClientRect()
  const mid = (r.top + r.bottom) / 2
  return mid >= row.top && mid <= row.bottom ? hit : null
}

/** Press `button` the way a click on it would. */
function press(button: HTMLButtonElement, pressedOnIt: boolean) {
  if (button.getAttribute('aria-haspopup')) {
    // Opens on pointer-down: it did already if the press began on it.
    if (!pressedOnIt) {
      button.dispatchEvent(
        new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerType: 'mouse' }),
      )
    }
    return
  }
  button.click()
}

export function SlideBar({ children, className }: { children: ReactNode; className?: string }) {
  const bar = useRef<HTMLDivElement>(null)
  const glow = useRef<HTMLDivElement>(null)
  const pressed = useRef<{ id: number; start: HTMLButtonElement } | null>(null)
  const over = useRef<HTMLButtonElement | null>(null)

  const highlight = (button: HTMLButtonElement | null) => {
    if (over.current === button) return
    over.current?.removeAttribute(ACTIVE)
    over.current = button
    const g = glow.current
    const b = bar.current
    if (!g || !b) return
    if (!button) {
      g.style.opacity = '0'
      return
    }
    button.setAttribute(ACTIVE, '')
    const r = button.getBoundingClientRect()
    const box = b.getBoundingClientRect()
    g.style.transform = `translateX(${r.left - box.left - 4}px)`
    g.style.width = `${r.width + 8}px`
    g.style.height = `${r.height + 8}px`
    g.style.top = `${r.top - box.top - 4}px`
    g.style.opacity = '1'
  }

  /** End the slide and clear everything it showed. */
  const finish = () => {
    pressed.current = null
    highlight(null)
  }

  return (
    <div
      ref={bar}
      className="relative flex items-center"
      onPointerDown={(e) => {
        if (e.button !== 0 || !bar.current) return
        const start = buttonAt(bar.current, e.clientX, e.clientY)
        if (!start) return
        pressed.current = { id: e.pointerId, start }
        bar.current.setPointerCapture(e.pointerId)
        // The first position appears in place, then later moves glide.
        if (glow.current) glow.current.style.transition = 'opacity 150ms'
        highlight(start)
        requestAnimationFrame(() => {
          if (glow.current) glow.current.style.transition = ''
        })
      }}
      onPointerMove={(e) => {
        if (!pressed.current || e.pointerId !== pressed.current.id || !bar.current) return
        highlight(buttonAt(bar.current, e.clientX, e.clientY))
      }}
      onPointerUp={(e) => {
        const p = pressed.current
        if (!p || e.pointerId !== p.id || !bar.current) return
        const target = buttonAt(bar.current, e.clientX, e.clientY)
        finish()
        if (target) press(target, target === p.start)
      }}
      onPointerCancel={finish}
      onLostPointerCapture={finish}
    >
      <div
        ref={glow}
        aria-hidden
        className="pointer-events-none absolute left-0 rounded-xl bg-foreground/10 opacity-0 ring-1 ring-border transition-[transform,width,opacity] duration-200 ease-[cubic-bezier(0.2,0.8,0.2,1.2)]"
      />
      <div
        className={cn(
          'relative flex items-center',
          // ACTIVE, written out: Tailwind only sees literal class names. The
          // buttons keep their own transitions.
          '[&_button[data-slide-active]]:scale-[1.18] [&_button[data-slide-active]]:text-foreground',
          className,
        )}
      >
        {children}
      </div>
    </div>
  )
}
