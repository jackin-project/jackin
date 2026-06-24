// Generates the brand icon bundle.
//
// PNG rasters are rendered through Takumi, matching the docs OG pipeline.
// The SVG favicon is static markup because browsers can display it directly.

import ImageResponse from '@takumi-rs/image-response'
import React from 'react'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')
const pub = join(root, 'public')

const jbMono = readFileSync(join(root, 'node_modules', '@fontsource', 'jetbrains-mono', 'files', 'jetbrains-mono-latin-500-normal.woff'))

// The mark is always a phosphor-green block with black letters and a white
// chevron. Square corners only — terminals cannot round, so neither do we.
const BLOCK = '#00ff41'
const INK = '#0a0a0a'
const CHEVRON = '#ffffff'

function Chevron({ size, stroke }: { size: number; stroke: number }) {
  return React.createElement('div', {
    style: {
      display: 'flex',
      width: size,
      height: size,
      borderTop: `${stroke}px solid ${CHEVRON}`,
      borderRight: `${stroke}px solid ${CHEVRON}`,
      transform: 'rotate(45deg)',
      marginLeft: -18,
      marginTop: 8,
    },
  })
}

function Icon() {
  return React.createElement(
    'div',
    {
      style: {
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: BLOCK,
        fontFamily: 'JetBrainsMono',
        fontWeight: 500,
        lineHeight: 1,
        borderRadius: 0,
      },
    },
    React.createElement(
      'div',
      {
        style: {
          display: 'flex',
          alignItems: 'baseline',
          fontSize: 290,
          letterSpacing: 0,
          marginTop: -14,
        },
      },
      React.createElement('span', { style: { display: 'flex', color: INK } }, 'j'),
      React.createElement(Chevron, { size: 132, stroke: 24 }),
    ),
  )
}

async function raster(size: number, out: string) {
  const response = new ImageResponse(React.createElement(Icon), {
    width: size,
    height: size,
    format: 'png',
    fonts: [{ name: 'JetBrainsMono', data: jbMono, weight: 500, style: 'normal' }],
  })
  await response.ready
  const png = Buffer.from(await response.arrayBuffer())
  writeFileSync(join(pub, out), png)
  console.log(`wrote ${out} (${size}x${size}, ${png.byteLength.toLocaleString()} bytes)`)
}

function faviconIco(images: Buffer[]) {
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
    entry.writeUInt8(0, 2)
    entry.writeUInt8(0, 3)
    entry.writeUInt16LE(1, 4)
    entry.writeUInt16LE(32, 6)
    entry.writeUInt32LE(image.length, 8)
    entry.writeUInt32LE(offset, 12)
    offset += image.length
    return entry
  })

  return Buffer.concat([header, ...entries, ...images])
}

writeFileSync(
  join(pub, 'favicon.svg'),
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" fill="${BLOCK}"/><text x="220" y="324" text-anchor="middle" font-family="JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" font-size="290" font-weight="500" fill="${INK}">j</text><path d="M286 174 382 256 286 338" fill="none" stroke="${CHEVRON}" stroke-width="40" stroke-linecap="square" stroke-linejoin="miter"/></svg>\n`,
)
console.log('wrote favicon.svg')

const icoImages = await Promise.all([16, 32, 48].map(async (size) => {
  const response = new ImageResponse(React.createElement(Icon), {
    width: size,
    height: size,
    format: 'png',
    fonts: [{ name: 'JetBrainsMono', data: jbMono, weight: 500, style: 'normal' }],
  })
  await response.ready
  return Buffer.from(await response.arrayBuffer())
}))
writeFileSync(join(pub, 'favicon.ico'), faviconIco(icoImages))
console.log('wrote favicon.ico')

await raster(180, 'apple-touch-icon.png')
await raster(192, 'icon-192.png')
await raster(512, 'icon-512.png')

const manifest = {
  name: 'jackin❯',
  short_name: 'j❯',
  description:
    "Run AI coding agents at full speed inside isolated containers: scoped access, per-agent state, and host boundaries that stay visible.",
  start_url: '/',
  display: 'standalone',
  background_color: '#0a0a0a',
  theme_color: '#0a0a0a',
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/apple-touch-icon.png', sizes: '180x180', type: 'image/png' },
  ],
}

writeFileSync(join(pub, 'site.webmanifest'), JSON.stringify(manifest, null, 2) + '\n')
console.log('wrote site.webmanifest')
