// The three automation levels the play screen's switch offers, and how each
// maps onto the autoplay settings:
//
//   off  — nothing is done for the player; the AI only evaluates.
//   auto — the player's tiles are played (discards, calls, riichi, win).
//   full — also starts the next game by itself, with the rest, log-out and
//          delay settings from the settings page.

import type { AppConfig } from '@/types'

export type AutoMode = 'off' | 'auto' | 'full'

export const AUTO_MODES: AutoMode[] = ['off', 'auto', 'full']

/** The level the saved settings amount to. */
export function modeOf(config: AppConfig): AutoMode {
  const ap = config.autoplay
  if (!ap?.enabled) return 'off'
  return ap.next_game?.enabled ? 'full' : 'auto'
}

/** The level that is actually in force: full auto is a paid feature, and
 *  without it the next game is not started (the Rust side checks the same),
 *  so a saved full auto plays as auto. */
export function modeInForce(config: AppConfig, locked: boolean): AutoMode {
  const mode = modeOf(config)
  return mode === 'full' && locked ? 'auto' : mode
}

/** `config` with the autoplay switches set for `mode`; the rest of the
 *  autoplay settings (delays, rooms, rests) are left as they are. */
export function withMode(config: AppConfig, mode: AutoMode): AppConfig {
  return {
    ...config,
    autoplay: {
      ...config.autoplay,
      enabled: mode !== 'off',
      next_game: { ...config.autoplay.next_game, enabled: mode === 'full' },
    },
  }
}
