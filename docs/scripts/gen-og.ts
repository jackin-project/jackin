// Generates two social-preview cards from the same layout:
//   - docs/public/og-image.png        — 1200×630 (OpenGraph / Facebook / Twitter standard)
//   - docs/public/og-image-github.png — 1280×640 (GitHub social-preview optimal 2:1)
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

// Grid sized so cells render ~22px tall at the canvas size.
// 80 × 24 keeps the character count ~2k max (well under satori's
// hard limit) and leaves enough density for the phosphor-rain feel.
// Per-cell width/height is derived from the target canvas dimensions.
const RAIN_COLS = 80
const RAIN_ROWS = 24
const RAIN_FONT_PX = 22

// Opacity inside the mask / at the soft edge / outside.
const MASK_INNER_OPACITY = 0.12
const MASK_OUTER_OPACITY = 1.0
// Mask threshold: d <= 1 is fully inside, d >= EDGE is fully outside.
const MASK_EDGE = 1.55

async function generateOgImage(
  width: number,
  height: number,
  outputPath: string,
): Promise<void> {
  const CELL_W = width / RAIN_COLS
  const CELL_H = height / RAIN_ROWS

  // Re-seed identically for each call so both outputs share the same
  // deterministic rain layout. State must also be fresh per call —
  // sharing would let the first render's ticks bleed into the second.
  const rainRng = makeSeededRng(0x1acc1a11)
  const rainState = createRainState(RAIN_COLS, RAIN_ROWS, rainRng)
  // 80 ticks is well past the warm-up — columns have fully advanced
  // and natural gaps are in place.
  for (let i = 0; i < 80; i++) tickRain(rainState, rainRng)

  // Per-cell opacity mask so the rain fades out under the text block
  // instead of needing a dark overlay on top. An elliptical soft mask
  // centred on the wordmark/tagline region: cells inside the ellipse
  // render near-invisible, cells outside stay at full brightness, with
  // a smooth transition between the two so the edge doesn't read as a
  // hard cutout.
  const TEXT_CENTER_X = width * 0.32
  const TEXT_CENTER_Y = height * 0.56
  const TEXT_RADIUS_X = width * 0.42
  const TEXT_RADIUS_Y = height * 0.4

  function cellOpacity(cx: number, cy: number): number {
    const dx = (cx - TEXT_CENTER_X) / TEXT_RADIUS_X
    const dy = (cy - TEXT_CENTER_Y) / TEXT_RADIUS_Y
    const d = Math.sqrt(dx * dx + dy * dy)
    if (d <= 1) return MASK_INNER_OPACITY
    if (d >= MASK_EDGE) return MASK_OUTER_OPACITY
    // Smoothstep between inner and outer — cheap cubic ease so the
    // transition band reads as atmospheric blur, not a sharp ring.
    const t = (d - 1) / (MASK_EDGE - 1)
    const smooth = t * t * (3 - 2 * t)
    return MASK_INNER_OPACITY + (MASK_OUTER_OPACITY - MASK_INNER_OPACITY) * smooth
  }

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
      const cx = c * CELL_W + CELL_W / 2
      const cy = r * CELL_H + CELL_H / 2
      const op = cellOpacity(cx, cy)
      // Tiny glyphs with opacity ~0.03 aren't worth rendering — skip
      // them so the satori tree stays as small as possible.
      if (op < 0.05) continue
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
              opacity: op,
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
      width: width,
      height: height,
      fonts: [
        { name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' },
      ],
    },
  )

  const rainPng = new Resvg(rainSvg, { fitTo: { mode: 'width', value: width } })
    .render()
    .asPng()
  const rainDataUri = `data:image/png;base64,${rainPng.toString('base64')}`

  // ─── Foreground card ────────────────────────────────────────────────

  // Layer stack (back → front):
  //   1. Solid black base
  //   2. Rain PNG — already pre-masked so cells under the text block
  //      are rendered near-invisible and cells outside stay bright.
  //      That replaces the old "bright rain + dark overlay" approach,
  //      which had to dim the rain everywhere to protect text legibility.
  //   3. The actual content (label / wordmark / tagline)
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
    // Rain layer — opacity mask is baked into the PNG per-cell.
    h('div', {
      style: {
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: 'flex',
        backgroundImage: `url("${rainDataUri}")`,
        backgroundSize: `${width}px ${height}px`,
        backgroundRepeat: 'no-repeat',
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
          // "in" is the accent word — its double duty (preposition + the
          // inside/outside boundary that's the brand concept) carries the
          // line with a neutral, ownable end-word.
          'Jack your AI coding agents',
          h('span', { style: { display: 'flex', color: '#00ff41' } }, ' in'),
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
          'Isolated worlds. Scoped access. Full autonomy.'
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
          h('span', { style: { display: 'flex', color: '#00ff41' } }, ' Operator'),
          ". They're already inside."
        )
      )
    )
  )

  const options: SatoriOptions = {
    width: width,
    height: height,
    fonts: [
      { name: 'Inter', data: interBold, weight: 800, style: 'normal' },
      { name: 'Inter', data: interRegular, weight: 500, style: 'normal' },
      { name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' },
    ],
  }

  // Satori accepts React-style nodes; our plain object tree matches the
  // structural shape so a type-assertion is safe here.
  const svg = await satori(tree as unknown as Parameters<typeof satori>[0], options)

  const png = new Resvg(svg, { fitTo: { mode: 'width', value: width } })
    .render()
    .asPng()

  mkdirSync(dirname(outputPath), { recursive: true })
  writeFileSync(outputPath, png)
  console.log(`wrote ${outputPath} (${png.byteLength.toLocaleString()} bytes)`)
}

await generateOgImage(1200, 630, join(root, 'public', 'og-image.png'))
await generateOgImage(1280, 640, join(root, 'public', 'og-image-github.png'))
