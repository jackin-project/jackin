import { source } from '@/lib/source'
import { site } from '@/lib/shared'
import { createFileRoute } from '@tanstack/react-router'

function sitemap() {
  const urls = [
    `${site.origin}/`,
    ...source.getPages().map((page) => new URL(page.url.replace(/\/?$/, '/'), site.origin).toString()),
  ]

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls
  .map(
    (url) => `  <url>
    <loc>${url}</loc>
    <changefreq>weekly</changefreq>
    <priority>${url === `${site.origin}/` ? '1.0' : '0.7'}</priority>
  </url>`,
  )
  .join('\n')}
</urlset>
`
}

export const Route = createFileRoute('/sitemap.xml')({
  server: {
    handlers: {
      GET: () =>
        new Response(sitemap(), {
          headers: { 'Content-Type': 'application/xml; charset=utf-8' },
        }),
    },
  },
})
