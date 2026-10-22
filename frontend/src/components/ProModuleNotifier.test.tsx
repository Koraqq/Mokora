import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render } from '@testing-library/react'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, o?: Record<string, unknown>) => (o && 'percent' in o ? `${key} ${o.percent}` : key),
    i18n: { language: 'en', changeLanguage: vi.fn() },
  }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}))

const toast = vi.hoisted(() => ({
  loading: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
  error: vi.fn(),
  dismiss: vi.fn(),
}))
vi.mock('sonner', () => ({ toast }))

const invoke = vi.fn()
vi.mock('@/lib/tauri', () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  listen: () => Promise.resolve(() => {}),
  HAS_TAURI: true,
}))

import type { ProStatus } from '@/lib/pro'

let status: ProStatus | null = null
vi.mock('@/hooks/useProStatus', () => ({ useProStatus: () => status }))

import { ProModuleNotifier } from './ProModuleNotifier'

describe('ProModuleNotifier', () => {
  beforeEach(() => {
    Object.values(toast).forEach((f) => f.mockReset())
    invoke.mockReset()
  })

  it('a module already there at start says nothing', () => {
    status = { state: 'ready', version: '0.1.0' }
    render(<ProModuleNotifier />)
    expect(toast.success).not.toHaveBeenCalled()
    expect(toast.loading).not.toHaveBeenCalled()
  })

  it('a download shows its progress, then that it is installed, in one toast', () => {
    status = { state: 'downloading', version: '0.1.0', received: 512 * 1024, total: 1024 * 1024 }
    const { rerender } = render(<ProModuleNotifier />)
    expect(toast.loading).toHaveBeenCalledWith('pro.downloading 50', expect.objectContaining({ id: 'pro-module' }))
    status = { state: 'ready', version: '0.1.0' }
    rerender(<ProModuleNotifier />)
    expect(toast.success).toHaveBeenCalledWith('pro.ready', expect.objectContaining({ id: 'pro-module' }))
  })

  it('a failure offers a retry that asks the Rust side again', () => {
    status = { state: 'failed', error: 'network' }
    render(<ProModuleNotifier />)
    const [, opts] = toast.error.mock.calls[0]
    expect(opts.description).toBe('pro.error.network')
    opts.action.onClick()
    expect(invoke).toHaveBeenCalledWith('pro_retry')
  })
})
