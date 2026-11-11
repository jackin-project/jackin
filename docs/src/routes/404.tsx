import { Route as rootRoute } from './__root'
import { NotFound } from '@/components/not-found'
import { createRoute } from '@tanstack/react-router'
import { site } from '@/lib/shared'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/404',
  head: () => ({
    meta: [
      { title: "Not found - jackin❯" },
      { name: 'description', content: site.description },
      { property: 'og:image', content: `${site.origin}/og-image.png` },
      { name: 'twitter:image', content: `${site.origin}/og-image.png` },
    ],
  }),
  component: NotFound,
})
