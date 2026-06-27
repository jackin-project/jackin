type BrandMarkProps = {
  byline?: boolean
  className?: string
}

// The jackin❯ lockup is the single generated, fully-outlined SVG at
// /brand/jackin-wordmark.svg (see scripts/gen-brand.ts) — the same file the TOC
// uses. Rendered as an <img> so every placement is byte-for-byte identical and
// needs no webfont. The wrapper carries the accessible label.
export function BrandMark({ byline = false, className }: BrandMarkProps) {
  return (
    <span
      aria-label={`jackin❯${byline ? ' by tailrocks' : ''}`}
      className={['jk-brand-mark', byline ? 'jk-brand-mark--lockup' : '', className].filter(Boolean).join(' ')}
      translate="no"
    >
      <img
        className="jk-brand-mark__svg"
        src={byline ? '/brand/jackin-lockup.svg' : '/brand/jackin-wordmark.svg'}
        alt=""
        aria-hidden="true"
      />
    </span>
  )
}
