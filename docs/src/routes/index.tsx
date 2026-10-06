import { createFileRoute } from '@tanstack/react-router'
import { Landing } from '@/components/landing/Landing'
import { site } from '@/lib/shared'

const title = 'jackin❯ - isolated AI coding agent containers'
const ogImage = `${site.origin}/og-image.png`
const ogAlt = 'jackin❯ - isolated AI coding agent containers with scoped access and visible host boundaries'

const softwareApplication = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: site.name,
  alternateName: ['jackin❯', 'jackin'],
  description: site.description,
  url: `${site.origin}/`,
  image: ogImage,
  applicationCategory: 'DeveloperApplication',
  operatingSystem: 'macOS, Linux',
  offers: {
    '@type': 'Offer',
    price: '0',
    priceCurrency: 'USD',
  },
  license: 'https://www.apache.org/licenses/LICENSE-2.0',
  sameAs: [site.repo],
  publisher: {
    '@type': 'Organization',
    name: 'jackin project',
    url: 'https://github.com/jackin-project',
  },
}

export const Route = createFileRoute('/')({
  head: () => ({
    meta: [
      { title },
      { name: 'description', content: site.description },
      { property: 'og:title', content: title },
      { property: 'og:description', content: site.description },
      { property: 'og:url', content: `${site.origin}/` },
      { property: 'og:type', content: 'website' },
      { property: 'og:site_name', content: site.name },
      { property: 'og:image', content: ogImage },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      { property: 'og:image:alt', content: ogAlt },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: title },
      { name: 'twitter:description', content: site.description },
      { name: 'twitter:image', content: ogImage },
      { name: 'twitter:image:alt', content: ogAlt },
    ],
    links: [{ rel: 'canonical', href: `${site.origin}/` }],
    scripts: [{ type: 'application/ld+json', children: JSON.stringify(softwareApplication) }],
  }),
  component: Landing,
})
