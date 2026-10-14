



import * as fontkit from 'fontkit'
import type { Font } from 'fontkit'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')

export const jbBoldData = readFileSync(
  join(root, 'node_modules', '@fontsource', 'jetbrains-mono', 'files', 'jetbrains-mono-latin-700-normal.woff'),
)
export const font = fontkit.create(jbBoldData) as Font


export const interData = readFileSync(
  join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-500-normal.woff'),
)
export const interFont = fontkit.create(interData) as Font

const round = (n: number) => Math.round(n * 100) / 100

export type OutlinedWord = {

  group: string

  baseline: number

  width: number

  inkRight: number
  bottom: number
  capCenter: number
}


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


  const s = Math.round(scale * 1e6) / 1e6
  return {
    group: `<g fill="${fill}" transform="translate(0 ${baseline}) scale(${s} ${-s})">${glyphs.join('')}</g>`,
    baseline,
    width: round(right * scale),
    inkRight: round(inkRight * scale),
    bottom: round((maxY - minY) * scale),
    capCenter: round((maxY - face.capHeight / 2) * scale),
  }
}


export function placeWord(word: OutlinedWord, x: number, baseline: number): string {
  return word.group.replace(/translate\(0 [0-9.-]+\)/, `translate(${round(x)} ${round(baseline)})`)
}
