import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.hoisted(() => vi.fn().mockResolvedValue(undefined))
const rawToast = vi.hoisted(() => {
  const fn = vi.fn().mockReturnValue('rendered-id')
  return Object.assign(fn, { dismiss: vi.fn() })
})

vi.mock('@/lib/tauri', () => ({ HAS_TAURI: true, invoke }))
vi.mock('sonner', () => ({ Toaster: vi.fn(), toast: rawToast }))

import { dismissToastLocally, showToast, toast } from './sonner'

describe('synchronised toasts', () => {
  beforeEach(() => {
    invoke.mockClear()
    rawToast.mockClear()
    rawToast.dismiss.mockClear()
  })

  it('publishes a frontend toast for every account UI', () => {
    const id = toast.error('failed', {
      id: 'shared-error',
      description: 'details',
      duration: Infinity,
    })

    expect(id).toBe('shared-error')
    expect(invoke).toHaveBeenCalledWith('publish_notification', {
      notification: {
        level: 'error',
        title: 'failed',
        body: 'details',
        sticky: true,
        id: 'shared-error',
      },
    })
  })

  it('broadcasts a user dismissal', () => {
    showToast('warning', 'expired', { id: 'license-loss-expired' })
    const options = rawToast.mock.calls[0][1]

    options.onDismiss()

    expect(invoke).toHaveBeenCalledWith('dismiss_notification', {
      id: 'license-loss-expired',
    })
  })

  it('does not echo a remote dismissal back to the backend', () => {
    showToast('info', 'done', { id: 'shared-done' })
    const options = rawToast.mock.calls[0][1]
    invoke.mockClear()

    dismissToastLocally('shared-done')
    options.onDismiss()

    expect(rawToast.dismiss).toHaveBeenCalledWith('shared-done')
    expect(invoke).not.toHaveBeenCalled()
  })
})
