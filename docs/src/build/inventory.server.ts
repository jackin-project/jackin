import { findNeighbour } from 'fumadocs-core/page-tree'
import { createFromSource } from 'fumadocs-core/search/server'
import { freshSerializedPageTree, getLLMText, publicPageUrls, source } from '@/lib/source'
import { slugsToMarkdownPath } from '@/lib/source-paths'
import { pageSeo } from '@/lib/seo'
import type { PageData } from './page-data'
import { llms } from 'fumadocs-core/source'
import { site } from '@/lib/shared'
import { renderOg } from './og.server'

export async function createInventory() {
  const pages: Record<string, PageData> = {}
  const artifacts: { path: string; response: Response }[] = []
  const tree = await freshSerializedPageTree()
  for (const page of source.getPages()) {
    if (pages[page.url]) throw new Error(`Duplicate page URL: ${page.url}`)
    artifacts.push({ path: slugsToMarkdownPath(page.slugs).url, response: new Response(await getLLMText(page)) })
    artifacts.push({ path: `/og/${page.slugs.join('/')}.webp`, response: await renderOg(page) })
    const neighbours = findNeighbour(source.getPageTree(), page.url, { separateRoot: false })
    const footer = (item: typeof neighbours.previous) => item && ({
      name: typeof item.name === 'string' ? item.name : String(item.name),
      description: typeof item.description === 'string' ? item.description : undefined,
      url: item.url,
    })
    const segment = page.slugs[0]
    pages[page.url] = {
      path: page.path,
      markdownUrl: slugsToMarkdownPath(page.slugs).url,
      publicUrls: publicPageUrls,
      footerItems: { previous: footer(neighbours.previous), next: footer(neighbours.next) },
      pageTree: tree,
      seo: pageSeo({
        title: page.data.title,
        description: page.data.description,
        path: page.url,
        slug: page.slugs.join('/'),
        section: segment ? segment.charAt(0).toUpperCase() + segment.slice(1).replace(/-/g, ' ') : undefined,
      }),
    }
  }
  const urls = [`${site.origin}/`, ...source.getPages().map((page) => new URL(page.url.replace(/\/?$/, '/'), site.origin).toString())]
  const sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls.map((url) => `  <url>
    <loc>${url}</loc>
    <changefreq>weekly</changefreq>
    <priority>${url === `${site.origin}/` ? '1.0' : '0.7'}</priority>
  </url>`).join('\n')}
</urlset>
`
  artifacts.push(
    { path: '/llms.txt', response: new Response(llms(source).index()) },
    { path: '/llms-full.txt', response: new Response((await Promise.all(source.getPages().map(getLLMText))).join('\n\n')) },
    { path: '/sitemap.xml', response: new Response(sitemap) },
    { path: '/api/search.json', response: await createFromSource(source, { language: 'english' }).staticGET() },
  )
  return { pages, artifacts }
}
