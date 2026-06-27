// Single source of truth for the jackin❯ wordmark geometry. Every surface —
// the DOM lockup (src/components/brand/brand-mark.svg), the favicon, the OG
// cards, and the readme-hero — derives the chevron from here, so the mark is
// identical everywhere and cannot drift.
//
// The chevron is the U+276F ❯ "heavy right-pointing angle quotation ornament",
// which is a plain straight-edged filled chevron (no curves). Its proportions
// are reconstructed from the glyph's own geometry, so we reproduce the exact ❯
// shape as a font-independent vector path — usable in raster surfaces where the
// ❯ codepoint isn't in JetBrains Mono. Ratios are ÷ the chevron height:
//   inner-left edge 0.2207, inner tip 0.3278, outer tip 0.5424; apex centered.

export const FONT_STACK =
  'JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace'

const CHEVRON_HEIGHT_RATIO = 0.72 // chevron height ÷ font-size (≈ cap height, sits at letter level)
const X_INNER_LEFT = 0.2207
const X_INNER_TIP = 0.3278
const X_OUTER_TIP = 0.5424
const WORD_ADVANCE = 0.6 // JetBrains Mono advance per char (÷ font-size)
const WORD_CHEVRON_GAP = 0.16 // gap between word and chevron (÷ font-size)

const round = (n: number) => Math.round(n * 100) / 100

export type Chevron = { d: string; right: number }

/** Filled ❯ chevron whose left edge is at `left`, centered on `cy`. */
export function chevron(fontSize: number, left: number, cy: number): Chevron {
  const h = fontSize * CHEVRON_HEIGHT_RATIO
  const half = h / 2
  const x = (r: number) => round(left + r * h)
  const yTop = round(cy - half)
  const yBot = round(cy + half)
  const yc = round(cy)
  const xl = round(left)
  const d =
    `M${xl} ${yBot} L${x(X_INNER_TIP)} ${yc} L${xl} ${yTop} ` +
    `L${x(X_INNER_LEFT)} ${yTop} L${x(X_OUTER_TIP)} ${yc} L${x(X_INNER_LEFT)} ${yBot} Z`
  return { d, right: x(X_OUTER_TIP) }
}

export const wordWidth = (fontSize: number, chars: number) => round(fontSize * WORD_ADVANCE * chars)
export const wordChevronGap = (fontSize: number) => fontSize * WORD_CHEVRON_GAP

/** Standalone chevron SVG (tight viewBox) for embedding as an <img>. */
export function chevronSvg(fontSize: number, color: string): { svg: string; width: number; height: number } {
  const h = round(fontSize * CHEVRON_HEIGHT_RATIO)
  const c = chevron(fontSize, 0, h / 2)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${c.right} ${h}"><path d="${c.d}" fill="${color}"/></svg>`
  return { svg, width: c.right, height: h }
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
  const height = round(fontSize * 1.05) // contain the word's ascenders/descenders
  const cy = round(height / 2)
  const ww = wordWidth(fontSize, word.length)
  const left = round(ww + wordChevronGap(fontSize))
  const c = chevron(fontSize, left, cy)
  const cls = opts.className ? ` class="${opts.className}"` : ''
  return `<svg xmlns="http://www.w3.org/2000/svg"${cls} viewBox="0 0 ${c.right} ${height}" aria-hidden="true" preserveAspectRatio="xMinYMid meet">
  <text x="0" y="${cy}" textLength="${ww}" lengthAdjust="spacingAndGlyphs" dominant-baseline="central" font-family="${fontFamily}" font-size="${fontSize}" font-weight="${fontWeight}" fill="${opts.wordColor}">${word}</text>
  <path d="${c.d}" fill="${opts.chevronColor}"/>
</svg>
`
}

/** Square app-icon (favicon): the "j❯" monogram centered on a filled square. */
export function faviconSvg(opts: { size?: number; bg: string; wordColor: string; chevronColor: string }): string {
  const size = opts.size ?? 512
  const fontSize = round(size * 0.5)
  const cy = round(size / 2)
  const ww = wordWidth(fontSize, 1) // "j"
  const gap = round(wordChevronGap(fontSize))
  const probe = chevron(fontSize, 0, cy)
  const contentWidth = ww + gap + probe.right
  const startX = round((size - contentWidth) / 2)
  const c = chevron(fontSize, round(startX + ww + gap), cy)
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" fill="${opts.bg}"/>
  <text x="${startX}" y="${cy}" textLength="${ww}" lengthAdjust="spacingAndGlyphs" dominant-baseline="central" font-family="${FONT_STACK}" font-size="${fontSize}" font-weight="600" fill="${opts.wordColor}">j</text>
  <path d="${c.d}" fill="${opts.chevronColor}"/>
</svg>
`
}
