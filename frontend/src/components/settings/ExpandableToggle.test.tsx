import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'en', changeLanguage: vi.fn() } }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

import { ExpandableToggle } from './ExpandableToggle'

function row(props: { checked?: boolean; locked?: boolean; onChange?: (v: boolean) => void }) {
  return render(
    <ExpandableToggle
      label="Auto play"
      checked={props.checked ?? false}
      onCheckedChange={props.onChange ?? (() => {})}
      locked={props.locked}
      lockReason="needs a membership"
    >
      <input aria-label="delay" defaultValue="300" />
    </ExpandableToggle>,
  )
}

describe('ExpandableToggle', () => {
  it('starts folded; the chevron unfolds and folds the settings', () => {
    row({})
    expect(screen.queryByLabelText('delay')).toBeNull()
    fireEvent.click(screen.getByLabelText('settings.automation.expand'))
    expect(screen.getByLabelText('delay')).toBeTruthy()
    fireEvent.click(screen.getByLabelText('settings.automation.collapse'))
    expect(screen.queryByLabelText('delay')).toBeNull()
  })

  it('the settings can be changed while the switch is off', () => {
    row({ checked: false })
    fireEvent.click(screen.getByLabelText('settings.automation.expand'))
    const input = screen.getByLabelText('delay') as HTMLInputElement
    expect(input.disabled).toBe(false)
    fireEvent.change(input, { target: { value: '500' } })
    expect(input.value).toBe('500')
  })

  it('the switch is its own: it does not fold or unfold', () => {
    const onChange = vi.fn()
    row({ onChange })
    fireEvent.click(screen.getByRole('switch'))
    expect(onChange).toHaveBeenCalledWith(true)
    expect(screen.queryByLabelText('delay')).toBeNull()
  })

  it('locked: a lock and its reason, and nothing can be pressed', () => {
    const onChange = vi.fn()
    row({ locked: true, onChange })
    expect(screen.getByLabelText('needs a membership')).toBeTruthy()
    const toggle = screen.getByRole('switch') as HTMLButtonElement
    const chevron = screen.getByLabelText('settings.automation.expand') as HTMLButtonElement
    expect(toggle.disabled).toBe(true)
    expect(chevron.disabled).toBe(true)
    fireEvent.click(toggle)
    fireEvent.click(chevron)
    expect(onChange).not.toHaveBeenCalled()
    expect(screen.queryByLabelText('delay')).toBeNull()
  })
})
