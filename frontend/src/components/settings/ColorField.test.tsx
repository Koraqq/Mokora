import { useState } from 'react'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { ColorField } from '@/components/settings/ColorField'
import { colorOfElement, hexToHsv, hsvToHex, parseHex } from '@/lib/color'

afterEach(cleanup)

function Harness({ start = '#fafafa' }: { start?: string }) {
  const [value, setValue] = useState(start)
  return (
    <div>
      <div data-testid="paint" style={{ backgroundColor: 'rgb(10, 200, 30)' }}>
        <ColorField label="light background" value={value} onChange={setValue} />
      </div>
      <output data-testid="out">{value}</output>
    </div>
  )
}

const out = () => screen.getByTestId('out').textContent

describe('color math', () => {
  it('reads short and long hex and rejects the rest', () => {
    expect(parseHex('#ABC')).toBe('#aabbcc')
    expect(parseHex('0a1b2c')).toBe('#0a1b2c')
    expect(parseHex('#12')).toBeNull()
    expect(parseHex('zzzzzz')).toBeNull()
  })

  it('round-trips through hsv and keeps the hue of a grey', () => {
    for (const hex of ['#ff0000', '#12ab9f', '#fafafa', '#000000']) {
      expect(hsvToHex(hexToHsv(hex))).toBe(hex)
    }
    expect(hexToHsv('#808080', 200).h).toBe(200)
  })

  it('finds the color under an element from its own and its parents backgrounds', () => {
    const outer = document.createElement('div')
    outer.style.backgroundColor = 'rgb(10, 20, 30)'
    const inner = document.createElement('span')
    outer.appendChild(inner)
    document.body.appendChild(outer)
    expect(colorOfElement(inner)).toBe('#0a141e')
    outer.remove()
  })
})

describe('ColorField', () => {
  it('opens a dark picker and takes RGB and hex', () => {
    render(<Harness />)
    expect(screen.queryByRole('dialog')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'light background' }))
    expect(screen.getByRole('dialog')).toBeTruthy()

    fireEvent.change(screen.getByLabelText('R'), { target: { value: '255' } })
    expect(out()).toBe('#fffafa')
    fireEvent.change(screen.getByLabelText('G'), { target: { value: '0' } })
    fireEvent.change(screen.getByLabelText('B'), { target: { value: '0' } })
    expect(out()).toBe('#ff0000')

    fireEvent.change(screen.getByLabelText('hex'), { target: { value: '#123' } })
    expect(out()).toBe('#112233')
    expect((screen.getByLabelText('G') as HTMLInputElement).value).toBe('34')
  })

  it('closes on Escape and on a click outside', () => {
    render(<Harness />)
    const open = () => fireEvent.click(screen.getByRole('button', { name: 'light background' }))
    open()
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(screen.queryByRole('dialog')).toBeNull()
    open()
    fireEvent.pointerDown(screen.getByTestId('out'))
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('picks the color of whatever is clicked while the pen is out, then Escape puts it away', () => {
    document.elementsFromPoint = () => [
      document.querySelector('[data-eyedropper]')!,
      screen.getByTestId('paint'),
    ]
    render(<Harness />)
    fireEvent.click(screen.getByRole('button', { name: 'light background' }))
    fireEvent.click(screen.getByRole('button', { name: 'pick' }))
    const layer = document.querySelector('[data-eyedropper]')!
    expect(layer).toBeTruthy()
    // Clicking the layer must not close the picker.
    fireEvent.pointerDown(layer)
    fireEvent.click(layer, { clientX: 1, clientY: 1 })
    expect(out()).toBe('#0ac81e')
    expect(document.querySelector('[data-eyedropper]')).toBeNull()
    expect(screen.getByRole('dialog')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'pick' }))
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(document.querySelector('[data-eyedropper]')).toBeNull()
    expect(screen.getByRole('dialog')).toBeTruthy()
    expect(out()).toBe('#0ac81e')
  })
})
