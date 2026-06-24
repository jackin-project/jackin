import { site } from './shared'

const OG_ALT = 'jackin❯ - isolated AI coding agent containers with scoped access and visible host boundaries'

export function ogImageUrl(slug: string) {
  return new URL(`/og/${slug}.webp`, site.origin).toString()
}

export function pageSeo({
  title,
  description,
  path,
  slug,
  section,
}: {
  title: string
  description?: string
  path: string
  slug: string
  section?: string
}) {
  const canonical = new URL(path.replace(/\/?$/, '/'), site.origin).toString()
  const image = ogImageUrl(slug)
  const desc = description ?? site.description

  const techArticle = {
    '@context': 'https://schema.org',
    '@type': 'TechArticle',
    headline: title,
    description: desc,
    url: canonical,
    image,
    inLanguage: 'en',
    articleSection: section,
    isPartOf: {
      '@type': 'WebSite',
      name: site.name,
      url: `${site.origin}/`,
    },
    publisher: {
      '@type': 'Organization',
      name: 'jackin project',
      url: 'https://github.com/jackin-project',
    },
  }

  const breadcrumb = {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: [
      {
        '@type': 'ListItem',
        position: 1,
        name: 'Home',
        item: `${site.origin}/`,
      },
      {
        '@type': 'ListItem',
        position: 2,
        name: title,
        item: canonical,
      },
    ],
  }

  return {
    meta: [
      { title: `${title} - jackin❯` },
      { name: 'description', content: desc },
      { property: 'og:title', content: title },
      { property: 'og:description', content: desc },
      { property: 'og:url', content: canonical },
      { property: 'og:type', content: 'article' },
      { property: 'og:site_name', content: site.name },
      { property: 'og:image', content: image },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      { property: 'og:image:alt', content: OG_ALT },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: title },
      { name: 'twitter:description', content: desc },
      { name: 'twitter:image', content: image },
      { name: 'twitter:image:alt', content: OG_ALT },
      ...(section ? [{ property: 'article:section', content: section }] : []),
    ],
    links: [{ rel: 'canonical', href: canonical }],
    scripts: [
      { type: 'application/ld+json', children: JSON.stringify(techArticle) },
      { type: 'application/ld+json', children: JSON.stringify(breadcrumb) },
    ],
  }
}
