import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { SettingsLock } from '@/components/SettingsLock'
import { useSettingsGuardStore } from '@/stores/settingsGuardStore'

beforeEach(() => useSettingsGuardStore.setState({ changes: 0, nudges: 0 }))

function press(exempt?: boolean) {
  const onClick = vi.fn()
  render(
    <SettingsLock exempt={exempt}>
      <button onClick={onClick}>go</button>
    </SettingsLock>,
  )
  fireEvent.click(screen.getByRole('button', { name: 'go' }))
  return onClick
}

describe('SettingsLock', () => {
  it('lets presses through while nothing waits to be saved', () => {
    expect(press()).toHaveBeenCalledTimes(1)
  })

  it('swallows them while changes wait', () => {
    useSettingsGuardStore.setState({ changes: 2 })
    expect(press()).not.toHaveBeenCalled()
  })

  it('leaves an exempt part usable while changes wait (the sidebar as its order is set)', () => {
    useSettingsGuardStore.setState({ changes: 2 })
    expect(press(true)).toHaveBeenCalledTimes(1)
  })
})
