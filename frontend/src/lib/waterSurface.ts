/**
 * The front of a liquid, as a row of columns on springs: the classic
 * height-field water model (a column is pulled to its rest level by a spring,
 * slowed by damping, and drags its neighbours along). A splash lifts one place
 * and the disturbance runs along the surface, rings, and dies away; with
 * nothing disturbing it the surface is perfectly still.
 *
 * Here the "surface" is the vertical edge of liquid lying in a vial: `level`
 * is how far across (percent of the width) it reaches, and each column is a
 * slice of the edge from top to bottom.
 */

/** Columns along the edge. */
export const COLUMNS = 48
/** Pull of a column back to the level, per second squared. */
const STIFFNESS = 70
/** Slowing, per second. About critical for a calm, brief ring-out. */
const DAMPING = 4.2
/** How strongly a column drags its neighbours (columns per second, squared). */
const SPREAD = 900
/** The integration step: small, so the neighbour coupling stays stable. */
const STEP = 1 / 240
/** Where a column counts as at rest, and the spring's own length scale. */
const REST_DISTANCE = 0.004
const REST_SPEED = 0.02
/** Never wider than this much of the vial from the level. */
const MAX_OFFSET = 14

export class WaterSurface {
  readonly position = new Float64Array(COLUMNS)
  readonly velocity = new Float64Array(COLUMNS)
  private target: number
  private carry = 0

  constructor(level: number) {
    this.target = level
    this.position.fill(level)
  }

  get level(): number {
    return this.target
  }

  /** Where the liquid should reach; the surface follows on its springs. */
  setLevel(level: number): void {
    this.target = level
  }

  /**
   * Kick the surface at `at` (0 top .. 1 bottom): `speed` is the push, in
   * percent of the vial per second, spread over a few columns around it.
   */
  splash(at: number, speed: number): void {
    const centre = Math.min(1, Math.max(0, at)) * (COLUMNS - 1)
    for (let i = 0; i < COLUMNS; i++) {
      const d = (i - centre) / 3
      this.velocity[i] += speed * Math.exp(-d * d)
    }
  }

  /** Move the water on by `seconds` of its own time. */
  advance(seconds: number): void {
    this.carry += Math.min(seconds, 1 / 20)
    while (this.carry >= STEP) {
      this.carry -= STEP
      this.step()
    }
  }

  private step(): void {
    const { position: p, velocity: v } = this
    // Neighbours first, from the positions as they stand, so the order of
    // the columns does not matter.
    for (let i = 0; i < COLUMNS; i++) {
      const left = p[i === 0 ? 0 : i - 1]
      const right = p[i === COLUMNS - 1 ? i : i + 1]
      const acceleration =
        STIFFNESS * (this.target - p[i]) - DAMPING * v[i] + SPREAD * (left + right - 2 * p[i])
      v[i] += acceleration * STEP
    }
    for (let i = 0; i < COLUMNS; i++) {
      p[i] += v[i] * STEP
      const off = p[i] - this.target
      if (off > MAX_OFFSET) p[i] = this.target + MAX_OFFSET
      else if (off < -MAX_OFFSET) p[i] = this.target - MAX_OFFSET
    }
  }

  /** Nothing moves: no need to draw or compute until something disturbs it. */
  get settled(): boolean {
    for (let i = 0; i < COLUMNS; i++) {
      if (
        Math.abs(this.position[i] - this.target) > REST_DISTANCE ||
        Math.abs(this.velocity[i]) > REST_SPEED
      ) {
        return false
      }
    }
    return true
  }

  /** Lie flat at the level, exactly. */
  rest(): void {
    this.position.fill(this.target)
    this.velocity.fill(0)
  }

  /** The liquid as an SVG path on a 100 x 100 box, reaching from the left. */
  path(shift = 0): string {
    const parts: string[] = ['M0,0']
    for (let i = 0; i < COLUMNS; i++) {
      const x = Math.min(100, Math.max(0, this.position[i] + shift))
      parts.push(`L${x.toFixed(2)},${((i / (COLUMNS - 1)) * 100).toFixed(2)}`)
    }
    parts.push('L0,100Z')
    return parts.join(' ')
  }
}
