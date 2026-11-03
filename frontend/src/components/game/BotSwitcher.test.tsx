import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'en' } }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))
vi.mock('@/components/ui/sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))
vi.mock('@/hooks/useLicense', () => ({ useLicense: () => [null, () => {}] }))
vi.mock('@/hooks/useProStatus', () => ({ useProStatus: () => null }))
vi.mock('@/lib/tauri', () => ({
  HAS_TAURI: false,
  listen: () => Promise.resolve(() => {}),
  invoke: (cmd: string) => Promise.resolve(cmd === 'list_bots' ? [] : null),
}))

import { BotSwitcher } from './BotSwitcher'

const highlight = () => screen.getByTestId('bot-mode-highlight').style.transform
const list = () => screen.getByTestId('bot-list').className

describe('BotSwitcher', () => {
  afterEach(cleanup)

  it('shows a switch between 3p and 4p even when both lists are the same', () => {
    render(<BotSwitcher minLeft={0} />)
    fireEvent.click(screen.getByLabelText('play.bot_switch'))
    // Opening fades the panel in; the list itself does not slide.
    expect(highlight()).toBe('translateY(0%)')
    expect(list()).not.toContain('slide-in')

    fireEvent.mouseEnter(screen.getByText('play.bot_4p'))
    expect(highlight()).toBe('translateY(100%)')
    expect(list()).toContain('slide-in-from-bottom-2')

    fireEvent.mouseEnter(screen.getByText('play.bot_3p'))
    expect(highlight()).toBe('translateY(0%)')
    expect(list()).toContain('slide-in-from-top-2')
  })

  it('does not slide again when the row already shown is pointed at', () => {
    render(<BotSwitcher minLeft={0} />)
    fireEvent.click(screen.getByLabelText('play.bot_switch'))
    fireEvent.mouseEnter(screen.getByText('play.bot_3p'))
    expect(list()).not.toContain('slide-in')
  })
})
