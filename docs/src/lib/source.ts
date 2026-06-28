import { createElement } from 'react'
import { loader } from 'fumadocs-core/source'
import { icons } from 'lucide-react'
import { docs } from 'collections/server'
import { site } from './shared'
export { markdownPathToSlugs, slugsToMarkdownPath, splatToSlugs } from './source-paths'

export const source = loader({
  source: docs.toFumadocsSource(),
  baseUrl: '/',
  // Resolve `icon` strings in meta.json (the doc-root tab tiles) to Lucide icons.
  icon(name) {
    if (name && name in icons) {
      return createElement(icons[name as keyof typeof icons])
    }
  },
})

export function pageCanonicalUrl(pageUrl: string) {
  return new URL(pageUrl.replace(/\/?$/, '/'), site.origin).toString()
}

export async function getLLMText(page: (typeof source)['$inferPage']) {
  const processed = await page.data.getText('processed')

  return `# ${page.data.title} (${pageCanonicalUrl(page.url)})

${processed}`
}
