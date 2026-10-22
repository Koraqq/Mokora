import { create } from 'zustand'

// Whether the "end Mokora?" confirmation is open. It is asked for by the power
// button, and by the backend (`quit-requested`) when the tray's quit or the
// window's × is used while a game is being played. The dialog itself lives in
// `QuitDialog`, mounted once at the app's root.
type QuitState = {
  asking: boolean
  setAsking: (asking: boolean) => void
}

export const useQuitStore = create<QuitState>((set) => ({
  asking: false,
  setAsking: (asking) => set({ asking }),
}))
