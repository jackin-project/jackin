import brandMarkSvg from './brand-mark.svg?raw'

type BrandMarkProps = {
  byline?: boolean
  className?: string
}

// Single source of truth for the jackin❯ lockup: the SVG lives in
// `brand-mark.svg` and is inlined here (via ?raw) so every placement — header,
// sidebar, mobile nav, landing, footer — is identical, scales with the host
// element's font-size (CSS `.jk-brand-mark__svg { height: 1.5em }`), and still
// picks up the page's mono webfont and the --jk-logo-* theme colors.
export function BrandMark({ byline = false, className }: BrandMarkProps) {
  return (
    <span
      aria-label={`jackin❯${byline ? ' by tailrocks' : ''}`}
      className={['jk-brand-mark', byline ? 'jk-brand-mark--lockup' : '', className].filter(Boolean).join(' ')}
      translate="no"
    >
      <span className="jk-brand-mark__svg-wrap" dangerouslySetInnerHTML={{ __html: brandMarkSvg }} />
      {byline ? <span className="jk-brand-mark__byline">by tailrocks</span> : null}
    </span>
  )
}
