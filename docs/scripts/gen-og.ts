// Generates the committed social-preview cards with Takumi.

import ImageResponse from '@takumi-rs/image-response'
import React from 'react'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { ageToColor, createRainState, tickRain } from '../src/components/landing/rainEngine'

const root = join(import.meta.dirname, '..')

const interBold = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-800-normal.woff'))
const interRegular = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-500-normal.woff'))
const jbMono = readFileSync(join(root, 'node_modules', '@fontsource', 'jetbrains-mono', 'files', 'jetbrains-mono-latin-600-normal.woff'))

const BG = '#0a0a0a'
const TEXT = '#ffffff'
const MUTED = '#9ca8a1'
const ACCENT = '#5cf07a'
const RAIN_COLS = 80
const RAIN_ROWS = 24

function seeded(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s + 0x6d2b79f5) | 0
    let t = Math.imul(s ^ (s >>> 15), 1 | s)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function Rain({ width, height }: { width: number; height: number }) {
  const rng = seeded(0x1acc1a11)
  const state = createRainState(RAIN_COLS, RAIN_ROWS, rng)
  for (let i = 0; i < 80; i++) tickRain(state, rng)

  const cellW = width / RAIN_COLS
  const cellH = height / RAIN_ROWS
  const children: React.ReactNode[] = []

  for (let r = 0; r < RAIN_ROWS; r++) {
    for (let c = 0; c < RAIN_COLS; c++) {
      const cell = state.grid[r][c]
      if (!cell) continue
      const color = ageToColor(cell.age)
      if (!color) continue
      children.push(
        React.createElement(
          'div',
          {
            key: `${r}-${c}`,
            style: {
              position: 'absolute',
              left: c * cellW,
              top: r * cellH,
              width: cellW,
              height: cellH,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontFamily: 'JetBrainsMono',
              fontSize: 22,
              fontWeight: 600,
              color,
              opacity: c < 44 && r > 4 && r < 20 ? 0.12 : 0.9,
            },
          },
          cell.ch,
        ),
      )
    }
  }

  return React.createElement('div', { style: { position: 'absolute', inset: 0, display: 'flex' } }, children)
}

function Chevron({ size, stroke, marginLeft = 0 }: { size: number; stroke: number; marginLeft?: number }) {
  return React.createElement('div', {
    style: {
      display: 'flex',
      width: size,
      height: size,
      borderTop: `${stroke}px solid ${ACCENT}`,
      borderRight: `${stroke}px solid ${ACCENT}`,
      transform: 'rotate(45deg)',
      marginLeft,
      marginTop: size * 0.08,
    },
  })
}

function Card({ width, height }: { width: number; height: number }) {
  return React.createElement(
    'div',
    {
      style: {
        width: '100%',
        height: '100%',
        display: 'flex',
        position: 'relative',
        backgroundColor: BG,
        fontFamily: 'Inter',
      },
    },
    React.createElement(Rain, { width, height }),
    React.createElement(
      'div',
      {
        style: {
          position: 'absolute',
          inset: 0,
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'center',
          padding: 80,
        },
      },
      React.createElement(
        'div',
        { style: { display: 'flex', alignItems: 'center', gap: 14, marginBottom: 24 } },
        React.createElement('div', { style: { display: 'flex', width: 56, height: 2, backgroundColor: ACCENT } }),
        React.createElement(
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
          'OPERATOR TERMINAL',
        ),
      ),
      React.createElement(
        'div',
        {
          style: {
            display: 'flex',
            fontFamily: 'JetBrainsMono',
            fontSize: 142,
            fontWeight: 600,
            letterSpacing: 0,
            color: TEXT,
            lineHeight: 1,
            marginBottom: 12,
          },
        },
        React.createElement('span', { style: { display: 'flex' } }, 'jackin'),
        React.createElement(Chevron, { size: 74, stroke: 14, marginLeft: 6 }),
      ),
      React.createElement(
        'div',
        {
          style: {
            display: 'flex',
            fontFamily: 'JetBrainsMono',
            fontSize: 24,
            fontWeight: 600,
            color: MUTED,
            marginBottom: 46,
          },
        },
        'by tailrocks',
      ),
      React.createElement(
        'div',
        { style: { display: 'flex', flexDirection: 'column', letterSpacing: -0.6 } },
        React.createElement(
          'div',
          { style: { display: 'flex', fontWeight: 600, fontSize: 36, color: TEXT, lineHeight: 1.2 } },
          'Jack your AI coding agents',
          React.createElement('span', { style: { display: 'flex', color: ACCENT } }, ' in'),
          '.',
        ),
        React.createElement(
          'div',
          { style: { display: 'flex', marginTop: 14, fontWeight: 500, fontSize: 26, color: MUTED, lineHeight: 1.35 } },
          'Isolated worlds. Scoped access. Full autonomy.',
        ),
        React.createElement(
          'div',
          { style: { display: 'flex', marginTop: 8, fontWeight: 500, fontSize: 26, color: MUTED, lineHeight: 1.35 } },
          "You're the ",
          React.createElement('span', { style: { display: 'flex', color: ACCENT } }, 'Operator'),
          ". They're already inside.",
        ),
      ),
    ),
  )
}

async function generate(width: number, height: number, output: string) {
  const response = new ImageResponse(React.createElement(Card, { width, height }), {
    width,
    height,
    format: 'png',
    fonts: [
      { name: 'Inter', data: interBold, weight: 800, style: 'normal' },
      { name: 'Inter', data: interRegular, weight: 500, style: 'normal' },
      { name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' },
    ],
  })
  await response.ready
  const png = Buffer.from(await response.arrayBuffer())
  writeFileSync(output, png)
  console.log(`wrote ${output} (${png.byteLength.toLocaleString()} bytes)`)
}

const readmeHeroSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="640" viewBox="0 0 1280 640" role="img" aria-label="jackin❯ by tailrocks">
  <rect width="1280" height="640" fill="${BG}"/>
  <g font-family="JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" font-size="22" font-weight="600" fill="${ACCENT}" opacity="0.26">
    <text x="42" y="70">j a c k i n ❯ │ ┆ j ❯ │ ┇ j a c k i n ❯ │ ┆ j ❯</text>
    <text x="118" y="138">┆ j ❯ │ j a c k i n ❯ ┇ │ j ❯ │ j a c k i n ❯</text>
    <text x="78" y="514">j ❯ │ ┇ j a c k i n ❯ │ ┆ j ❯ │ j a c k i n ❯</text>
    <text x="172" y="582">│ j a c k i n ❯ ┆ j ❯ │ ┇ j a c k i n ❯ │</text>
  </g>
  <g font-family="JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" text-anchor="middle">
    <text x="580" y="322" font-size="138" font-weight="600" fill="${TEXT}">jackin</text>
    <path d="M836 214 916 292 836 370" fill="none" stroke="${ACCENT}" stroke-width="24" stroke-linecap="square" stroke-linejoin="miter"/>
    <text x="640" y="374" font-size="26" font-weight="500" fill="${MUTED}">by tailrocks</text>
  </g>
</svg>
`

writeFileSync(join(root, 'public', 'readme-hero.svg'), readmeHeroSvg)
console.log('wrote readme-hero.svg')

await generate(1200, 630, join(root, 'public', 'og-image.png'))
await generate(1280, 640, join(root, 'public', 'og-image-github.png'))
await generate(1280, 640, join(root, 'public', 'readme-hero.png'))
