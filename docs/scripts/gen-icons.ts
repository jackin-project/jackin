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

const interBlack = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-900-normal.woff'))

const BG = '#0a0b0a'
const TEXT = '#f4f7f5'
const ACCENT = '#00ff41'

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
        backgroundColor: BG,
        fontFamily: 'Inter',
        fontWeight: 900,
        lineHeight: 1,
        borderRadius: 80,
      },
    },
    React.createElement(
      'div',
      {
        style: {
          display: 'flex',
          alignItems: 'baseline',
          fontSize: 320,
          letterSpacing: -16,
          marginTop: -24,
        },
      },
      React.createElement('span', { style: { display: 'flex', color: TEXT } }, 'j'),
      React.createElement('span', { style: { display: 'flex', color: ACCENT } }, "'"),
    ),
  )
}

async function raster(size: number, out: string) {
  const response = new ImageResponse(React.createElement(Icon), {
    width: size,
    height: size,
    format: 'png',
    fonts: [{ name: 'Inter', data: interBlack, weight: 900, style: 'normal' }],
  })
  await response.ready
  const png = Buffer.from(await response.arrayBuffer())
  writeFileSync(join(pub, out), png)
  console.log(`wrote ${out} (${size}x${size}, ${png.byteLength.toLocaleString()} bytes)`)
}

writeFileSync(
  join(pub, 'favicon.svg'),
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="80" fill="${BG}"/><text x="256" y="330" text-anchor="middle" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="320" font-weight="900" letter-spacing="-16"><tspan fill="${TEXT}">j</tspan><tspan fill="${ACCENT}">'</tspan></text></svg>\n`,
)
console.log('wrote favicon.svg')

await raster(180, 'apple-touch-icon.png')
await raster(192, 'icon-192.png')
await raster(512, 'icon-512.png')

const manifest = {
  name: "jackin'",
  short_name: "jackin'",
  description:
    "Jack your AI coding agents in. Isolated worlds, scoped access, full autonomy. You're the Operator. They're already inside.",
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
