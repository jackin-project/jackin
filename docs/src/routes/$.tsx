import { Route as rootRoute } from './__root'
import { useMDXComponents } from '@/components/mdx'
import { SocialIcons } from '@/components/chrome/SocialIcons'
import { ThemeToggle } from '@/components/chrome/ThemeToggle'
import { baseOptions } from '@/lib/layout.shared'
import { splatToSlugs } from '@/lib/source-paths'
import { pages } from 'collections/pages.generated'
import { gitConfig } from '@/lib/shared'
import { createRoute, notFound } from '@tanstack/react-router'
import browserCollections from 'collections/browser'
import { useFumadocsLoader } from 'fumadocs-core/source/client'
import { DocsLayout } from 'fumadocs-ui/layouts/docs'
import { Layers, SquareTerminal } from 'lucide-react'
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  MarkdownCopyButton,
  ViewOptionsPopover,
} from 'fumadocs-ui/layouts/docs/page'
import { Suspense } from 'react'

type PageFooterItem = {
  name: string
  description?: string
  url: string
}

type PageFooterItems = {
  previous?: PageFooterItem
  next?: PageFooterItem
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/$',
  component: Page,
  loader: async ({ params }) => {
    const slugs = splatToSlugs(params._splat)
    const data = pages[`/${slugs.join('/')}`]
    if (!data) throw notFound()
    await clientLoader.preload(data.path)
    return data
  },
  head: ({ loaderData }) => loaderData?.seo ?? {},
})

const clientLoader = browserCollections.docs.createClientLoader({
  component(
    { toc, frontmatter, default: MDX },
    {
      markdownUrl,
      footerItems,
      path,
    }: {
      markdownUrl: string
      footerItems: PageFooterItems
      path: string
    },
  ) {
    return (
      <DocsPage toc={toc} footer={{ items: footerItems, className: 'jk-page-footer' }}>
        <DocsTitle className="jk-page-title">{frontmatter.title}</DocsTitle>
        <DocsDescription className="jk-page-description">{frontmatter.description}</DocsDescription>
        <div className="jk-page-actions">
          <MarkdownCopyButton markdownUrl={markdownUrl} />
          <ViewOptionsPopover
            markdownUrl={markdownUrl}
            githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/docs/content/${path}`}
          />
        </div>
        <DocsBody>
          <MDX components={useMDXComponents()} />
        </DocsBody>
      </DocsPage>
    )
  },
})

function Page() {
  const loaderData = Route.useLoaderData()
  const { pageTree, path, markdownUrl, footerItems } = useFumadocsLoader(loaderData)
  const publicUrls = new Set(loaderData.publicUrls)

  return (
    <DocsLayout
      {...baseOptions()}
      tree={pageTree}
      sidebar={{
        defaultOpenLevel: 2,
        tabMode: 'auto',
        tabs: [
          {
            title: 'jackin❯',
            description: 'Install, run, and operate jackin❯.',
            url: '/getting-started/why',
            urls: publicUrls,
            icon: <SquareTerminal />,
          },
          {
            title: 'Behind jackin❯',
            description: 'Internals and developer reference.',
            url: '/reference',
            icon: <Layers />,
          },
        ],
        footer: (
          <div className="jk-sidebar-footer">
            <ThemeToggle />
            <SocialIcons />
          </div>
        ),
      }}
    >
      <a href={markdownUrl} hidden />
      <Suspense>{clientLoader.useContent(path, { markdownUrl, footerItems, path })}</Suspense>
    </DocsLayout>
  )
}
