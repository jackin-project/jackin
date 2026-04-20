// Generates docs/public/og-image.png — a 1200×630 social-preview card.
// Landing aesthetic: dark bg with a subtle dot grid, the jackin' wordmark
// (white text + green tick), and a one-line tagline.
//
// Run manually when the design changes:  bun run gen-og
// (see package.json -> "gen-og" script)

import sharp from 'sharp'
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const OUT = join(__dirname, '..', 'public', 'og-image.png')

const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <defs>
    <pattern id="dots" width="28" height="28" patternUnits="userSpaceOnUse">
      <circle cx="1" cy="1" r="1" fill="rgba(244,247,245,0.05)"/>
    </pattern>
    <radialGradient id="vignette" cx="50%" cy="38%" r="70%">
      <stop offset="0%" stop-color="transparent"/>
      <stop offset="100%" stop-color="rgba(5,6,5,0.55)"/>
    </radialGradient>
    <filter id="glow" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="8" result="blur"/>
      <feMerge>
        <feMergeNode in="blur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>

  <!-- dark base + dot grid + soft vignette -->
  <rect width="1200" height="630" fill="#0a0b0a"/>
  <rect width="1200" height="630" fill="url(#dots)"/>
  <rect width="1200" height="630" fill="url(#vignette)"/>

  <!-- accent hairline in top-left corner -->
  <rect x="80" y="128" width="56" height="2" fill="#00ff41"/>
  <text x="148" y="135" font-family="'JetBrains Mono', ui-monospace, monospace" font-size="18" fill="#5e6a64" letter-spacing="4" font-weight="600">JACKIN PROJECT</text>

  <!-- wordmark -->
  <text x="80" y="340" font-family="'Inter', system-ui, sans-serif" font-weight="900" font-size="200" fill="#f4f7f5" letter-spacing="-10">jackin<tspan fill="#00ff41" filter="url(#glow)">'</tspan></text>

  <!-- tagline -->
  <text x="80" y="420" font-family="'Inter', system-ui, sans-serif" font-weight="500" font-size="36" fill="#9ca8a1" letter-spacing="-0.8">CLI for orchestrating AI coding agents</text>
  <text x="80" y="470" font-family="'Inter', system-ui, sans-serif" font-weight="500" font-size="36" fill="#9ca8a1" letter-spacing="-0.8">in isolated Docker containers.</text>

  <!-- URL pinned at bottom-right -->
  <text x="1120" y="555" text-anchor="end" font-family="'JetBrains Mono', ui-monospace, monospace" font-size="20" fill="#5e6a64" letter-spacing="2">jackin.tailrocks.com</text>
</svg>`

mkdirSync(dirname(OUT), { recursive: true })
await sharp(Buffer.from(svg)).png({ compressionLevel: 9 }).toFile(OUT)
console.log(`wrote ${OUT}`)
