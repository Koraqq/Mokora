// Which built-in bot played each recorded game, for the History page's
// per-bot views (rank distribution, cumulative PT, detailed stats).
//
// Mokora's games carry `bot: "mokora-3p"` (3p) or `"mokora-4p"`
// (4p), Mora's `"mora-3p"`, Yakuman's `"yakuman-3p"` / `"yakuman-4p"`. Every
// other game was played by the original base bot — including records from
// before `bot` was stored, which all predate Mokora — so the split is
// Mokora, Mora, Yakuman and the rest. Records saved before the built-in
// bots were renamed carry their old names (`mora-native3p`…), read as the
// new ones.

import {
  MOKORA_3P,
  MOKORA_4P,
  MORA_3P,
  MORA_4P,
  YAKUMAN_3P,
  YAKUMAN_4P,
  canonicalBotName,
  isNativeBot,
  nativeBotShortName,
} from '@/lib/nativeBots'
import type { BotInfo, GameRecord } from '@/types'

/** A per-bot view: every game together (`all`), or one bot family's games
 *  (a key of [`botFamily`]). */
export type BotView = string

/** The family a recorded bot belongs to, which the History page filters by:
 *  a built-in bot's both player counts are one (`mora`, `yakuman`, and the
 *  retired `mokora` / `base`), any other bot is its own name. Games recorded
 *  before the bot was stored are the original base bot's. */
export function botFamily(bot: string | null | undefined): string {
  const name = canonicalBotName(bot ?? '')
  if (!name) return 'base'
  return nativeBotShortName(name)?.toLowerCase() ?? name
}

/** The families the History page offers: those of the bots installed now,
 *  each once, labelled by their short name (or the bot's own display name). */
export function botFamilies(bots: BotInfo[]): Array<{ key: string; label: string }> {
  const seen = new Set<string>()
  const out: Array<{ key: string; label: string }> = []
  for (const b of bots) {
    const key = botFamily(b.name)
    if (seen.has(key)) continue
    seen.add(key)
    const label = isNativeBot(b.name)
      ? (nativeBotShortName(b.name) ?? b.name)
      : (b.manifest?.bot.display ?? b.name)
    out.push({ key, label })
  }
  return out
}

/** Is `bot` one of Mokora's models (either player count)? */
export function isMokora(bot: string | null | undefined): boolean {
  const name = canonicalBotName(bot ?? '')
  return name === MOKORA_3P || name === MOKORA_4P
}

/** Is `bot` Mora (either player count)? */
export function isMora(bot: string | null | undefined): boolean {
  const name = canonicalBotName(bot ?? '')
  return name === MORA_3P || name === MORA_4P
}

/** Is `bot` Yakuman (either player count)? */
export function isYakuman(bot: string | null | undefined): boolean {
  const name = canonicalBotName(bot ?? '')
  return name === YAKUMAN_3P || name === YAKUMAN_4P
}

/** A recorded game's built-in bot, for the cumulative-PT chart's markers. */
export type BuiltinBot = 'base' | 'mokora' | 'mora' | 'yakuman'

/** Which built-in bot a recorded `bot` name is (any other is drawn as base). */
export function botOfName(bot: string | null | undefined): BuiltinBot {
  if (isMokora(bot)) return 'mokora'
  if (isMora(bot)) return 'mora'
  if (isYakuman(bot)) return 'yakuman'
  return 'base'
}

export function botOf(r: GameRecord): BuiltinBot {
  return botOfName(r.bot)
}

/** The games of one bot family, or all of them. */
export function recordsOfBot(records: GameRecord[], view: BotView): GameRecord[] {
  if (view === 'all') return records
  return records.filter((r) => botFamily(r.bot) === view)
}
