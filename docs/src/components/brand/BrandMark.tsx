type BrandMarkProps = {
  form?: 'wordmark' | 'monogram'
  byline?: boolean
  className?: string
}

// Single source of truth for the jackin❯ lockup. Rendered as one inline SVG so
// every placement (header, sidebar, mobile nav, landing, footer) is identical
// and scales with the host element's font-size (via CSS height: 1.5em on the
// svg). `textLength` pins the glyph runs to a fixed advance, so the green block
// always hugs the letters regardless of the loaded mono font's metrics.
const FONT = 'var(--sl-font-mono)'
const SIZE = 72 // glyph size in viewBox units
const CHAR = SIZE * 0.6 // mono advance per char
const PAD_X = 22
const GAP = 14
const HEIGHT = 98
const CHEVRON = 46

export function BrandMark({ form = 'wordmark', byline = false, className }: BrandMarkProps) {
  const word = form === 'monogram' ? 'j' : 'jackin'
  const label = `${word}❯${byline ? ' by tailrocks' : ''}`

  const wordLen = word.length * CHAR
  const chevX = PAD_X + wordLen + GAP
  const width = chevX + CHEVRON + PAD_X
  const mid = HEIGHT / 2

  return (
    <span
      aria-label={label}
      className={['jk-brand-mark', `jk-brand-mark--${form}`, byline ? 'jk-brand-mark--lockup' : '', className]
        .filter(Boolean)
        .join(' ')}
      translate="no"
    >
      <svg
        className="jk-brand-mark__svg"
        viewBox={`0 0 ${width} ${HEIGHT}`}
        role="img"
        aria-hidden="true"
        preserveAspectRatio="xMinYMid meet"
      >
        <rect x="0" y="0" width={width} height={HEIGHT} fill="var(--jk-logo-block)" />
        <text
          x={PAD_X}
          y={mid}
          textLength={wordLen}
          lengthAdjust="spacingAndGlyphs"
          dominantBaseline="central"
          fontFamily={FONT}
          fontSize={SIZE}
          fontWeight={500}
          fill="var(--jk-logo-ink)"
        >
          {word}
        </text>
        <text
          x={chevX}
          y={mid}
          textLength={CHEVRON}
          lengthAdjust="spacingAndGlyphs"
          dominantBaseline="central"
          fontFamily={FONT}
          fontSize={SIZE}
          fontWeight={600}
          fill="var(--jk-logo-chevron)"
        >
          ❯
        </text>
      </svg>
      {byline ? <span className="jk-brand-mark__byline">by tailrocks</span> : null}
    </span>
  )
}
