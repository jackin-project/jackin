type BrandMarkProps = {
  form?: 'wordmark' | 'monogram'
  byline?: boolean
  className?: string
}

export function BrandMark({ form = 'wordmark', byline = false, className }: BrandMarkProps) {
  const text = form === 'monogram' ? 'j' : 'jackin'
  const label = `${text}❯${byline ? ' by tailrocks' : ''}`

  return (
    <span
      aria-label={label}
      className={['jk-brand-mark', `jk-brand-mark--${form}`, byline ? 'jk-brand-mark--lockup' : '', className]
        .filter(Boolean)
        .join(' ')}
      translate="no"
    >
      <span className="jk-brand-mark__text">{text}</span>
      <span className="jk-brand-mark__chevron">❯</span>
      {byline ? <span className="jk-brand-mark__byline">by tailrocks</span> : null}
    </span>
  )
}
