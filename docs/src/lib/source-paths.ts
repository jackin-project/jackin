// Client-safe path helpers. Keep server-only collection loading in source.ts.
export function splatToSlugs(splat: string | undefined, stripSuffix?: string) {
  const segments = splat?.split('/').filter(Boolean) ?? []
  const last = segments.length - 1
  if (stripSuffix && last >= 0 && segments[last].endsWith(stripSuffix)) {
    segments[last] = segments[last].slice(0, -stripSuffix.length)
  }
  return segments
}

export function markdownPathToSlugs(segments: string[]) {
  if (segments.length === 0) return []

  const slugs = [...segments]
  slugs[slugs.length - 1] = slugs[slugs.length - 1].replace(/\.md$/, '')
  if (slugs.length === 1 && slugs[0] === 'index') slugs.pop()
  return slugs
}

export function slugsToMarkdownPath(slugs: string[]) {
  const segments = [...slugs]
  if (segments.length === 0) {
    segments.push('index.md')
  } else {
    segments[segments.length - 1] += '.md'
  }

  return {
    segments,
    url: `/${segments.join('/')}`,
  }
}
