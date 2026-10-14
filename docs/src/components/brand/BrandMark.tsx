type BrandMarkProps = {


  byline?: boolean
  className?: string
}







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
