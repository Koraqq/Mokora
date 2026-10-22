import { describe, expect, it } from 'vitest'

import { gameKind, matchGameId, paifuUrl, roomLabelKey } from './matchInfo'
import en from '@/i18n/resources/en.json'
import ja from '@/i18n/resources/ja.json'
import zhCN from '@/i18n/resources/zh-CN.json'
import zhTW from '@/i18n/resources/zh-TW.json'

describe('roomLabelKey', () => {
  it('maps Majsoul ranked mode ids to room tiers', () => {
    expect(
      roomLabelKey({ platform: 'majsoul', mode_id: 9 }),
    ).toEqual({ key: 'history.room.majsoul_gold' })
    // 3p ids share the tier keys.
    expect(
      roomLabelKey({ platform: 'majsoul', mode_id: 17 }),
    ).toEqual({ key: 'history.room.majsoul_bronze' })
    expect(
      roomLabelKey({ platform: 'majsoul', mode_id: 26 }),
    ).toEqual({ key: 'history.room.majsoul_throne' })
  })

  it('falls back to the raw number for unknown Majsoul mode ids', () => {
    expect(roomLabelKey({ platform: 'majsoul', mode_id: 99 })).toEqual({
      key: 'history.room.raw',
      params: { id: 99 },
    })
    expect(roomLabelKey({ platform: 'majsoul' })).toBeNull()
  })

  it('labels non-matchmade Majsoul tables by contest or friendly room', () => {
    expect(
      roomLabelKey({ platform: 'majsoul', contest_uid: 777, room_id: 33123 }),
    ).toEqual({ key: 'history.room.majsoul_contest', params: { id: 777 } })
    expect(roomLabelKey({ platform: 'majsoul', room_id: 33123 })).toEqual({
      key: 'history.room.majsoul_friendly',
      params: { id: 33123 },
    })
    // A ranked mode id wins over the room number.
    expect(
      roomLabelKey({ platform: 'majsoul', mode_id: 12, room_id: 33123 }),
    ).toEqual({ key: 'history.room.majsoul_jade' })
  })

  it('decodes Tenhou GO type bits into room tiers', () => {
    // 0x80 alone = Joukyuu, 0x20 alone = Tokujou, both = Houou.
    expect(
      roomLabelKey({ platform: 'tenhou', go_type: 0x09, lobby: 0 }),
    ).toEqual({ key: 'history.room.tenhou_ippan' })
    expect(
      roomLabelKey({ platform: 'tenhou', go_type: 0x89, lobby: 0 }),
    ).toEqual({ key: 'history.room.tenhou_joukyuu' })
    expect(
      roomLabelKey({ platform: 'tenhou', go_type: 0x29, lobby: 0 }),
    ).toEqual({ key: 'history.room.tenhou_tokujou' })
    expect(
      roomLabelKey({ platform: 'tenhou', go_type: 0xa9, lobby: 0 }),
    ).toEqual({ key: 'history.room.tenhou_houou' })
  })

  it('labels non-zero Tenhou lobbies by number instead of tier', () => {
    expect(
      roomLabelKey({ platform: 'tenhou', go_type: 0x09, lobby: 7994 }),
    ).toEqual({ key: 'history.room.tenhou_lobby', params: { lobby: 7994 } })
  })

  it('labels Riichi City games by game mode', () => {
    // Ranked (1001): tier from stage_type, generic ranked label without one.
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1001, stage_type: 2 }),
    ).toEqual({ key: 'history.room.rc_moon' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1001 }),
    ).toEqual({ key: 'history.room.rc_ranked' })
    // Non-ranked modes label by the GamePlayType mapping.
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1003 }),
    ).toEqual({ key: 'history.room.rc_friendly' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1002 }),
    ).toEqual({ key: 'history.room.rc_tournament' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1004 }),
    ).toEqual({ key: 'history.room.rc_one_round' })
    // The mode-specific labels extracted from the client's own enum.
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1010 }),
    ).toEqual({ key: 'history.room.rc_taiwan' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1015 }),
    ).toEqual({ key: 'history.room.rc_practice' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1021 }),
    ).toEqual({ key: 'history.room.rc_taiwan_ranked' })
    expect(
      roomLabelKey({ platform: 'riichi_city', game_play: 1022 }),
    ).toEqual({ key: 'history.room.rc_hidden_war' })
    // Unknown mode ids degrade to the raw number.
    expect(roomLabelKey({ platform: 'riichi_city', game_play: 1099 })).toEqual({
      key: 'history.room.raw',
      params: { id: 1099 },
    })
  })

  it('maps Riichi City stage types 1-4 to Star/Moon/Sun/Galaxy', () => {
    expect(
      roomLabelKey({ platform: 'riichi_city', stage_type: 1 }),
    ).toEqual({ key: 'history.room.rc_star' })
    expect(
      roomLabelKey({ platform: 'riichi_city', stage_type: 4 }),
    ).toEqual({ key: 'history.room.rc_galaxy' })
    // 0/absent = not a ranked queue; unknown values degrade to the number.
    expect(roomLabelKey({ platform: 'riichi_city', stage_type: 0 })).toBeNull()
    expect(roomLabelKey({ platform: 'riichi_city' })).toBeNull()
    expect(roomLabelKey({ platform: 'riichi_city', stage_type: 9 })).toEqual({
      key: 'history.room.raw',
      params: { id: 9 },
    })
  })

  it('returns null for missing match info', () => {
    expect(roomLabelKey(null)).toBeNull()
    expect(roomLabelKey(undefined)).toBeNull()
  })
})

describe('matchGameId / paifuUrl', () => {
  it('surfaces each platform game id', () => {
    expect(
      matchGameId({ platform: 'majsoul', game_uuid: '240101-uuid' }),
    ).toBe('240101-uuid')
    expect(
      matchGameId({ platform: 'tenhou', log_id: '2026010100gm-00a9-0000-cafe0001' }),
    ).toBe('2026010100gm-00a9-0000-cafe0001')
    expect(
      matchGameId({ platform: 'riichi_city', room_id: 'tabletoken0001' }),
    ).toBe('tabletoken0001')
  })

  it('builds replay links for Tenhou only, with the viewer seat when known', () => {
    expect(
      paifuUrl({ platform: 'tenhou', log_id: '2026010100gm-00a9-0000-cafe0001' }),
    ).toBe('https://tenhou.net/0/?log=2026010100gm-00a9-0000-cafe0001')
    expect(
      paifuUrl(
        { platform: 'tenhou', log_id: '2026010100gm-00a9-0000-cafe0001' },
        2,
      ),
    ).toBe('https://tenhou.net/0/?log=2026010100gm-00a9-0000-cafe0001&tw=2')
    expect(paifuUrl({ platform: 'majsoul', game_uuid: 'u' }, 0)).toBeNull()
    expect(paifuUrl({ platform: 'riichi_city', room_id: 'x' })).toBeNull()
  })
})

describe('locale coverage', () => {
  // Every key roomLabelKey can emit must resolve in every locale — a tier
  // added to a map without its strings would otherwise ship a raw i18n key
  // to the UI with no test failure.
  const emittableKeys = [
    ...['bronze', 'silver', 'gold', 'jade', 'throne', 'melee'].map(
      (t) => `majsoul_${t}`,
    ),
    'majsoul_friendly',
    'majsoul_contest',
    ...['ippan', 'joukyuu', 'tokujou', 'houou'].map((t) => `tenhou_${t}`),
    'tenhou_lobby',
    ...['star', 'moon', 'sun', 'galaxy'].map((t) => `rc_${t}`),
    'rc_ranked',
    'rc_tournament',
    'rc_friendly',
    'rc_one_round',
    'rc_two_player',
    'rc_ak_2p',
    'rc_command',
    'rc_chinitsu',
    'rc_taiwan',
    'rc_ai_challenge',
    'rc_chill',
    'rc_seventeen',
    'rc_practice',
    'rc_fury_waves',
    'rc_taiwan_ranked',
    'rc_hidden_war',
    'raw',
  ]

  it.each([
    ['en', en],
    ['ja', ja],
    ['zh-CN', zhCN],
    ['zh-TW', zhTW],
  ])('%s has every emittable history.room key', (_name, locale) => {
    const room = (locale as { history: { room: Record<string, string> } })
      .history.room
    for (const key of emittableKeys) {
      expect(room[key], `history.room.${key}`).toBeTypeOf('string')
    }
  })
})

describe('gameKind', () => {
  it('tells ranked tables, friendly rooms and events apart', () => {
    expect(gameKind({ platform: 'majsoul', mode_id: 21, category: 2 })).toBe('ranked')
    expect(gameKind({ platform: 'majsoul', room_id: 92939, category: 1 })).toBe('friendly')
    // Older records: no category, a room number and no matchmode.
    expect(gameKind({ platform: 'majsoul', room_id: 92939 })).toBe('friendly')
    expect(gameKind({ platform: 'majsoul', contest_uid: 5 })).toBe('event')
    expect(gameKind({ platform: 'majsoul', mode_id: 40, category: 2, rules: ['chuanma'] })).toBe('event')
    expect(gameKind({ platform: 'majsoul', mode_id: 40, category: 4 })).toBe('event')
    expect(gameKind({ platform: 'tenhou', lobby: 1234 })).toBe('friendly')
    expect(gameKind({ platform: 'tenhou', lobby: 0, go_type: 0xa9 })).toBe('ranked')
    expect(gameKind({ platform: 'riichi_city', game_play: 1003 })).toBe('friendly')
    expect(gameKind({ platform: 'riichi_city', game_play: 1009 })).toBe('event')
    expect(gameKind({ platform: 'riichi_city', game_play: 1001 })).toBe('ranked')
    // Nothing to go on: counted as ranked.
    expect(gameKind(null)).toBe('ranked')
    expect(gameKind({ platform: 'majsoul' })).toBe('ranked')
  })

  it('labels an event table as an event, not by its matchmode', () => {
    expect(
      roomLabelKey({ platform: 'majsoul', mode_id: 40, category: 2, rules: ['wanxiangxiuluo_mode'] }),
    ).toEqual({ key: 'history.room.majsoul_event' })
  })
})
