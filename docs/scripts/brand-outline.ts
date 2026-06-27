// Word outlining for the brand generators. The jackin❯ word is rendered as
// vector paths (JetBrains Mono Bold → outlines), never as SVG <text>, so every
// generated SVG is font-independent and identical on any surface.

import * as fontkit from 'fontkit'
import type { Font } from 'fontkit'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')

export const jbBoldData = readFileSync(
  join(root, 'node_modules', '@fontsource', 'jetbrains-mono', 'files', 'jetbrains-mono-latin-700-normal.woff'),
)
export const font = fontkit.create(jbBoldData) as Font

// Inter — used for the "by tailrocks" byline (a sans subtext, not the mono mark).
export const interData = readFileSync(
  join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-500-normal.woff'),
)
export const interFont = fontkit.create(interData) as Font

const round = (n: number) => Math.round(n * 100) / 100

export type OutlinedWord = {
  /** <g> of vector paths, baseline placed at y = its returned baseline. */
  group: string
  /** placement-ready: replace the group's translate to move it. */
  baseline: number
  /** advance width (includes the trailing side-bearing). */
  width: number
  /** visual ink right edge of the last glyph (no side-bearing). */
  inkRight: number
  bottom: number
  capCenter: number
}

/** Outline `text` at `fontSize`, filled with `fill`, in `face` (default mono bold). */
export function outlineWord(text: string, fontSize: number, fill: string, face: Font = font): OutlinedWord {
  const scale = fontSize / face.unitsPerEm
  const run = face.layout(text)
  let x = 0
  let maxY = -Infinity
  let minY = Infinity
  let right = 0
  let inkRight = 0
  const glyphs: string[] = []
  for (const g of run.glyphs) {
    const d = g.path.toSVG()
    if (d) glyphs.push(`<path transform="translate(${round(x)} 0)" d="${d}"/>`)
    const bb = g.path.bbox
    maxY = Math.max(maxY, bb.maxY)
    minY = Math.min(minY, bb.minY)
    if (d) inkRight = Math.max(inkRight, x + bb.maxX)
    right = x + g.advanceWidth
    x += g.advanceWidth
  }
  const baseline = round(maxY * scale)
  return {
    group: `<g fill="${fill}" transform="translate(0 ${baseline}) scale(${round(scale)} ${round(-scale)})">${glyphs.join('')}</g>`,
    baseline,
    width: round(right * scale),
    inkRight: round(inkRight * scale),
    bottom: round((maxY - minY) * scale),
    capCenter: round((maxY - face.capHeight / 2) * scale),
  }
}

/** Re-place an outlined word's group at (x, baseline). */
export function placeWord(word: OutlinedWord, x: number, baseline: number): string {
  return word.group.replace(/translate\(0 [0-9.-]+\)/, `translate(${round(x)} ${round(baseline)})`)
}
