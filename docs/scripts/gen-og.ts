// Generates docs/public/og-image.png — 1200×630 social-preview card.
// Uses satori (JSX-like tree → SVG with embedded fonts) + resvg-js
// (SVG → PNG) so the build works in any environment without relying on
// system-installed fonts.
//
// Run: `bun run gen-og`

import satori from 'satori'
import type { SatoriOptions } from 'satori'
import { Resvg } from '@resvg/resvg-js'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

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

// Satori's input is a JSX-like tree. Using a tiny `h` helper instead of
// JSX avoids needing a JSX transform just for this one script.
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

const tree = h(
  'div',
  {
    style: {
      width: '100%',
      height: '100%',
      display: 'flex',
      flexDirection: 'column',
      justifyContent: 'center',
      padding: 80,
      backgroundColor: '#0a0b0a',
      backgroundImage:
        'radial-gradient(rgba(244,247,245,0.05) 1px, transparent 1px)',
      backgroundSize: '28px 28px',
      fontFamily: 'Inter',
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
        fontSize: 200,
        letterSpacing: -10,
        color: '#f4f7f5',
        lineHeight: 1,
        marginBottom: 28,
      },
    },
    h('span', { style: { display: 'flex' } }, 'jackin'),
    h('span', { style: { display: 'flex', color: '#00ff41' } }, "'")
  ),
  // Tagline
  h(
    'div',
    {
      style: {
        display: 'flex',
        flexDirection: 'column',
        fontWeight: 500,
        fontSize: 36,
        color: '#9ca8a1',
        letterSpacing: -0.8,
        lineHeight: 1.3,
      },
    },
    h('div', { style: { display: 'flex' } }, 'CLI for orchestrating AI coding agents'),
    h('div', { style: { display: 'flex' } }, 'in isolated Docker containers.')
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
