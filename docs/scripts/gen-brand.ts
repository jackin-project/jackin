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
