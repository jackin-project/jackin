// Generates the favicon raster bundle + web app manifest.
// Sources: the existing favicon.svg in public/ (single SVG favicon).
// Outputs:
//   - apple-touch-icon.png (180×180)
//   - icon-192.png (192×192)  — Android home-screen
//   - icon-512.png (512×512)  — Android home-screen / splash
//   - site.webmanifest         — PWA metadata
//
// Run manually when the source favicon or brand copy changes:
//   bun run gen-icons

import sharp from 'sharp'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = fileURLToPath(new URL('.', import.meta.url))
const pub = join(__dirname, '..', 'public')

const svg = readFileSync(join(pub, 'favicon.svg'))

async function render(size, out) {
  await sharp(svg, { density: 300 })
    .resize(size, size, {
      fit: 'contain',
      background: { r: 10, g: 11, b: 10, alpha: 1 }, // jk-bg
    })
    .png({ compressionLevel: 9 })
    .toFile(join(pub, out))
  console.log(`wrote ${out} (${size}×${size})`)
}

await render(180, 'apple-touch-icon.png')
await render(192, 'icon-192.png')
await render(512, 'icon-512.png')

const manifest = {
  name: "jackin'",
  short_name: "jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated Docker containers',
  start_url: '/',
  display: 'standalone',
  background_color: '#0a0b0a',
  theme_color: '#0a0b0a',
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/apple-touch-icon.png', sizes: '180x180', type: 'image/png' },
  ],
}

writeFileSync(join(pub, 'site.webmanifest'), JSON.stringify(manifest, null, 2) + '\n')
console.log('wrote site.webmanifest')
