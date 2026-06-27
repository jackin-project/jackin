type BrandMarkProps = {
  className?: string
}

// The jackin❯ lockup is the single generated, fully-outlined SVG at
// /brand/jackin-wordmark.svg (see scripts/gen-brand.ts) — it carries the
// "by tailrocks" byline and is the same file the TOC uses. Rendered as an <img>
// so every placement is byte-for-byte identical and needs no webfont.
export function BrandMark({ className }: BrandMarkProps) {
  return (
    <span
      aria-label="jackin❯ by tailrocks"
      className={['jk-brand-mark', className].filter(Boolean).join(' ')}
      translate="no"
    >
      <img className="jk-brand-mark__svg" src="/brand/jackin-wordmark.svg" alt="" aria-hidden="true" />
    </span>
  )
}
