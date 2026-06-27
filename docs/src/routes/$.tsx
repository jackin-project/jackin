import { useMDXComponents } from '@/components/mdx'
import { SocialIcons } from '@/components/chrome/SocialIcons'
import { ThemeToggle } from '@/components/chrome/ThemeToggle'
import { baseOptions } from '@/lib/layout.shared'
import { pageSeo } from '@/lib/seo'
import { slugsToMarkdownPath, splatToSlugs } from '@/lib/source-paths'
import { gitConfig } from '@/lib/shared'
import { createFileRoute, Link, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { staticFunctionMiddleware } from '@tanstack/start-static-server-functions'
import browserCollections from 'collections/browser'
import { useFumadocsLoader } from 'fumadocs-core/source/client'
import { DocsLayout } from 'fumadocs-ui/layouts/docs'
import { Layers, Route as RoadmapIcon, SquareTerminal } from "lucide-react"
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

export const Route = createFileRoute('/$')({
  component: Page,
  loader: async ({ params }) => {
    const slugs = splatToSlugs(params._splat)
    const data = await serverLoader({ data: slugs })
    await clientLoader.preload(data.path)
    return data
  },
  head: ({ loaderData }) => loaderData?.seo ?? {},
})

const serverLoader = createServerFn({
  method: 'GET',
})
  .inputValidator((slugs: string[]) => slugs)
  .middleware([staticFunctionMiddleware])
  .handler(async ({ data: slugs }) => {
    const { source } = await import('@/lib/source')
    const { findNeighbour } = await import('fumadocs-core/page-tree')
    const page = source.getPage(slugs)
    if (!page) throw notFound()

    const footerItems = findNeighbour(source.getPageTree(), page.url, { separateRoot: false })
    const serializeFooterItem = (item: typeof footerItems.previous): PageFooterItem | undefined => {
      if (!item) return undefined

      return {
        name: typeof item.name === 'string' ? item.name : String(item.name),
        description: typeof item.description === 'string' ? item.description : undefined,
        url: item.url,
      }
    }

    const slug = page.slugs.join('/')
    const sectionSegment = page.slugs[0]
    const section = sectionSegment
      ? sectionSegment.charAt(0).toUpperCase() + sectionSegment.slice(1).replace(/-/g, ' ')
      : undefined

    return {
      path: page.path,
      markdownUrl: slugsToMarkdownPath(page.slugs).url,
      footerItems: {
        previous: serializeFooterItem(footerItems.previous),
        next: serializeFooterItem(footerItems.next),
      },
      pageTree: await source.serializePageTree(source.getPageTree()),
      seo: pageSeo({
        title: page.data.title,
        description: page.data.description,
        path: page.url,
        slug,
        section,
      }),
    }
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
            githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/docs/content/docs/${path}`}
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
  const { pageTree, path, markdownUrl, footerItems } = useFumadocsLoader(Route.useLoaderData())

  return (
    <DocsLayout
      {...baseOptions()}
      tree={pageTree}
      sidebar={{
        defaultOpenLevel: 2,
        // Render the section switcher in the sidebar. The desktop sidebar only
        // shows it when tabMode === 'auto'; don't rely on the fumadocs default
        // (it changed across versions and silently dropped the switcher).
        tabMode: 'auto',
        // Three doc blocks, switched via the sidebar dropdown (fumadocs Sidebar Tabs).
        // Public is the default; Internals and Roadmap are separate roots, hidden
        // until the reader switches. Order matters: `/` prefix-matches everything,
        // so the more specific roots must come after it for active-tab detection.
        tabs: [
          {
            // Public is the catch-all section spanning /getting-started, /guides,
            // /commands — no shared prefix but `/`. Matching on `/` keeps the
            // switcher present on every public page; the more-specific /reference
            // and /roadmap tabs still win on their pages via findLast (order matters).
            title: 'jackin❯',
            description: 'Install, run, and operate jackin❯.',
            url: '/',
            icon: <SquareTerminal />,
          },
          {
            title: 'Behind jackin❯',
            description: 'Internals, research, and developer reference.',
            url: '/reference',
            icon: <Layers />,
          },
          {
            title: 'Roadmap',
            description: 'Planned, in-progress, and shipped work on jackin❯ itself.',
            url: '/roadmap',
            icon: <RoadmapIcon />,
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
      <Link to={markdownUrl} hidden />
      <Suspense>{clientLoader.useContent(path, { markdownUrl, footerItems, path })}</Suspense>
    </DocsLayout>
  )
}
