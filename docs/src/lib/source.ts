import { loader } from 'fumadocs-core/source'
import { docs } from 'collections/server'
import { site } from './shared'

export const source = loader({
  source: docs.toFumadocsSource(),
  baseUrl: '/',
})

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

export function pageCanonicalUrl(pageUrl: string) {
  return new URL(pageUrl.replace(/\/?$/, '/'), site.origin).toString()
}

export async function getLLMText(page: (typeof source)['$inferPage']) {
  const processed = await page.data.getText('processed')

  return `# ${page.data.title} (${pageCanonicalUrl(page.url)})

${processed}`
}
