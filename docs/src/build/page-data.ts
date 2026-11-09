import type { source } from '@/lib/source'
import type { pageSeo } from '@/lib/seo'

type FooterItem = { name: string; description?: string; url: string }

export type PageData = {
  path: string
  markdownUrl: string
  publicUrls: string[]
  footerItems: { previous?: FooterItem; next?: FooterItem }
  pageTree: Awaited<ReturnType<typeof source.serializePageTree>>
  seo: ReturnType<typeof pageSeo>
}
