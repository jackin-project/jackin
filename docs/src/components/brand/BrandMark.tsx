type BrandMarkProps = {
  /** Use the lockup with the "by tailrocks" byline (footer / large surfaces).
   *  Nav chrome (header, sidebar, top nav) leaves this off. */
  byline?: boolean
  className?: string
}

// Generated, fully-outlined SVG (scripts/gen-brand.ts), rendered as an <img> so
// every placement is byte-for-byte identical and needs no webfont.
export function BrandMark({ byline = false, className }: BrandMarkProps) {
  return (
    <span
      aria-label={`jackin❯${byline ? ' by tailrocks' : ''}`}
      className={['jk-brand-mark', className].filter(Boolean).join(' ')}
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
