import { useLayoutEffect, type RefObject } from 'react'
import { invoke } from '@/lib/tauri'

// The game is a second web view laid over this one, so it is positioned from
// here: it goes wherever `ref`'s element is, follows it when the window is
// resized, and goes off screen with the page. It also steps aside while a
// dialog is open — a web view always paints above the page, so a dialog
// centred over the game would otherwise be hidden behind it.

const DIALOG = '[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]'

export function useGamePlacement(ref: RefObject<HTMLElement | null>, enabled = true) {
  useLayoutEffect(() => {
    const el = ref.current
    if (!el || !enabled) return
    let frame = 0
    let last = ''
    const sync = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        if (document.querySelector(DIALOG)) {
          if (last !== 'hidden') {
            last = 'hidden'
            invoke('hide_game').catch(() => {})
          }
          return
        }
        const r = el.getBoundingClientRect()
        const next = `${r.left},${r.top},${r.width},${r.height}`
        if (next === last) return
        last = next
        invoke('place_game', { x: r.left, y: r.top, width: r.width, height: r.height }).catch(
          () => {},
        )
      })
    }
    const resize = new ResizeObserver(sync)
    resize.observe(el)
    // Dialogs mount into the body: watch it for them coming and going.
    const dom = new MutationObserver(sync)
    dom.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-state'],
    })
    window.addEventListener('resize', sync)
    sync()
    return () => {
      resize.disconnect()
      dom.disconnect()
      window.removeEventListener('resize', sync)
      cancelAnimationFrame(frame)
      invoke('hide_game').catch(() => {})
    }
  }, [ref, enabled])
}
