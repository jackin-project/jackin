import { useMDXComponents } from '@/components/mdx'
import { ThemeToggle } from '@/components/chrome/ThemeToggle'
import { baseOptions } from '@/lib/layout.shared'
import { pageSeo } from '@/lib/seo'
import { slugsToMarkdownPath, source } from '@/lib/source'
import { gitConfig } from '@/lib/shared'
import { createFileRoute, Link, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { staticFunctionMiddleware } from '@tanstack/start-static-server-functions'
import browserCollections from 'collections/browser'
import { useFumadocsLoader } from 'fumadocs-core/source/client'
import { DocsLayout } from 'fumadocs-ui/layouts/docs'
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  MarkdownCopyButton,
  ViewOptionsPopover,
} from 'fumadocs-ui/layouts/docs/page'
import { Suspense } from 'react'

export const Route = createFileRoute('/$')({
  component: Page,
  loader: async ({ params }) => {
    const slugs = params._splat?.split('/').filter(Boolean) ?? []
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
    const page = source.getPage(slugs)
    if (!page) throw notFound()

    const slug = page.slugs.join('/')
    const sectionSegment = page.slugs[0]
    const section = sectionSegment
      ? sectionSegment.charAt(0).toUpperCase() + sectionSegment.slice(1).replace(/-/g, ' ')
      : undefined

    return {
      path: page.path,
      markdownUrl: slugsToMarkdownPath(page.slugs).url,
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
      path,
    }: {
      markdownUrl: string
      path: string
    },
  ) {
    return (
      <DocsPage toc={toc}>
        <DocsTitle>{frontmatter.title}</DocsTitle>
        <DocsDescription>{frontmatter.description}</DocsDescription>
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
  const { pageTree, path, markdownUrl } = useFumadocsLoader(Route.useLoaderData())

  return (
    <DocsLayout
      {...baseOptions()}
      tree={pageTree}
      sidebar={{
        defaultOpenLevel: 2,
        footer: (
          <div className="jk-sidebar-footer">
            <ThemeToggle />
          </div>
        ),
      }}
    >
      <Link to={markdownUrl} hidden />
      <Suspense>{clientLoader.useContent(path, { markdownUrl, path })}</Suspense>
    </DocsLayout>
  )
}
