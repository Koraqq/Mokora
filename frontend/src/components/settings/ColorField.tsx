import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react'
import { createPortal } from 'react-dom'
import { Pipette } from 'lucide-react'

import {
  colorOfElement,
  hexToHsv,
  hexToRgb,
  hsvToHex,
  parseHex,
  rgbToHex,
  type Hsv,
} from '@/lib/color'

const POPOVER_W = 336
const POPOVER_H = 196

// The pen the pointer turns into while picking; the tip (hotspot) is at the bottom left.
const PEN_PATHS =
  "<path d='m2 22 1-1h3l9-9'/><path d='M3 21v-3l9-9'/><path d='m15 6 3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4Z'/>"
const PEN_SVG = (stroke: string, width: number) =>
  `<g fill='none' stroke='${stroke}' stroke-width='${width}' stroke-linecap='round' stroke-linejoin='round'>${PEN_PATHS}</g>`
const PEN_CURSOR = `url("data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'>${PEN_SVG('#000', 4)}${PEN_SVG('#fff', 2)}</svg>`,
)}") 2 22, crosshair`

const fraction = (value: number, from: number, size: number) =>
  Math.min(1, Math.max(0, (value - from) / size))

/** Pointer handlers that report where on the element (0-1 across, 0-1 down) it is pressed or dragged. */
function dragOn(onMove: (x: number, y: number) => void) {
  const report = (e: ReactPointerEvent<HTMLElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    onMove(fraction(e.clientX, box.left, box.width), fraction(e.clientY, box.top, box.height))
  }
  return {
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
      e.currentTarget.setPointerCapture?.(e.pointerId)
      report(e)
    },
    onPointerMove: (e: ReactPointerEvent<HTMLElement>) => {
      if (e.buttons & 1) report(e)
    },
  }
}

const hueColor = (h: number) => hsvToHex({ h, s: 1, v: 1 })

const field =
  'h-7 w-full rounded border border-white/10 bg-[#0b0e0f] px-2 text-right text-xs tabular-nums text-[#e7ecee] outline-none focus:border-white/40'

export function ColorField({
  value,
  onChange,
  label,
}: {
  value: string
  onChange: (hex: string) => void
  label: string
}) {
  const [open, setOpen] = useState<{ left: number; top: number } | null>(null)
  const [picking, setPicking] = useState(false)
  const button = useRef<HTMLButtonElement>(null)
  const popover = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const outside = (e: PointerEvent) => {
      if (picking) return
      const target = e.target as Node
      if (popover.current?.contains(target) || button.current?.contains(target)) return
      setOpen(null)
    }
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      if (picking) setPicking(false)
      else setOpen(null)
    }
    const scrolled = (e: Event) => {
      if (!picking && !popover.current?.contains(e.target as Node)) setOpen(null)
    }
    document.addEventListener('pointerdown', outside, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('scroll', scrolled, true)
    return () => {
      document.removeEventListener('pointerdown', outside, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('scroll', scrolled, true)
    }
  }, [open, picking])

  const toggle = () => {
    if (open) return setOpen(null)
    const box = button.current?.getBoundingClientRect()
    if (!box) return
    const below = box.bottom + 6 + POPOVER_H <= window.innerHeight
    setOpen({
      left: Math.max(8, Math.min(box.left, window.innerWidth - POPOVER_W - 8)),
      top: below ? box.bottom + 6 : Math.max(8, box.top - 6 - POPOVER_H),
    })
  }

  return (
    <>
      <button
        ref={button}
        type="button"
        aria-label={label}
        aria-expanded={!!open}
        onClick={toggle}
        className="h-9 w-full cursor-pointer rounded border border-border"
        style={{ background: value }}
      />
      {open &&
        createPortal(
          <div
            ref={popover}
            role="dialog"
            aria-label={label}
            style={{ left: open.left, top: open.top, width: POPOVER_W }}
            className="fixed z-[9999] rounded-lg border border-white/10 bg-[#14181a] p-3 text-[#e7ecee] shadow-2xl"
          >
            <Picker
              value={value}
              onChange={onChange}
              picking={picking}
              setPicking={setPicking}
            />
          </div>,
          document.body,
        )}
    </>
  )
}

function Picker({
  value,
  onChange,
  picking,
  setPicking,
}: {
  value: string
  onChange: (hex: string) => void
  picking: boolean
  setPicking: (picking: boolean) => void
}) {
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value))
  const [seen, setSeen] = useState(value)
  const [hexText, setHexText] = useState<string | null>(null)
  const area = useRef<HTMLDivElement>(null)
  const strip = useRef<HTMLDivElement>(null)

  // A color that came from elsewhere (the pen, a reset, an import) moves the handles.
  if (value !== seen) {
    setSeen(value)
    if (hsvToHex(hsv) !== value) setHsv(hexToHsv(value, hsv.h))
  }

  const rgb = hexToRgb(value)
  const emit = (next: Hsv) => {
    setHsv(next)
    onChange(hsvToHex(next))
  }
  const emitHex = (hex: string) => {
    setHsv(hexToHsv(hex, hsv.h))
    onChange(hex)
  }
  const setChannel = (channel: 'r' | 'g' | 'b', text: string) => {
    const n = Math.min(255, Math.max(0, parseInt(text, 10) || 0))
    emitHex(rgbToHex({ ...rgb, [channel]: n }))
  }

  return (
    <div className="flex gap-3">
      <div
        ref={area}
        data-testid="color-area"
        className="relative h-[170px] w-[160px] shrink-0 cursor-crosshair touch-none rounded"
        style={{
          background: `linear-gradient(to top,#000,transparent),linear-gradient(to right,#fff,${hueColor(hsv.h)})`,
        }}
        {...dragOn((x, y) => emit({ ...hsv, s: x, v: 1 - y }))}
      >
        <span
          className="pointer-events-none absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.6)]"
          style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: value }}
        />
      </div>
      <div className="grid min-w-0 flex-1 content-start gap-2">
        <div className="flex items-center gap-2">
          <button
            type="button"
            aria-label="pick"
            aria-pressed={picking}
            onClick={() => setPicking(true)}
            className="grid size-7 shrink-0 place-items-center rounded border border-white/10 text-[#aab3b7] hover:bg-white/10 hover:text-white"
          >
            <Pipette className="size-4" />
          </button>
          <span
            className="size-7 shrink-0 rounded-full border border-white/20"
            style={{ background: value }}
          />
          <input
            aria-label="hex"
            spellCheck={false}
            value={hexText ?? value}
            onFocus={() => setHexText(value)}
            onChange={(e) => {
              setHexText(e.target.value)
              const hex = parseHex(e.target.value)
              if (hex) emitHex(hex)
            }}
            onBlur={() => setHexText(null)}
            className={`${field} font-mono`}
          />
        </div>
        <div
          ref={strip}
          data-testid="color-hue"
          className="relative h-3 cursor-pointer touch-none rounded-full"
          style={{
            background:
              'linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)',
          }}
          {...dragOn((x) => emit({ ...hsv, h: x * 360 }))}
        >
          <span
            className="pointer-events-none absolute top-1/2 size-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.6)]"
            style={{ left: `${(hsv.h / 360) * 100}%`, background: hueColor(hsv.h) }}
          />
        </div>
        {(['r', 'g', 'b'] as const).map((channel) => (
          <label key={channel} className="flex items-center gap-2 text-xs text-[#aab3b7]">
            <span className="w-3 font-medium uppercase">{channel}</span>
            <input
              aria-label={channel.toUpperCase()}
              type="number"
              min={0}
              max={255}
              value={rgb[channel]}
              onChange={(e) => setChannel(channel, e.target.value)}
              className={`${field} [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none`}
            />
          </label>
        ))}
      </div>
      {picking && (
        <PenLayer
          areas={[area, strip]}
          hsv={hsv}
          onPick={(hex) => {
            setPicking(false)
            if (hex) emitHex(hex)
          }}
        />
      )}
    </div>
  )
}

/** Covers the whole app while the pen is out: whatever is clicked gives its color. */
function PenLayer({
  areas,
  hsv,
  onPick,
}: {
  areas: [RefObject<HTMLDivElement | null>, RefObject<HTMLDivElement | null>]
  hsv: Hsv
  onPick: (hex: string | null) => void
}) {
  const layer = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<{ x: number; y: number; hex: string } | null>(null)

  const colorAt = (x: number, y: number): string | null => {
    const [area, strip] = areas.map((r) => r.current?.getBoundingClientRect())
    const inside = (b: DOMRect | undefined) =>
      !!b && x >= b.left && x <= b.right && y >= b.top && y <= b.bottom
    if (inside(area))
      return hsvToHex({
        h: hsv.h,
        s: fraction(x, area!.left, area!.width),
        v: 1 - fraction(y, area!.top, area!.height),
      })
    if (inside(strip)) return hueColor(fraction(x, strip!.left, strip!.width) * 360)
    const el = document.elementsFromPoint(x, y).find((e) => e !== layer.current)
    return el ? colorOfElement(el) : null
  }

  return createPortal(
    <div
      ref={layer}
      data-eyedropper
      className="fixed inset-0 z-[10001]"
      style={{ cursor: PEN_CURSOR }}
      onPointerMove={(e) => {
        const hex = colorAt(e.clientX, e.clientY)
        setHover(hex ? { x: e.clientX, y: e.clientY, hex } : null)
      }}
      onPointerLeave={() => setHover(null)}
      onClick={(e) => onPick(colorAt(e.clientX, e.clientY))}
    >
      {hover && (
        <span
          className="pointer-events-none fixed size-7 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.6)]"
          style={{ left: hover.x + 14, top: hover.y + 8, background: hover.hex }}
        />
      )}
    </div>,
    document.body,
  )
}
