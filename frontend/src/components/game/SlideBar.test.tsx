import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'

import { SlideBar } from './SlideBar'

// jsdom has no layout: three 32px buttons side by side in a 32px-high row.
function layout() {
  const bar = screen.getByTestId('bar').closest('.relative.flex') as HTMLElement
  const rect = (left: number, top = 0, w = 32, h = 32) =>
    ({ left, top, right: left + w, bottom: top + h, width: w, height: h, x: left, y: top }) as DOMRect
  bar.parentElement!.getBoundingClientRect = () => rect(0, 0, 200, 32)
  bar.getBoundingClientRect = () => rect(0, 0, 200, 32)
  const buttons = ['a', 'b', 'menu'].map((id) => screen.getByTestId(id))
  buttons.forEach((b, i) => (b.getBoundingClientRect = () => rect(i * 40)))
  const below = screen.getByTestId('panel-button')
  below.getBoundingClientRect = () => rect(0, 60)
  document.elementFromPoint = (x: number, y: number) => {
    if (y >= 60) return below
    return buttons.find((_, i) => x >= i * 40 && x < i * 40 + 32) ?? null
  }
  return bar.parentElement!
}

const at = (i: number) => ({ clientX: i * 40 + 10, clientY: 10, pointerId: 1, button: 0 })

describe('SlideBar', () => {
  const clicks: string[] = []
  const menuDown = vi.fn()

  beforeEach(() => {
    clicks.length = 0
    menuDown.mockReset()
    HTMLElement.prototype.setPointerCapture = vi.fn()
    render(
      <SlideBar>
        <span data-testid="bar" />
        <button data-testid="a" onClick={() => clicks.push('a')} />
        <button data-testid="b" onClick={() => clicks.push('b')} />
        <button data-testid="menu" aria-haspopup="menu" onPointerDown={menuDown} />
        <button data-testid="panel-button" onClick={() => clicks.push('panel')} />
      </SlideBar>,
    )
  })

  afterEach(() => {
    // @ts-expect-error restore jsdom's missing API
    delete document.elementFromPoint
  })

  it('pressing one button and letting go on another presses only the second', () => {
    const bar = layout()
    fireEvent.pointerDown(bar, at(0))
    fireEvent.pointerMove(bar, at(1))
    expect(screen.getByTestId('b').hasAttribute('data-slide-active')).toBe(true)
    expect(screen.getByTestId('a').hasAttribute('data-slide-active')).toBe(false)
    fireEvent.pointerUp(bar, at(1))
    expect(clicks).toEqual(['b'])
    expect(screen.getByTestId('b').hasAttribute('data-slide-active')).toBe(false)
  })

  it('a plain click still presses its button, once', () => {
    const bar = layout()
    fireEvent.pointerDown(bar, at(0))
    fireEvent.pointerUp(bar, at(0))
    expect(clicks).toEqual(['a'])
  })

  it('letting go off the buttons presses nothing and clears the highlight', () => {
    const bar = layout()
    fireEvent.pointerDown(bar, at(0))
    fireEvent.pointerUp(bar, { ...at(0), clientX: 500 })
    expect(clicks).toEqual([])
    expect(document.querySelector('[data-slide-active]')).toBeNull()
  })

  it('a menu that opens on pointer-down is opened by a slide, not twice by a click', () => {
    const bar = layout()
    fireEvent.pointerDown(bar, at(0))
    fireEvent.pointerUp(bar, at(2))
    expect(menuDown).toHaveBeenCalledTimes(1)
    menuDown.mockReset()
    // Pressed on it: its own pointer-down already opened it.
    fireEvent.pointerDown(screen.getByTestId('menu'), at(2))
    fireEvent.pointerUp(bar, at(2))
    expect(menuDown).toHaveBeenCalledTimes(1)
  })

  it('a button stays pressable while scaled up past the row edges', () => {
    const bar = layout()
    // As the highlight scales it: 32px grown to 38px about the same centre.
    screen.getByTestId('b').getBoundingClientRect = () =>
      ({ left: 37, top: -3, right: 75, bottom: 35, width: 38, height: 38, x: 37, y: -3 }) as DOMRect
    fireEvent.pointerDown(bar, at(0))
    fireEvent.pointerMove(bar, at(1))
    fireEvent.pointerMove(bar, { ...at(1), clientX: 52 })
    expect(screen.getByTestId('b').hasAttribute('data-slide-active')).toBe(true)
    fireEvent.pointerUp(bar, { ...at(1), clientX: 52 })
    expect(clicks).toEqual(['b'])
  })

  it('buttons of a panel hanging below the row are left alone', () => {
    const bar = layout()
    fireEvent.pointerDown(bar, { ...at(0), clientY: 70 })
    expect(document.querySelector('[data-slide-active]')).toBeNull()
    expect(HTMLElement.prototype.setPointerCapture).not.toHaveBeenCalled()
  })
})
