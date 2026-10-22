import { create } from 'zustand'
import type { AnalysisResult } from '@/types'

type AnalysisStore = {
  result: AnalysisResult | null
  updatedAt: number | null
  set: (r: AnalysisResult | null) => void
}

export const useAnalysisStore = create<AnalysisStore>((set) => ({
  result: null,
  updatedAt: null,
  set: (r) => set({ result: r, updatedAt: r ? Date.now() : null }),
}))
