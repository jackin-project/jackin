// Single generator for the jackin❯ logo. Produces every representation of the
// mark from one geometry source (brand-geometry.ts):
//   - the DOM lockup  src/components/brand/brand-mark.svg  (themed CSS vars,
//     page webfont, scales by font-size; imported via ?raw by BrandMark)
//   - standalone assets under  public/brand/  as SVG and PNG, per style.
//
// Logo style: transparent background, white "jackin", green chevron. New styles
// are just entries in STYLES; new representations are SVG (string) + PNG (Takumi).

import ImageResponse from '@takumi-rs/image-response'
import React from 'react'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { caretSvg, faviconSvg, lockupSvg, wordChevronGap, wordWidth } from './brand-geometry'

const root = join(import.meta.dirname, '..')
const jbMono = readFileSync(
  join(root, 'node_modules', '@fontsource', 'jetbrains-mono', 'files', 'jetbrains-mono-latin-600-normal.woff'),
)

const WHITE = '#ffffff'
const GREEN = '#5cf07a'

// Variants = which glyphs the mark shows. Styles = colors. Each (variant × style)
// is emitted as SVG and PNG; the DOM lockup is a separate themed SVG.
const VARIANTS = [
  { name: 'wordmark', word: 'jackin' },
  { name: 'monogram', word: 'j' },
]
const STYLES = {
  // Canonical: white word + green chevron on a transparent background.
  default: { wordColor: WHITE, chevronColor: GREEN },
}

// 1. DOM lockup — theme-aware colors + the page mono webfont.
writeFileSync(
  join(root, 'src', 'components', 'brand', 'brand-mark.svg'),
  lockupSvg({
    wordColor: 'var(--jk-text)',
    chevronColor: 'var(--jk-brand)',
    fontFamily: 'var(--sl-font-mono)',
    fontWeight: 500,
    className: 'jk-brand-mark__svg',
  }),
)
console.log('wrote src/components/brand/brand-mark.svg')

// 2. Favicon — j❯ monogram on a dark square (an icon needs a background).
writeFileSync(
  join(root, 'public', 'favicon.svg'),
  faviconSvg({ bg: '#0a0a0a', wordColor: WHITE, chevronColor: GREEN }),
)
console.log('wrote public/favicon.svg')

// 3. Standalone assets — SVG + PNG, transparent background.
const brandDir = join(root, 'public', 'brand')
mkdirSync(brandDir, { recursive: true })

const PNG_FONT_SIZE = 220

async function renderPng(word: string, wordColor: string, chevronColor: string): Promise<Buffer> {
  const fontSize = PNG_FONT_SIZE
  const fontWeight = 600
  const chev = caretSvg(fontSize, chevronColor)
  const gap = wordChevronGap(fontSize)
  const width = Math.ceil(wordWidth(fontSize, word.length) + gap + chev.width)
  const height = Math.ceil(chev.height)

  const element = React.createElement(
    'div',
    {
      style: {
        display: 'flex',
        alignItems: 'center',
        width: '100%',
        height: '100%',
        // Transparent background — no backgroundColor set.
      },
    },
    React.createElement(
      'span',
      {
        style: {
          display: 'flex',
          fontFamily: 'JetBrainsMono',
          fontSize,
          fontWeight,
          color: wordColor,
          lineHeight: 1,
        },
      },
      word,
    ),
    React.createElement('img', {
      width: chev.width,
      height: chev.height,
      src: `data:image/svg+xml;base64,${Buffer.from(chev.svg).toString('base64')}`,
      style: { marginLeft: gap },
    }),
  )

  const response = new ImageResponse(element, {
    width,
    height,
    format: 'png',
    fonts: [{ name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' }],
  })
  await response.ready
  return Buffer.from(await response.arrayBuffer())
}

for (const [styleName, style] of Object.entries(STYLES)) {
  for (const variant of VARIANTS) {
    const base = `jackin-${variant.name}${styleName === 'default' ? '' : `-${styleName}`}`

    const svg = lockupSvg({
      word: variant.word,
      fontSize: 200,
      fontWeight: 600,
      wordColor: style.wordColor,
      chevronColor: style.chevronColor,
    })
    writeFileSync(join(brandDir, `${base}.svg`), svg)
    console.log(`wrote public/brand/${base}.svg`)

    const png = await renderPng(variant.word, style.wordColor, style.chevronColor)
    writeFileSync(join(brandDir, `${base}.png`), png)
    console.log(`wrote public/brand/${base}.png (${png.byteLength.toLocaleString()} bytes)`)
  }
}

// 4. App-icon bundle (favicon.ico, apple-touch, PWA icons) — the j❯ monogram on
//    a dark square, rendered from the same geometry as favicon.svg.
const ICON_BG = '#0a0a0a'

function iconElement(size: number) {
  const fontSize = Math.round(size * 0.5)
  const chev = caretSvg(fontSize, GREEN)
  return React.createElement(
    'div',
    {
      style: {
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: ICON_BG,
      },
    },
    React.createElement(
      'span',
      { style: { display: 'flex', fontFamily: 'JetBrainsMono', fontSize, fontWeight: 600, color: WHITE, lineHeight: 1 } },
      'j',
    ),
    React.createElement('img', {
      width: chev.width,
      height: chev.height,
      src: `data:image/svg+xml;base64,${Buffer.from(chev.svg).toString('base64')}`,
      style: { marginLeft: wordChevronGap(fontSize) },
    }),
  )
}

async function iconPng(size: number): Promise<Buffer> {
  const response = new ImageResponse(iconElement(size), {
    width: size,
    height: size,
    format: 'png',
    fonts: [{ name: 'JetBrainsMono', data: jbMono, weight: 600, style: 'normal' }],
  })
  await response.ready
  return Buffer.from(await response.arrayBuffer())
}

function faviconIco(images: Buffer[]): Buffer {
  const header = Buffer.alloc(6)
  header.writeUInt16LE(0, 0)
  header.writeUInt16LE(1, 2)
  header.writeUInt16LE(images.length, 4)
  let offset = 6 + images.length * 16
  const entries = images.map((image) => {
    const entry = Buffer.alloc(16)
    const size = image.readUInt32BE(16)
    entry.writeUInt8(size === 256 ? 0 : size, 0)
    entry.writeUInt8(size === 256 ? 0 : size, 1)
    entry.writeUInt16LE(1, 4)
    entry.writeUInt16LE(32, 6)
    entry.writeUInt32LE(image.length, 8)
    entry.writeUInt32LE(offset, 12)
    offset += image.length
    return entry
  })
  return Buffer.concat([header, ...entries, ...images])
}

const pub = join(root, 'public')
const ico = await Promise.all([16, 32, 48].map((s) => iconPng(s)))
writeFileSync(join(pub, 'favicon.ico'), faviconIco(ico))
console.log('wrote public/favicon.ico')

for (const [name, size] of [
  ['apple-touch-icon.png', 180],
  ['icon-192.png', 192],
  ['icon-512.png', 512],
] as const) {
  const png = await iconPng(size)
  writeFileSync(join(pub, name), png)
  console.log(`wrote public/${name} (${size}x${size}, ${png.byteLength.toLocaleString()} bytes)`)
}

const manifest = {
  name: 'jackin❯',
  short_name: 'j❯',
  description:
    'Run AI coding agents at full speed inside isolated containers: scoped access, per-agent state, and host boundaries that stay visible.',
  start_url: '/',
  display: 'standalone',
  background_color: ICON_BG,
  theme_color: ICON_BG,
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/apple-touch-icon.png', sizes: '180x180', type: 'image/png' },
  ],
}
writeFileSync(join(pub, 'site.webmanifest'), `${JSON.stringify(manifest, null, 2)}\n`)
console.log('wrote public/site.webmanifest')
