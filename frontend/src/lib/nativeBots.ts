// Reserved names of the built-in, pure-Rust bots (see the backend's
// `bot::native` module — `is_native` gates on exactly these). Always
// available: their weights are embedded in the binary, so no install or
// Python environment is ever needed. Shared here so the Bots table, the
// status bar and the Setup wizard can't drift
// apart on the spelling.

/** Retired built-in bots (the original base bots and Mokora): no longer offered
 *  or run, but game records played by them still carry these names, so the
 *  history and review pages can label them. */
export const NATIVE_4P = 'base-4p'
export const NATIVE_3P = 'base-3p'
export const MOKORA_3P = 'mokora-3p'
export const MOKORA_4P = 'mokora-4p'
/** Mora: the sanma bot at Mokora's size on the richer V2 feature planes
 *  (discard order, tedashi, passed tiles, shanten, ukeire). 3-player only. */
export const MORA_3P = 'mora-3p'
/** Mora's yonma model: the same V2 planes for four players. 4-player only. */
export const MORA_4P = 'mora-4p'
/** Yakuman: Mora's weights playing every hand for a yakuman (the
 *  backend's yakuman planner). One per player count. */
export const YAKUMAN_3P = 'yakuman-3p'
export const YAKUMAN_4P = 'yakuman-4p'

/** The built-in bots offered today, each with the player count it plays. */
const NATIVE_BOTS: Record<string, '3p' | '4p'> = {
  [MORA_3P]: '3p',
  [MORA_4P]: '4p',
  [YAKUMAN_3P]: '3p',
  [YAKUMAN_4P]: '4p',
}

/** The names the built-in bots went by before `<name>-3p` / `<name>-4p`
 *  (the backend's `bot::native::renamed`). Game records saved before still
 *  carry them; configs are renamed when loaded. */
const LEGACY_NAMES: Record<string, string> = {
  'mokora-native3p': MOKORA_3P,
  'mokora-native4p': MOKORA_4P,
  'mora-native3p': MORA_3P,
  'yakuman-native3p': YAKUMAN_3P,
}

/** `name`, or a built-in bot's current name for its old one. */
export function canonicalBotName(name: string): string {
  return LEGACY_NAMES[name] ?? name
}

/** The short name of a built-in bot (`Yakuman`, `Mora`, `Mokora`,
 *  `Base`); null for any other bot. */
export function nativeBotShortName(name: string): string | null {
  name = canonicalBotName(name)
  if (name === YAKUMAN_3P || name === YAKUMAN_4P) return 'Yakuman'
  if (name === MORA_3P || name === MORA_4P) return 'Mora'
  if (name === MOKORA_3P || name === MOKORA_4P) return 'Mokora'
  if (name === NATIVE_3P || name === NATIVE_4P) return 'Base'
  return null
}

/** The player counts a built-in bot plays: every one is for a single count. */
export function nativeModes(name: string): Array<'3p' | '4p'> {
  name = canonicalBotName(name)
  return [NATIVE_BOTS[name] ?? '4p']
}

/** The bot that plays for `name` under the license (the backend's
 *  `native::in_force`): Yakuman is a paid feature, and without it its side's
 *  Mora plays. Every other name is itself. */
export function botInForce(name: string, paid: boolean): string {
  const c = canonicalBotName(name)
  if (!paid && c === YAKUMAN_3P) return MORA_3P
  if (!paid && c === YAKUMAN_4P) return MORA_4P
  return name
}

export function isNativeBot(name: string): boolean {
  name = canonicalBotName(name)
  return name in NATIVE_BOTS
}
