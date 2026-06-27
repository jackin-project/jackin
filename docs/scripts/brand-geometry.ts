// Single source of truth for the jackin❯ wordmark geometry. Every surface —
// the DOM lockup (src/components/brand/brand-mark.svg), the OG cards, and the
// readme-hero — derives the chevron from these ratios, so the mark is identical
// everywhere and cannot drift. The terminal pill is the one exception: a TTY
// can't draw paths, so it falls back to the ❯ glyph (a proven medium limit).
//
// Ratios are derived from the original readme-hero caret (height 148, width 74,
// stroke 24 at a "jackin" font-size of 138) so that surface is unchanged while
// the smaller DOM lockup now matches it exactly.

export const FONT_STACK =
  'JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace'

const CHEVRON_HEIGHT_RATIO = 148 / 138 // chevron height ÷ font-size
const CHEVRON_WIDTH_RATIO = 74 / 148 // chevron width ÷ chevron height
const CHEVRON_STROKE_RATIO = 24 / 148 // stroke ÷ chevron height
const WORD_ADVANCE = 0.6 // JetBrains Mono advance per char (÷ font-size)
const WORD_CHEVRON_GAP = 0.22 // gap between word and chevron (÷ font-size)

const round = (n: number) => Math.round(n * 100) / 100

export type Caret = { d: string; strokeWidth: number; right: number }

/** Open chevron caret whose left edge is at `left`, centered on `cy`. */
export function caret(fontSize: number, left: number, cy: number): Caret {
  const height = fontSize * CHEVRON_HEIGHT_RATIO
  const half = height / 2
  const apex = left + height * CHEVRON_WIDTH_RATIO
  const strokeWidth = height * CHEVRON_STROKE_RATIO
  return {
    d: `M${round(left)} ${round(cy - half)} L${round(apex)} ${round(cy)} L${round(left)} ${round(cy + half)}`,
    strokeWidth: round(strokeWidth),
    right: round(apex + strokeWidth / 2),
  }
}

export const wordWidth = (fontSize: number, chars: number) => round(fontSize * WORD_ADVANCE * chars)
export const wordChevronGap = (fontSize: number) => fontSize * WORD_CHEVRON_GAP

/** Standalone caret SVG (its own tight viewBox) for embedding as an <img>. */
export function caretSvg(fontSize: number, color: string): { svg: string; width: number; height: number } {
  const c = caret(fontSize, fontSize * CHEVRON_HEIGHT_RATIO * CHEVRON_STROKE_RATIO * 0.5, 0)
  const height = fontSize * CHEVRON_HEIGHT_RATIO + c.strokeWidth
  const cy = height / 2
  const placed = caret(fontSize, c.strokeWidth / 2, cy)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${placed.right} ${round(height)}"><path d="${placed.d}" fill="none" stroke="${color}" stroke-width="${placed.strokeWidth}" stroke-linecap="square" stroke-linejoin="miter"/></svg>`
  return { svg, width: placed.right, height: round(height) }
}

/** Square app-icon (favicon): the "j❯" monogram centered on a filled square,
 *  using the same caret primitive as the wordmark. */
export function faviconSvg(opts: { size?: number; bg: string; wordColor: string; chevronColor: string }): string {
  const size = opts.size ?? 512
  const fontSize = round(size * 0.5)
  const cy = round(size / 2)
  const ww = wordWidth(fontSize, 1) // "j"
  const gap = round(wordChevronGap(fontSize))
  const probe = caret(fontSize, 0, cy)
  const contentWidth = ww + gap + probe.right
  const startX = round((size - contentWidth) / 2)
  const c = caret(fontSize, round(startX + ww + gap), cy)
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" fill="${opts.bg}"/>
  <text x="${startX}" y="${cy}" textLength="${ww}" lengthAdjust="spacingAndGlyphs" dominant-baseline="central" font-family="${FONT_STACK}" font-size="${fontSize}" font-weight="600" fill="${opts.wordColor}">j</text>
  <path d="${c.d}" fill="none" stroke="${opts.chevronColor}" stroke-width="${c.strokeWidth}" stroke-linecap="square" stroke-linejoin="miter"/>
</svg>
`
}

/** Full "jackin❯" lockup SVG. Colors are passed in so the DOM can use theme
 *  CSS vars while raster surfaces pass fixed hex. */
export function lockupSvg(opts: {
  word?: string
  fontSize?: number
  fontWeight?: number
  fontFamily?: string
  wordColor: string
  chevronColor: string
  className?: string
}): string {
  const word = opts.word ?? 'jackin'
  const fontSize = opts.fontSize ?? 72
  const fontWeight = opts.fontWeight ?? 500
  const fontFamily = opts.fontFamily ?? FONT_STACK
  const caretHeight = fontSize * CHEVRON_HEIGHT_RATIO
  // Pad the viewBox by the stroke so the square caps aren't clipped top/bottom.
  const height = round(caretHeight + caretHeight * CHEVRON_STROKE_RATIO)
  const cy = round(height / 2)
  const ww = wordWidth(fontSize, word.length)
  const left = round(ww + wordChevronGap(fontSize))
  const c = caret(fontSize, left, cy)
  const cls = opts.className ? ` class="${opts.className}"` : ''
  return `<svg xmlns="http://www.w3.org/2000/svg"${cls} viewBox="0 0 ${c.right} ${height}" aria-hidden="true" preserveAspectRatio="xMinYMid meet">
  <text x="0" y="${cy}" textLength="${ww}" lengthAdjust="spacingAndGlyphs" dominant-baseline="central" font-family="${fontFamily}" font-size="${fontSize}" font-weight="${fontWeight}" fill="${opts.wordColor}">${word}</text>
  <path d="${c.d}" fill="none" stroke="${opts.chevronColor}" stroke-width="${c.strokeWidth}" stroke-linecap="square" stroke-linejoin="miter"/>
</svg>
`
}
