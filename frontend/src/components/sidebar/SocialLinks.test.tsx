import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'

const openExternal = vi.fn()
vi.mock('@/lib/external', () => ({
  MOKORA_DISCORD_URL: 'https://discord.example/mokora',
  MOKORA_GITHUB_URL: 'https://github.example/mokora',
  openExternal: (url: string) => openExternal(url),
}))

import { SocialLinks } from './SocialLinks'

describe('SocialLinks', () => {
  beforeEach(() => openExternal.mockReset())

  it('opens each link through the backend instead of navigating the webview', () => {
    render(<SocialLinks open />)
    const discord = screen.getByRole('link', { name: 'Discord' })
    expect(fireEvent.click(discord)).toBe(false) // default prevented
    expect(openExternal).toHaveBeenCalledWith('https://discord.example/mokora')
    fireEvent.click(screen.getByRole('link', { name: 'GitHub' }))
    expect(openExternal).toHaveBeenCalledWith('https://github.example/mokora')
  })

  it('lies flat in the open sidebar and stands in a column on the rail', () => {
    const { container, rerender } = render(<SocialLinks open />)
    const list = container.querySelector('[data-social-links]')!
    expect(list.classList.contains('flex-row')).toBe(true)
    rerender(<SocialLinks open={false} />)
    expect(list.classList.contains('flex-col')).toBe(true)
  })
})
