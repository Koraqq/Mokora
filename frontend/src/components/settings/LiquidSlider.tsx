import { useEffect, useLayoutEffect, useRef, useState } from 'react'

import { WaterSurface } from '@/lib/waterSurface'
import { cn } from '@/lib/utils'

type Props = {
  value: number
  min: number
  max: number
  /** One press of an arrow key. */
  step: number
  /** What the pointer snaps to while dragging: fine, so the size glides. */
  dragStep?: number
  onChange: (value: number) => void
  label: string
  /** What shows inside the vial, e.g. `110%`. */
  display: string
  className?: string
}

/** How hard a move of the level splashes the surface (percent per second, per
 *  percent moved), and the most one move may. */
const SPLASH_PER_PERCENT = 30
const SPLASH_MAX = 70

function snap(raw: number, min: number, max: number, step: number): number {
  const stepped = Math.round((raw - min) / step) * step + min
  return Math.round(Math.min(max, Math.max(min, stepped)) * 1000) / 1000
}

/**
 * A thick vial of liquid that is the slider: the level is the value. The
 * liquid is simulated, not animated — a row of springs coupled to each other
 * (see `WaterSurface`): moving the level splashes the surface where it is
 * touched, the disturbance runs up and down the edge, rings and dies away,
 * and when the water has settled it is perfectly still and costs nothing.
 */
export function LiquidSlider({
  value,
  min,
  max,
  step,
  dragStep = 0.01,
  onChange,
  label,
  display,
  className,
}: Props) {
  const rootRef = useRef<HTMLDivElement>(null)
  const frontRef = useRef<SVGPathElement>(null)
  const backRef = useRef<SVGPathElement>(null)
  const percent = ((value - min) / (max - min)) * 100
  const [surface] = useState(() => new WaterSurface(percent))
  const last = useRef(percent)
  // Where the pointer touched, top to bottom: the splash lands there.
  const touchAt = useRef(0.5)
  const frame = useRef(0)
  const clock = useRef(0)

  const draw = () => {
    frontRef.current?.setAttribute('d', surface.path())
    // The film of liquid a little ahead of the body.
    backRef.current?.setAttribute('d', surface.path(1.1))
  }

  const run = () => {
    if (frame.current !== 0) return
    clock.current = performance.now()
    const tick = (now: number) => {
      surface.advance((now - clock.current) / 1000)
      clock.current = now
      if (surface.settled) {
        surface.rest()
        draw()
        frame.current = 0
        return
      }
      draw()
      frame.current = requestAnimationFrame(tick)
    }
    frame.current = requestAnimationFrame(tick)
  }

  // The liquid is in place before the first paint.
  useLayoutEffect(() => {
    draw()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const moved = percent - last.current
    last.current = percent
    surface.setLevel(percent)
    const reduced =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (reduced || typeof requestAnimationFrame !== 'function') {
      surface.rest()
      draw()
      return
    }
    if (moved !== 0) {
      const push = Math.max(-SPLASH_MAX, Math.min(SPLASH_MAX, moved * SPLASH_PER_PERCENT))
      surface.splash(touchAt.current, push)
    }
    run()
    // `draw` and `run` only touch refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [percent])

  useEffect(
    () => () => {
      if (frame.current !== 0) cancelAnimationFrame(frame.current)
      frame.current = 0
    },
    [],
  )

  const setFromPointer = (clientX: number, clientY: number) => {
    const rect = rootRef.current?.getBoundingClientRect()
    if (!rect || rect.width === 0) return
    if (rect.height > 0) touchAt.current = Math.min(1, Math.max(0, (clientY - rect.top) / rect.height))
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
    const next = snap(min + ratio * (max - min), min, max, dragStep)
    if (next !== value) onChange(next)
  }

  return (
    <div
      ref={rootRef}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={display}
      className={cn(
        'relative h-8 w-full cursor-ew-resize touch-none select-none overflow-hidden rounded-lg',
        'border border-border bg-black/25 shadow-[inset_0_2px_6px_rgb(0_0_0/0.35)] outline-none',
        'focus-visible:ring-2 focus-visible:ring-ring',
        className,
      )}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId)
        setFromPointer(e.clientX, e.clientY)
      }}
      onPointerMove={(e) => {
        if (e.currentTarget.hasPointerCapture(e.pointerId)) setFromPointer(e.clientX, e.clientY)
      }}
      onKeyDown={(e) => {
        const delta =
          e.key === 'ArrowRight' || e.key === 'ArrowUp'
            ? step
            : e.key === 'ArrowLeft' || e.key === 'ArrowDown'
              ? -step
              : 0
        if (delta !== 0) {
          e.preventDefault()
          onChange(snap(value + delta, min, max, dragStep))
        } else if (e.key === 'Home') {
          e.preventDefault()
          onChange(min)
        } else if (e.key === 'End') {
          e.preventDefault()
          onChange(max)
        }
      }}
    >
      <svg
        aria-hidden
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        className="pointer-events-none absolute inset-0 h-full w-full"
      >
        <path ref={backRef} fill="var(--primary)" opacity="0.35" />
        <path ref={frontRef} fill="var(--primary)" opacity="0.88" />
      </svg>
      {/* Marks every 10%, like a measuring glass. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-y-0 left-0 right-0 flex justify-between px-[10%]"
      >
        {Array.from({ length: 9 }, (_, i) => (
          <span key={i} className="mt-auto h-2 w-px bg-foreground/25" />
        ))}
      </div>
      <span className="pointer-events-none absolute inset-y-0 left-4 flex items-center font-mono text-base font-semibold tabular-nums text-foreground drop-shadow-[0_1px_2px_rgb(0_0_0/0.5)]">
        {display}
      </span>
    </div>
  )
}
