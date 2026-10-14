








const CHEVRON_HEIGHT_RATIO = 0.92
const X_INNER_LEFT = 0.2207
const X_INNER_TIP = 0.3278
const X_OUTER_TIP = 0.5424
const WORD_CHEVRON_GAP = 0.16

const round = (n: number) => Math.round(n * 100) / 100

export const chevronHeight = (fontSize: number) => fontSize * CHEVRON_HEIGHT_RATIO

export type Chevron = { d: string; right: number }

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

export function chevronSvg(fontSize: number, color: string): { svg: string; width: number; height: number } {
  const h = round(fontSize * CHEVRON_HEIGHT_RATIO)
  const c = chevron(fontSize, 0, h / 2)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${c.right} ${h}"><path d="${c.d}" fill="${color}"/></svg>`
  return { svg, width: c.right, height: h }
}
