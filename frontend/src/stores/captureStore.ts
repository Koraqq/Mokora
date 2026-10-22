import { create } from 'zustand'
import type { CaptureStatus } from '@/types'

type CaptureStore = {
  status: CaptureStatus
  set: (s: CaptureStatus) => void
}

export const useCaptureStore = create<CaptureStore>((set) => ({
  status: { state: 'stopped' },
  set: (status) => set({ status }),
}))
