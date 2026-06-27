// Single generator for the jackin❯ logo. One geometry source (brand-geometry.ts)
// → one set of assets, used everywhere:
//   - public/brand/jackin-wordmark.svg  ← the canonical logo, imported by
//     BrandMark and used as the TOC wordmark. The word is OUTLINED (JetBrains
//     Mono Bold → vector paths) and the chevron is the ❯ vector path, so the
//     single file renders identically as inline SVG, <img>, or background —
//     with no dependence on a loaded webfont.
//   - public/brand/jackin-monogram.svg, *.png, favicon.svg/.ico, app icons.
//
// Logo style: transparent background, white "jackin" (bold), green ❯ chevron.
// To change the mark, edit this file or brand-geometry.ts and rerun `gen-brand`.

import { Resvg } from '@resvg/resvg-js'
import ImageResponse from '@takumi-rs/image-response'
import React from 'react'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { chevron, chevronHeight, chevronSvg, wordChevronGap } from './brand-geometry'
import { font, interData, interFont, jbBoldData, outlineWord, placeWord } from './brand-outline'

const root = join(import.meta.dirname, '..')

const WHITE = '#ffffff'
const GREEN = '#5cf07a'
const DARK = '#0a0a0a'
const GREY = '#9ca8a1'
const round = (n: number) => Math.round(n * 100) / 100

// The canonical lockup: outlined word + ❯ chevron, transparent background.
// With `byline`, a small grey "by tailrocks" (also outlined) sits beneath it.
function wordmarkSvg(word: string, fontSize: number, wordColor: string, chevronColor: string, byline = false): string {
  const w = outlineWord(word, fontSize, wordColor)
  const left = round(w.width + wordChevronGap(fontSize))
  const cy = round(w.capCenter + fontSize * 0.06) // nudge chevron down from the cap center
  const c = chevron(fontSize, left, cy)
  let width = c.right
  let height = round(Math.max(w.bottom, cy + chevronHeight(fontSize) / 2))
  let bylineMarkup = ''
  if (byline) {
    const bf = Math.round(fontSize * 0.28)
    const by = outlineWord('by tailrocks', bf, GREY, interFont) // sans subtext, not the mono mark
    // Tuck the byline right under the letters (off the baseline, not the "j"
    // descender) so it almost touches the word.
    const baseline = round(w.baseline + 0.18 * bf + by.baseline)
    const bylineX = round(w.inkRight - by.inkRight) // ink right edge aligns with the end of "n"
    bylineMarkup = `\n  ${placeWord(by, bylineX, baseline)}`
    height = round(Math.max(height, baseline + (by.bottom - by.baseline)))
  }
  const label = `${word}❯${byline ? ' by tailrocks' : ''}`
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${label}" preserveAspectRatio="xMinYMid meet">
  ${w.group}
  <path d="${c.d}" fill="${chevronColor}"/>${bylineMarkup}
</svg>
`
}

// Square app icon: outlined "j" + chevron centered on a filled square.
function faviconSvg(size: number): string {
  const fontSize = round(size * 0.46)
  const w = outlineWord('j', fontSize, WHITE)
  const gap = round(wordChevronGap(fontSize))
  const contentW = w.width + gap + chevron(fontSize, 0, 0).right
  const startX = round((size - contentW) / 2)
  const cy = round(size / 2)
  // Baseline so the "j" cap is vertically centered on the square.
  const baseline = round(cy + (font.capHeight / 2) * (fontSize / font.unitsPerEm))
  const wg = placeWord(w, startX, baseline)
  const c = chevron(fontSize, round(startX + w.width + gap), cy)
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" fill="${DARK}"/>
  ${wg}
  <path d="${c.d}" fill="${GREEN}"/>
</svg>
`
}

const brandDir = join(root, 'public', 'brand')
const pub = join(root, 'public')
mkdirSync(brandDir, { recursive: true })

// 1. Canonical wordmark + monogram (outlined SVG). Used by BrandMark and the TOC.
// Guarantee, by rasterizing the result, that the byline's right edge lines up
// with the end of the "n" (the actual rendered pixels — not font metrics, which
// can lie). Throws if the two right edges differ by more than `tolUnits`. This
// would have caught the scale-rounding bug that pushed the byline ~30 units out.
function assertBylineAligned(svg: string, tolUnits = 3) {
  const vbW = Number(svg.match(/viewBox="0 0 ([0-9.]+)/)?.[1])
  const { pixels, width, height } = new Resvg(svg, { fitTo: { mode: 'width', value: 2000 } }).render()
  const toVb = (px: number) => (px / width) * vbW
  // Classify pixels by color, not position: white = word, grey = byline, green =
  // chevron (ignored). Robust to chevron size / byline vertical placement.
  let wordRight = -1
  let bylineRight = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4
      if (pixels[i + 3] < 128) continue
      const r = pixels[i]
      const g = pixels[i + 1]
      const b = pixels[i + 2]
      const white = r > 225 && g > 225 && b > 225
      const green = g - r > 40 || g - b > 40
      const grey = !white && !green && r > 110 && r < 215 && Math.abs(r - g) < 35 && Math.abs(g - b) < 35
      if (white && x > wordRight) wordRight = x
      if (grey && x > bylineRight) bylineRight = x
    }
  }
  const delta = Math.abs(toVb(wordRight) - toVb(bylineRight))
  if (delta > tolUnits) {
    throw new Error(
      `byline misaligned by ${delta.toFixed(1)} units (word right ${toVb(wordRight).toFixed(1)}, byline right ${toVb(bylineRight).toFixed(1)})`,
    )
  }
  console.log(`✓ byline aligned to the word (Δ ${delta.toFixed(2)} units)`)
}

// Canonical wordmark carries the "by tailrocks" byline (shown on every logo).
// The monogram (used for square icons) stays byline-free.
const wordmark = wordmarkSvg('jackin', 200, WHITE, GREEN, true)
assertBylineAligned(wordmark)
writeFileSync(join(brandDir, 'jackin-wordmark.svg'), wordmark)
console.log('wrote public/brand/jackin-wordmark.svg')
writeFileSync(join(brandDir, 'jackin-monogram.svg'), wordmarkSvg('j', 200, WHITE, GREEN, false))
console.log('wrote public/brand/jackin-monogram.svg')

// 2. Favicon (outlined, renders without a webfont).
writeFileSync(join(pub, 'favicon.svg'), faviconSvg(512))
console.log('wrote public/favicon.svg')

// 3. PNG rasters via Takumi (embeds the bold font, so text is exact).
function wordmarkElement(word: string, fontSize: number, withBg: boolean, byline = false) {
  const chev = chevronSvg(fontSize, GREEN)
  const row = React.createElement(
    'div',
    { style: { display: 'flex', alignItems: 'center' } },
    React.createElement(
      'span',
      { style: { display: 'flex', fontFamily: 'JetBrainsMono', fontSize, fontWeight: 700, color: WHITE, lineHeight: 1 } },
      word,
    ),
    React.createElement('img', {
      width: chev.width,
      height: chev.height,
      src: `data:image/svg+xml;base64,${Buffer.from(chev.svg).toString('base64')}`,
      style: { marginLeft: wordChevronGap(fontSize), marginTop: Math.round(fontSize * 0.12) },
    }),
  )
  const children: React.ReactNode[] = [row]
  if (byline) {
    const w = outlineWord(word, fontSize, WHITE)
    children.push(
      React.createElement(
        'div',
        {
          style: {
            display: 'flex',
            fontFamily: 'Inter',
            fontSize: Math.round(fontSize * 0.28),
            fontWeight: 500,
            color: GREY,
            marginTop: -Math.round(fontSize * 0.06), // tuck up under the letters
            // Ink right edge aligns with the end of the word (past chevron + side-bearing).
            alignSelf: 'flex-end',
            marginRight: Math.round(chev.width + wordChevronGap(fontSize) + (w.width - w.inkRight)),
          },
        },
        'by tailrocks',
      ),
    )
  }
  return React.createElement(
    'div',
    {
      style: {
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        width: '100%',
        height: '100%',
        ...(withBg ? { backgroundColor: DARK } : {}),
      },
    },
    ...children,
  )
}

async function renderPng(element: React.ReactElement, width: number, height: number): Promise<Buffer> {
  const response = new ImageResponse(element, {
    width,
    height,
    format: 'png',
    fonts: [
      { name: 'JetBrainsMono', data: jbBoldData, weight: 700, style: 'normal' },
      { name: 'Inter', data: interData, weight: 500, style: 'normal' },
    ],
  })
  await response.ready
  return Buffer.from(await response.arrayBuffer())
}

for (const [name, word] of [['jackin-wordmark', 'jackin'], ['jackin-monogram', 'j']] as const) {
  const fontSize = 220
  const byline = word === 'jackin'
  const chev = chevronSvg(fontSize, GREEN)
  const width = Math.ceil(fontSize * 0.6 * word.length + wordChevronGap(fontSize) + chev.width)
  const height = Math.ceil(chev.height * 1.4 + (byline ? fontSize * 0.45 : 0))
  const png = await renderPng(wordmarkElement(word, fontSize, false, byline), width, height)
  writeFileSync(join(brandDir, `${name}.png`), png)
  console.log(`wrote public/brand/${name}.png (${png.byteLength.toLocaleString()} bytes)`)
}

// 4. App-icon bundle.
async function iconPng(size: number): Promise<Buffer> {
  return renderPng(wordmarkElement('j', Math.round(size * 0.5), true), size, size)
}

function faviconIco(images: Buffer[]): Buffer {
  const header = Buffer.alloc(6)
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

const ico = await Promise.all([16, 32, 48].map((s) => iconPng(s)))
writeFileSync(join(pub, 'favicon.ico'), faviconIco(ico))
console.log('wrote public/favicon.ico')

for (const [name, size] of [['apple-touch-icon.png', 180], ['icon-192.png', 192], ['icon-512.png', 512]] as const) {
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
  background_color: DARK,
  theme_color: DARK,
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/apple-touch-icon.png', sizes: '180x180', type: 'image/png' },
  ],
}
writeFileSync(join(pub, 'site.webmanifest'), `${JSON.stringify(manifest, null, 2)}\n`)
console.log('wrote public/site.webmanifest')
