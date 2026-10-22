import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { useSidebar } from '@/hooks/useSidebar'

/** Whether an Esc press means "leave the page": it is Esc, nothing has used it
 *  yet, the player is not typing, and no dialog, menu or sidebar drawer is
 *  open (those close on Esc themselves). */
export function leavesOnEscape(e: {
  key: string
  defaultPrevented: boolean
  typing: boolean
  layerOpen: boolean
  drawerOpen?: boolean
}): boolean {
  return (
    e.key === 'Escape' && !e.defaultPrevented && !e.typing && !e.layerOpen && !e.drawerOpen
  )
}

const LAYER = '[role="dialog"], [role="alertdialog"], [role="listbox"], [role="menu"]'

/** The element keys are typed into, when `el` is one. */
function typingIn(el: EventTarget | null): HTMLElement | null {
  if (!(el instanceof HTMLElement)) return null
  return el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) ? el : null
}

/**
 * Esc on any page of the app takes it back to the main screen (the play
 * screen, or the game dashboard); mounted once by the app shell. Listens
 * before Radix's own Esc handling, so an open dialog or drop-down still sees
 * the layer and closes first. Esc in a text field only leaves the field (the
 * next Esc leaves the page). Unsaved settings still hold the page (the
 * router's blocker nudges the settings bar instead).
 */
export function useEscapeToHome() {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const onPlay = pathname === '/play'
  useEffect(() => {
    if (onPlay) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      const layerOpen = document.querySelector(LAYER) !== null
      const field = typingIn(e.target)
      if (field && !layerOpen && !e.defaultPrevented) {
        field.blur()
        return
      }
      if (
        leavesOnEscape({
          key: e.key,
          defaultPrevented: e.defaultPrevented,
          typing: field !== null,
          layerOpen,
          drawerOpen: useSidebar.getState().isDrawerOpen,
        })
      ) {
        navigate('/')
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [navigate, onPlay])
}
