type BrandMarkProps = {
  /** Use the lockup with the "by tailrocks" byline (footer / large surfaces).
   *  Nav chrome (header, sidebar, top nav) leaves this off. */
  byline?: boolean
  className?: string
}

// Generated, fully-outlined SVG (scripts/gen-brand.ts), rendered as an <img> so
// every placement is byte-for-byte identical and needs no webfont. Two variants
// are emitted: the default white-word mark for dark surfaces, and an "onlight"
// dark-word mark for white chrome in light mode. CSS (docs-theme.css) shows the
// right one per theme; the hero, whose canvas stays dark in light mode, keeps the
// white-word variant.
export function BrandMark({ byline = false, className }: BrandMarkProps) {
  const onDark = byline ? '/brand/jackin-lockup.svg' : '/brand/jackin-wordmark.svg'
  const onLight = byline ? '/brand/jackin-lockup-onlight.svg' : '/brand/jackin-wordmark-onlight.svg'
  return (
    <span
      aria-label={`jackin❯${byline ? ' by tailrocks' : ''}`}
      className={['jk-brand-mark', className].filter(Boolean).join(' ')}
      translate="no"
    >
      <img className="jk-brand-mark__svg jk-brand-mark__svg--ondark" src={onDark} alt="" aria-hidden="true" />
      <img className="jk-brand-mark__svg jk-brand-mark__svg--onlight" src={onLight} alt="" aria-hidden="true" />
    </span>
  )
}
