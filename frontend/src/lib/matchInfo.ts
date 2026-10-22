// Display helpers for `GameRecord.match_info` (room / rank-lobby labels and
// paifu ids). The backend persists raw platform ids; the label mapping lives
// here so an id this build doesn't know degrades to showing the number
// instead of a wrong name.

import type { MatchInfo } from '@/types'

/**
 * Majsoul ranked matchmode id → room tier key. From the game's own
 * matchmode table: Bronze–Jade own three 4p ids each (best-of-one / East /
 * South) plus two 3p ids (East / South); Melee and Throne have no
 * best-of-one.
 */
const MAJSOUL_ROOM_TIERS: Record<number, string> = {
  1: 'bronze',
  2: 'bronze',
  3: 'bronze',
  4: 'silver',
  5: 'silver',
  6: 'silver',
  7: 'gold',
  8: 'gold',
  9: 'gold',
  10: 'jade',
  11: 'jade',
  12: 'jade',
  13: 'melee',
  14: 'melee',
  15: 'throne',
  16: 'throne',
  17: 'bronze',
  18: 'bronze',
  19: 'silver',
  20: 'silver',
  21: 'gold',
  22: 'gold',
  23: 'jade',
  24: 'jade',
  25: 'throne',
  26: 'throne',
  27: 'melee',
  28: 'melee',
}

/** Room tier key ('bronze' … 'throne', 'melee') of a Majsoul ranked
 *  matchmode id, or null for an unranked / unknown one. */
export function majsoulRoomTier(modeId: number | null | undefined): string | null {
  if (modeId == null) return null
  return MAJSOUL_ROOM_TIERS[modeId] ?? null
}

/**
 * Riichi City `options.stage_type` → ranked room tier, lowest to highest.
 * The client's tier enum is exhaustive at four (NewStar/BrightMoon/HotSun/
 * MilkyWay). Tier names follow the client's own strings
 * (`MATCHING_VIEW_STATE_TYPE_*`: 新星/霞月/炎陽/銀河, EN Star/Moon/Sun/
 * Galaxy); its replay list labels a ranked game exactly this way.
 */
const RIICHI_CITY_ROOM_TIERS: Record<number, string> = {
  1: 'star',
  2: 'moon',
  3: 'sun',
  4: 'galaxy',
}

/**
 * Riichi City `options.game_play` → mode label key, from the client's
 * `GamePlayType` enum (its game logic ships as Lua in
 * `Mahjong-JP_Data/StreamingAssets/Base/lua_common`). Labels are the
 * client's own localized strings — the ones its replay list uses to title
 * each mode.
 *
 * 1001 (Rank) is handled separately via the stage tier above. 1005 is the
 * one-round lobby's queue instance of 1004, and 1008 the casual-hall
 * instance of 1007 — each pair shares one client string. 1006 (TwoPlay,
 * folded into friend rooms) and 1011 (AK2P, a retired collab) have no
 * replay-list branch in the client; their labels combine the official
 * 2-player string (PLAY_NAME_2) with the enum's own comment. `All` (0) is
 * a filter sentinel and `CombineMahjong` (1) is a merge minigame — neither
 * reaches a mahjong table.
 */
const RIICHI_CITY_MODES: Record<number, string> = {
  1002: 'rc_tournament',
  1003: 'rc_friendly',
  1004: 'rc_one_round',
  1005: 'rc_one_round',
  1006: 'rc_two_player',
  1007: 'rc_command',
  1008: 'rc_command',
  1009: 'rc_chinitsu',
  1010: 'rc_taiwan',
  1011: 'rc_ak_2p',
  1012: 'rc_ai_challenge',
  1013: 'rc_chill',
  1014: 'rc_seventeen',
  1015: 'rc_practice',
  1016: 'rc_fury_waves',
  1021: 'rc_taiwan_ranked',
  1022: 'rc_hidden_war',
}

/**
 * Tenhou `<GO type=…>` room bits → tier key. 0x80 alone = Joukyuu, 0x20
 * alone = Tokujou, both = Houou, neither = Ippan (ranked lobby 0 only —
 * private lobbies use the lobby number instead).
 */
function tenhouTier(goType: number): string {
  const idx = (goType & 0x20 ? 2 : 0) + (goType & 0x80 ? 1 : 0)
  return ['ippan', 'joukyuu', 'tokujou', 'houou'][idx]
}

/**
 * i18n key (+ params) for the room / rank lobby a game was played in, or
 * null when the record carries nothing displayable. Callers render with
 * `t(key, params)`.
 */
export function roomLabelKey(
  info: MatchInfo | null | undefined,
): { key: string; params?: Record<string, unknown> } | null {
  if (!info) return null
  switch (info.platform) {
    case 'majsoul': {
      // An event's matchmode (萬象修羅 and the like) is not a room tier.
      if (gameKind(info) === 'event' && info.contest_uid == null) {
        return { key: 'history.room.majsoul_event' }
      }
      if (info.mode_id != null) {
        const tier = MAJSOUL_ROOM_TIERS[info.mode_id]
        return tier
          ? { key: `history.room.majsoul_${tier}` }
          : { key: 'history.room.raw', params: { id: info.mode_id } }
      }
      // Non-matchmade tables (mode_id 0 → absent): tournament, then
      // friendly/AI room, identified by their raw numbers.
      if (info.contest_uid != null) {
        return { key: 'history.room.majsoul_contest', params: { id: info.contest_uid } }
      }
      if (info.room_id != null) {
        return { key: 'history.room.majsoul_friendly', params: { id: info.room_id } }
      }
      return null
    }
    case 'tenhou': {
      if (info.lobby != null && info.lobby !== 0) {
        return { key: 'history.room.tenhou_lobby', params: { lobby: info.lobby } }
      }
      if (info.go_type == null) return null
      return { key: `history.room.tenhou_${tenhouTier(info.go_type)}` }
    }
    case 'riichi_city': {
      const gp = info.game_play
      // Ranked queue: label by the stage tier (falling back to the generic
      // "Ranked Match" when the tier is missing or unknown).
      if (gp === 1001 || (gp == null && info.stage_type)) {
        const tier = info.stage_type
          ? RIICHI_CITY_ROOM_TIERS[info.stage_type]
          : undefined
        if (tier) return { key: `history.room.rc_${tier}` }
        if (gp === 1001) return { key: 'history.room.rc_ranked' }
        return { key: 'history.room.raw', params: { id: info.stage_type } }
      }
      if (gp == null) return null
      const mode = RIICHI_CITY_MODES[gp]
      return mode
        ? { key: `history.room.${mode}` }
        : { key: 'history.room.raw', params: { id: gp } }
    }
  }
  // Unreachable for the current MatchInfo union; keeps the declared return
  // type honest if a platform variant ships before this mirror learns it.
  return null
}

/** What kind of game a record was: a ranked one, a friendly room, or an
 *  event — the 比賽場's tournaments and special modes. Only ranked games
 *  earn rank points. */
export type GameKind = 'ranked' | 'friendly' | 'event'

/** Riichi City friendly-room `game_play`. */
const RC_FRIENDLY = 1003

/**
 * The kind of game `info` describes. A Mahjong Soul friendly room is
 * `category` 1 (or, on records from before it was kept, a room number and no
 * ranked matchmode); a tournament, a category other than ranked, or any
 * special rule (萬象修羅, 赤羽之戰 …) is an event. A Tenhou private lobby is
 * friendly; a Riichi City game outside its ranked queue is friendly in its
 * friend rooms and an event otherwise. Records with nothing to go on count as
 * ranked.
 */
export function gameKind(info: MatchInfo | null | undefined): GameKind {
  if (!info) return 'ranked'
  switch (info.platform) {
    case 'majsoul': {
      if (info.category === 1) return 'friendly'
      if (info.category == null && info.mode_id == null && info.room_id != null) return 'friendly'
      if (info.contest_uid != null || (info.rules?.length ?? 0) > 0) return 'event'
      if (info.category != null && info.category !== 2) return 'event'
      return 'ranked'
    }
    case 'tenhou':
      return info.lobby != null && info.lobby !== 0 ? 'friendly' : 'ranked'
    case 'riichi_city':
      if (info.game_play == null || info.game_play === 1001) return 'ranked'
      return info.game_play === RC_FRIENDLY ? 'friendly' : 'event'
  }
  return 'ranked'
}

/** The platform's own game (paifu) id, if the record carries one. */
export function matchGameId(info: MatchInfo | null | undefined): string | null {
  if (!info) return null
  switch (info.platform) {
    case 'majsoul':
      return info.game_uuid ?? null
    case 'tenhou':
      return info.log_id ?? null
    case 'riichi_city':
      // Table-instance token — not a replay id, but the closest the wire
      // offers to a per-game identifier.
      return info.room_id ?? null
  }
  return null
}

/**
 * Replay URL, when one can be built without guessing. Tenhou log links are
 * region-independent; Majsoul replay hosts differ per region (which isn't
 * recorded), so Majsoul gets the copyable uuid only. `ourSeat` (the
 * record's `our_seat`, wire-absolute = Tenhou's `tw` index) opens the
 * replay from the player's own perspective.
 */
export function paifuUrl(
  info: MatchInfo | null | undefined,
  ourSeat?: number | null,
): string | null {
  if (info?.platform === 'tenhou' && info.log_id) {
    const tw = ourSeat == null ? '' : `&tw=${ourSeat}`
    return `https://tenhou.net/0/?log=${encodeURIComponent(info.log_id)}${tw}`
  }
  return null
}
