// Generates the brand icon bundle (favicon.svg + PNG rasters + manifest).
//
// Renders the "j'" wordmark via satori (text → SVG with text converted
// to paths, Inter Black embedded) + resvg (SVG → PNG). Using the same
// pipeline as scripts/gen-og.ts means the icon matches the landing
// wordmark glyph-for-glyph, and the output SVG has no font dependency
// (text is baked into path data) so the browser doesn't need to know
// about Inter to render it.
//
// Outputs into docs/public/:
//   - favicon.svg           → modern browsers
//   - apple-touch-icon.png  → 180×180, iOS home-screen
//   - icon-192.png          → 192×192, Android home-screen
//   - icon-512.png          → 512×512, Android splash
//   - site.webmanifest      → PWA metadata
//
// Run manually when brand colours / typography change:
//   cd docs && bun run scripts/gen-icons.ts

import satori from 'satori'
import type { SatoriOptions } from 'satori'
import { Resvg } from '@resvg/resvg-js'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = join(__dirname, '..')
const pub = join(root, 'public')

const font = (pkg: string, file: string): Buffer =>
  readFileSync(join(root, 'node_modules', pkg, 'files', file))

// Static Inter 900 — the same hand-drawn Black used for the landing
// wordmark. Variable-axis 900 reads thinner and doesn't match.
const interBlack = font('@fontsource/inter', 'inter-latin-900-normal.woff')

// Palette — matches landing/styles.css tokens. Kept literal (not
// imported) so this script has zero runtime dependencies on the
// Astro/Tailwind pipeline.
const BG = '#0a0b0a'        // --landing-bg
const TEXT = '#f4f7f5'      // --landing-text
const ACCENT = '#00ff41'    // --landing-accent

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

// The icon tree: a black rounded square holding the "j'" wordmark in
// Inter Black, white "j" next to a Matrix-green apostrophe. Sized in
// px against a 512-unit canvas — larger rasters come from satori's
// output; smaller rasters are produced by resvg scaling.
//
// The negative letterSpacing mirrors the wordmark (-0.06em). Padding
// is tuned so glyphs fill the square without clipping the descender.
const tree = h(
  'div',
  {
    style: {
      width: '100%',
      height: '100%',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      backgroundColor: BG,
      fontFamily: 'Inter',
      fontWeight: 900,
      lineHeight: 1,
      // borderRadius on the top container is honoured by satori and
      // preserved when resvg rasterises.
      borderRadius: 80,
    },
  },
  h(
    'div',
    {
      style: {
        display: 'flex',
        alignItems: 'baseline',
        fontSize: 320,
        letterSpacing: -16,
        // The 'j' has a deep descender; baseline-align means the whole
        // pair sits low. Nudge up so the dot clears the top of the
        // rounded square and the hook clears the bottom.
        marginTop: -24,
      },
    },
    h('span', { style: { display: 'flex', color: TEXT } }, 'j'),
    h('span', { style: { display: 'flex', color: ACCENT } }, "'"),
  ),
)

const options: SatoriOptions = {
  width: 512,
  height: 512,
  fonts: [{ name: 'Inter', data: interBlack, weight: 900, style: 'normal' }],
}

const svg = await satori(tree as unknown as Parameters<typeof satori>[0], options)

// Write the SVG as-is for modern browsers. Satori output already has
// text converted to path data, so no font dependency at display time.
writeFileSync(join(pub, 'favicon.svg'), svg)
console.log(`wrote favicon.svg`)

async function raster(size: number, out: string): Promise<void> {
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: size } })
    .render()
    .asPng()
  writeFileSync(join(pub, out), png)
  console.log(`wrote ${out} (${size}×${size}, ${png.byteLength.toLocaleString()} bytes)`)
}

await raster(180, 'apple-touch-icon.png')
await raster(192, 'icon-192.png')
await raster(512, 'icon-512.png')

const manifest = {
  name: "jackin'",
  short_name: "jackin'",
  description:
    "Jack your AI coding agents into the Matrix. Their own isolated worlds, scoped access, full autonomy. You're the Operator. They're already inside.",
  start_url: '/',
  display: 'standalone',
  background_color: BG,
  theme_color: BG,
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/apple-touch-icon.png', sizes: '180x180', type: 'image/png' },
  ],
}

writeFileSync(join(pub, 'site.webmanifest'), JSON.stringify(manifest, null, 2) + '\n')
console.log('wrote site.webmanifest')
