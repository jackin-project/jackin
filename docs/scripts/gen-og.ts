// Generates docs/public/og-image.png — 1200×630 social-preview card.
// Uses satori (JSX-like tree → SVG with embedded fonts) + resvg-js
// (SVG → PNG) so the build works in any environment without relying on
// system-installed fonts.
//
// Run: `cd docs && bun run scripts/gen-og.ts`

import satori from 'satori'
import type { SatoriOptions } from 'satori'
import { Resvg } from '@resvg/resvg-js'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createRainState, tickRain, ageToColor } from '../src/components/landing/rainEngine'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = join(__dirname, '..')

const font = (pkg: string, file: string): Buffer =>
  readFileSync(join(root, 'node_modules', pkg, 'files', file))

const interBold = font('@fontsource/inter', 'inter-latin-800-normal.woff')
const interRegular = font('@fontsource/inter', 'inter-latin-500-normal.woff')
const jbMono = font(
  '@fontsource/jetbrains-mono',
  'jetbrains-mono-latin-600-normal.woff'
)

// Satori's input is a JSX-like tree. A tiny `h` helper avoids needing
// a JSX transform just for this one script. Hoisted up here so the
// rain generator (below) can use it before the foreground tree is
// defined further down.
type SatoriNode = {
  type: string
  props: Record<string, unknown> & { children?: unknown }
}

const h = (
  type: string,
  props: Record<string, unknown> = {},
  ...children: unknown[]
): SatoriNode => ({
  type,
  props: { ...props, children: children.flat().filter(Boolean) },
})

// ─── Digital rain background ────────────────────────────────────────
//
// Runs the exact rainEngine the landing uses (same char pool, same
// age-to-colour ramp, same column behaviour) against a seeded RNG so
// the generated card is deterministic across builds — otherwise the
// OG image would change on every regen for no reason. The engine is
// ticked enough frames to reach a populated steady state.

function makeSeededRng(seed: number): () => number {
  let s = seed >>> 0
  return function () {
    s = (s + 0x6d2b79f5) | 0
    let t = Math.imul(s ^ (s >>> 15), 1 | s)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

// Grid sized so cells render ~22px tall at the 1200×630 canvas.
// 80 × 24 keeps the character count ~2k max (well under satori's
// hard limit) and leaves enough density for the Matrix feel.
const RAIN_COLS = 80
const RAIN_ROWS = 24
const CELL_W = 1200 / RAIN_COLS
const CELL_H = 630 / RAIN_ROWS
const RAIN_FONT_PX = 22

const rainRng = makeSeededRng(0x1acc1a11)
const rainState = createRainState(RAIN_COLS, RAIN_ROWS, rainRng)
// 80 ticks is well past the warm-up — columns have fully advanced
// and natural gaps are in place.
for (let i = 0; i < 80; i++) tickRain(rainState, rainRng)

// Build the rain grid as a satori node tree — each cell is a
// positioned span. Routing it through satori means the output SVG
// has the glyphs baked into path data (no font dependency for the
// downstream resvg pass), matching how we render the main card.

const rainChildren: SatoriNode[] = []
for (let r = 0; r < RAIN_ROWS; r++) {
  for (let c = 0; c < RAIN_COLS; c++) {
    const cell = rainState.grid[r][c]
    if (!cell) continue
    const colour = ageToColor(cell.age)
    if (!colour) continue
    rainChildren.push(
      h(
        'div',
        {
          style: {
            position: 'absolute',
            left: `${c * CELL_W}px`,
            top: `${r * CELL_H}px`,
            width: `${CELL_W}px`,
            height: `${CELL_H}px`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontFamily: 'JetBrainsMono',
            fontSize: RAIN_FONT_PX,
            fontWeight: 600,
            color: colour,
          },
        },
        cell.ch,
      ),
    )
  }
}
const rainTree = h(
  'div',
  {
    style: {
      position: 'relative',
      width: '100%',
      height: '100%',
      display: 'flex',
      backgroundColor: '#0a0b0a',
    },
  },
  ...rainChildren,
)

const rainSvg = await satori(
  rainTree as unknown as Parameters<typeof satori>[0],
  {
    width: 1200,
    height: 630,
    fonts: [
      { name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' },
    ],
  },
)

const rainPng = new Resvg(rainSvg, { fitTo: { mode: 'width', value: 1200 } })
  .render()
  .asPng()
const rainDataUri = `data:image/png;base64,${rainPng.toString('base64')}`

// ─── Foreground card ────────────────────────────────────────────────

// Layer stack (back → front):
//   1. Solid black base
//   2. Rain PNG (deterministic Matrix drizzle)
//   3. Semi-transparent black overlay to knock the rain back so it
//      reads as atmosphere, not foreground noise
//   4. Radial vignette for light focus on the text block
//   5. The actual content (label / wordmark / tagline)
const tree = h(
  'div',
  {
    style: {
      width: '100%',
      height: '100%',
      display: 'flex',
      position: 'relative',
      backgroundColor: '#0a0b0a',
      fontFamily: 'Inter',
    },
  },
  // Rain layer
  h('div', {
    style: {
      position: 'absolute',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      display: 'flex',
      backgroundImage: `url("${rainDataUri}")`,
      backgroundSize: '1200px 630px',
      backgroundRepeat: 'no-repeat',
    },
  }),
  // Dim overlay — 62% black so the rain reads as mood, not content.
  h('div', {
    style: {
      position: 'absolute',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      display: 'flex',
      backgroundColor: 'rgba(10, 11, 10, 0.62)',
    },
  }),
  // Soft radial behind the text block so the tagline pops.
  h('div', {
    style: {
      position: 'absolute',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      display: 'flex',
      backgroundImage:
        'radial-gradient(ellipse at 30% 50%, rgba(10,11,10,0.4), transparent 70%)',
    },
  }),
  // Content
  h(
    'div',
    {
      style: {
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'center',
        padding: 80,
      },
    },
    // Label row: green dash + OPERATOR TERMINAL
    h(
      'div',
      {
        style: { display: 'flex', alignItems: 'center', gap: 14, marginBottom: 24 },
      },
      h('div', {
        style: { display: 'flex', width: 56, height: 2, backgroundColor: '#00ff41' },
      }),
      h(
        'div',
        {
          style: {
            display: 'flex',
            fontFamily: 'JetBrainsMono',
            fontSize: 18,
            color: '#5e6a64',
            letterSpacing: 4,
            fontWeight: 600,
          },
        },
        'OPERATOR TERMINAL'
      )
    ),
    // Wordmark
    h(
      'div',
      {
        style: {
          display: 'flex',
          fontWeight: 800,
          fontSize: 176,
          letterSpacing: -8,
          color: '#f4f7f5',
          lineHeight: 1,
          marginBottom: 22,
        },
      },
      h('span', { style: { display: 'flex' } }, 'jackin'),
      h('span', { style: { display: 'flex', color: '#00ff41' } }, "'")
    ),
    // Tagline — brand narrative, not implementation detail. Deliberately
    // says "isolated worlds" rather than "Docker containers" so the copy
    // stays honest when the VM backend lands. "Operator" echoes the
    // OPERATOR TERMINAL label above so the two accent beats rhyme.
    h(
      'div',
      {
        style: {
          display: 'flex',
          flexDirection: 'column',
          letterSpacing: -0.6,
        },
      },
      h(
        'div',
        {
          style: {
            display: 'flex',
            fontWeight: 600,
            fontSize: 36,
            color: '#f4f7f5',
            lineHeight: 1.2,
          },
        },
        // Satori trims whitespace between flex children; nbsp inside the
        // span preserves the visible gap without a CSS margin hack.
        'Jack your AI coding agents into the',
        h('span', { style: { display: 'flex', color: '#00ff41' } }, '\u00A0Matrix'),
        '.'
      ),
      h(
        'div',
        {
          style: {
            display: 'flex',
            marginTop: 14,
            fontWeight: 500,
            fontSize: 26,
            color: '#9ca8a1',
            lineHeight: 1.35,
          },
        },
        'Their own isolated worlds. Scoped access. Full autonomy.'
      ),
      h(
        'div',
        {
          style: {
            display: 'flex',
            marginTop: 8,
            fontWeight: 500,
            fontSize: 26,
            color: '#9ca8a1',
            lineHeight: 1.35,
          },
        },
        "You're the",
        h('span', { style: { display: 'flex', color: '#00ff41' } }, '\u00A0Operator'),
        ". They're already inside."
      )
    )
  )
)

const options: SatoriOptions = {
  width: 1200,
  height: 630,
  fonts: [
    { name: 'Inter', data: interBold, weight: 800, style: 'normal' },
    { name: 'Inter', data: interRegular, weight: 500, style: 'normal' },
    { name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' },
  ],
}

// Satori accepts React-style nodes; our plain object tree matches the
// structural shape so a type-assertion is safe here.
const svg = await satori(tree as unknown as Parameters<typeof satori>[0], options)

const png = new Resvg(svg, { fitTo: { mode: 'width', value: 1200 } })
  .render()
  .asPng()

const out = join(root, 'public', 'og-image.png')
mkdirSync(dirname(out), { recursive: true })
writeFileSync(out, png)
console.log(`wrote ${out} (${png.byteLength.toLocaleString()} bytes)`)
