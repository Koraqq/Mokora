import { create } from 'zustand'
import type { BotInfo, BotStatus } from '@/types'

type BotStore = {
  status: BotStatus
  list: BotInfo[]
  setStatus: (s: BotStatus) => void
  setList: (l: BotInfo[]) => void
}

export const useBotStore = create<BotStore>((set) => ({
  status: { state: 'idle' },
  list: [],
  setStatus: (status) => set({ status }),
  setList: (list) => set({ list }),
}))
