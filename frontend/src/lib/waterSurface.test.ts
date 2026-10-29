import { describe, expect, it } from 'vitest'

import { COLUMNS, WaterSurface } from '@/lib/waterSurface'

function run(water: WaterSurface, seconds: number) {
  for (let t = 0; t < seconds; t += 1 / 60) water.advance(1 / 60)
}

describe('WaterSurface', () => {
  it('lies perfectly still when nothing disturbs it', () => {
    const water = new WaterSurface(40)
    expect(water.settled).toBe(true)
    run(water, 2)
    expect(Array.from(water.position).every((x) => x === 40)).toBe(true)
  })

  it('rings after a splash, the disturbance running along the surface, then settles', () => {
    const water = new WaterSurface(50)
    water.splash(0.5, 60)
    expect(water.settled).toBe(false)
    // At first only around the splash.
    const edge = () => Math.abs(water.position[0] - 50)
    run(water, 0.05)
    const early = edge()
    // Then it has travelled to the far ends.
    run(water, 0.6)
    expect(Math.abs(water.position[COLUMNS - 1] - 50) + edge()).toBeGreaterThan(early)
    // And dies away to nothing.
    run(water, 8)
    expect(water.settled).toBe(true)
    water.rest()
    expect(Math.max(...Array.from(water.position).map((x) => Math.abs(x - 50)))).toBe(0)
  })

  it('swings both ways, as water does, before it calms', () => {
    const water = new WaterSurface(50)
    water.splash(0.5, 60)
    let up = false
    let down = false
    for (let t = 0; t < 3; t += 1 / 60) {
      water.advance(1 / 60)
      const off = water.position[Math.floor(COLUMNS / 2)] - 50
      if (off > 0.2) up = true
      if (off < -0.2) down = true
    }
    expect(up && down).toBe(true)
  })

  it('follows a new level on its springs and ends exactly on it', () => {
    const water = new WaterSurface(30)
    water.setLevel(70)
    run(water, 0.05)
    expect(water.position[10]).toBeGreaterThan(30)
    expect(water.position[10]).toBeLessThan(70)
    run(water, 8)
    expect(water.settled).toBe(true)
    expect(water.position[10]).toBeCloseTo(70, 2)
  })

  it('stays inside the vial and never blows up', () => {
    const water = new WaterSurface(98)
    for (let i = 0; i < 20; i++) {
      water.splash(Math.random(), 500)
      run(water, 0.1)
    }
    expect(Array.from(water.position).every((x) => Number.isFinite(x))).toBe(true)
    const path = water.path()
    for (const x of path.matchAll(/L([\d.]+),/g)) expect(Number(x[1])).toBeLessThanOrEqual(100)
  })

  it('draws a closed path of the whole height', () => {
    const path = new WaterSurface(25).path()
    expect(path.startsWith('M0,0 L25.00,0.00')).toBe(true)
    expect(path.endsWith('L0,100Z')).toBe(true)
    expect(path.split(' ').length).toBe(COLUMNS + 2)
  })
})
