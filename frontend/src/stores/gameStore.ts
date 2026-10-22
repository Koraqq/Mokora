import { create } from 'zustand'
import type { GameStateSnapshot, MahgenView } from '@/types'

type GameStore = {
  game: GameStateSnapshot | null
  view: MahgenView | null
  setGame: (g: GameStateSnapshot | null) => void
  setView: (v: MahgenView | null) => void
}

export const useGameStore = create<GameStore>((set) => ({
  game: null,
  view: null,
  setGame: (game) => set({ game }),
  setView: (view) => set({ view }),
}))

/** Active player count for the current game. Defaults to 4 when no game is in
 * progress. UI components should use this for any seat-relative arithmetic
 * (winds, kamicha/shimocha, etc.) so 3p (sanma) and 4p stay correct. */
export const useNumPlayers = () => useGameStore((s) => s.game?.num_players ?? 4)
