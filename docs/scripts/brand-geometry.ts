// Chevron geometry for the jackin❯ mark — the single definition of the ❯ shape.
// The word is outlined separately (brand-outline.ts); together they make the
// fully-vector lockup in gen-brand.ts. Never rendered as SVG <text>.
//
// The chevron is the U+276F ❯ "heavy right-pointing angle ornament": a plain
// straight-edged filled chevron. Its proportions are reconstructed from the
// glyph's own geometry (ratios ÷ chevron height): inner-left 0.2207, inner tip
// 0.3278, outer tip 0.5424; apex centered.

const CHEVRON_HEIGHT_RATIO = 0.72 // chevron height ÷ font-size (≈ cap height, sits at letter level)
const X_INNER_LEFT = 0.2207
const X_INNER_TIP = 0.3278
const X_OUTER_TIP = 0.5424
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

export const wordChevronGap = (fontSize: number) => fontSize * WORD_CHEVRON_GAP

/** Standalone chevron SVG (tight viewBox) for embedding as an <img>. */
export function chevronSvg(fontSize: number, color: string): { svg: string; width: number; height: number } {
  const h = round(fontSize * CHEVRON_HEIGHT_RATIO)
  const c = chevron(fontSize, 0, h / 2)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${c.right} ${h}"><path d="${c.d}" fill="${color}"/></svg>`
  return { svg, width: c.right, height: h }
}
