// The Risk tile's colour scale for a deal-in %, shared with the model's
// danger tile. Moved here from `tiles/RiskChartTile.tsx` as it was.

// Map deal-in risk (%) to a hue via a piecewise-linear ramp anchored to the
// deal-in thresholds that matter for defence. The curve is deliberately
// non-linear: mixed_risk is a deal-in % whose decision-critical band sits in
// ~0..16%, so a plain linear 0..20 map left genuinely dangerous tiles (~13%)
// looking yellow-green and almost never reached red. The anchors line up with
// the engine's own risk table (src/analysis/data/risk.rs): suji ~2-5%, no-suji
// 3/7 ~9.5%, no-suji 5 ~12.8%, with dora/late/ippatsu/multi-tenpai pushing 16%+.
// Colours are inline styles (not Tailwind classes) since the hue is per tile.
const RISK_HUE_ANCHORS: ReadonlyArray<readonly [risk: number, hue: number]> = [
  [0, 225], // blue  — genbutsu / guaranteed safe
  [5, 130], // green — suji, low risk
  [10, 45], // amber — no-suji 3/7, moderate
  [16, 0], //  red   — no-suji 5 + dora, late, ippatsu, multi-tenpai
]

function riskHue(v: number): number {
  const a = RISK_HUE_ANCHORS
  if (v <= a[0][0]) return a[0][1]
  for (let i = 1; i < a.length; i++) {
    const [v0, h0] = a[i - 1]
    const [v1, h1] = a[i]
    if (v <= v1) return h0 + ((h1 - h0) * (v - v0)) / (v1 - v0)
  }
  return a[a.length - 1][1] // clamp >= last anchor (16%) to full red
}

export type RiskColor = { ring: string; glow: string; chip: string }

export function riskColor(value: number | null): RiskColor {
  if (value == null) {
    // No analysis yet: neutral grey, no hue.
    return { ring: 'hsl(0 0% 55%)', glow: 'hsl(0 0% 55% / 0.4)', chip: 'hsl(0 0% 45%)' }
  }
  const hue = riskHue(value)
  return {
    ring: `hsl(${hue} 80% 55%)`,
    glow: `hsl(${hue} 85% 55% / 0.5)`,
    // Darker so white text on the chip stays legible across the whole gradient.
    chip: `hsl(${hue} 70% 40%)`,
  }
}
