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

const round = (n: number) => Math.round(n * 100) / 100

export type OutlinedWord = {
  /** <g> of vector paths, baseline placed at y = its returned baseline. */
  group: string
  /** placement-ready: replace the group's translate to move it. */
  baseline: number
  width: number
  bottom: number
  capCenter: number
}

/** Outline `text` in JetBrains Mono Bold at `fontSize`, filled with `fill`. */
export function outlineWord(text: string, fontSize: number, fill: string): OutlinedWord {
  const scale = fontSize / font.unitsPerEm
  const run = font.layout(text)
  let x = 0
  let maxY = -Infinity
  let minY = Infinity
  let right = 0
  const glyphs: string[] = []
  for (const g of run.glyphs) {
    const d = g.path.toSVG()
    if (d) glyphs.push(`<path transform="translate(${round(x)} 0)" d="${d}"/>`)
    const bb = g.path.bbox
    maxY = Math.max(maxY, bb.maxY)
    minY = Math.min(minY, bb.minY)
    right = x + g.advanceWidth
    x += g.advanceWidth
  }
  const baseline = round(maxY * scale)
  return {
    group: `<g fill="${fill}" transform="translate(0 ${baseline}) scale(${round(scale)} ${round(-scale)})">${glyphs.join('')}</g>`,
    baseline,
    width: round(right * scale),
    bottom: round((maxY - minY) * scale),
    capCenter: round((maxY - font.capHeight / 2) * scale),
  }
}

/** Re-place an outlined word's group at (x, baseline). */
export function placeWord(word: OutlinedWord, x: number, baseline: number): string {
  return word.group.replace(/translate\(0 [0-9.-]+\)/, `translate(${round(x)} ${round(baseline)})`)
}
