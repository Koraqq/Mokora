import { describe, expect, it } from 'vitest'

import { nextGameTargetLabel, restLeft } from './nextGame'

const t = (key: string) => key

describe('nextGameTargetLabel', () => {
  it('names a configured room and table', () => {
    expect(nextGameTargetLabel({ room: 'jade', players: null, rule: 'three_south' }, t)).toBe(
      'settings.autoplay.next_game_room_jade・settings.autoplay.next_game_rule_three_south',
    )
  })

  it('names the ladder when the table follows the player counts', () => {
    expect(nextGameTargetLabel({ room: 'gold', players: 3, rule: null }, t)).toBe(
      'settings.autoplay.next_game_room_gold・tile.next_game.ladder_3',
    )
  })

  it('says the room comes from the rank before it is known', () => {
    expect(nextGameTargetLabel({ room: null, players: 4, rule: null }, t)).toBe(
      'tile.next_game.room_by_rank・tile.next_game.ladder_4',
    )
  })
})

describe('restLeft', () => {
  it('rounds up to the second and pads', () => {
    expect(restLeft(65_001, 0)).toBe('1:06')
    expect(restLeft(5 * 60_000, 0)).toBe('5:00')
  })

  it('stops at zero', () => {
    expect(restLeft(1_000, 5_000)).toBe('0:00')
  })
})
