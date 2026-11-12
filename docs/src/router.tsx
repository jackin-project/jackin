import { createRouter as createTanStackRouter } from '@tanstack/react-router'
import { NotFound } from '@/components/not-found'
import { Route as rootRoute } from './routes/__root'
import { Route as indexRoute } from './routes/index'
import { Route as notFoundRoute } from './routes/404'
import { Route as docsRoute } from './routes/$'

const routeTree = rootRoute.addChildren([indexRoute, notFoundRoute, docsRoute])

export function getRouter() {
  return createTanStackRouter({
    routeTree,
    defaultPreload: 'intent',
    scrollRestoration: true,
    defaultNotFoundComponent: NotFound,
  })
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof getRouter>
  }
}
