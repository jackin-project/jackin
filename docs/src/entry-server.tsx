import { createRequestHandler, renderRouterToString, RouterServer } from '@tanstack/react-router/ssr/server'
import { getRouter } from './router'

export type ClientAssets = { script: string; css: string[] }

export function render(request: Request, assets: ClientAssets) {
  return createRequestHandler({
    request,
    createRouter: getRouter,
    getRouterManifest: () => ({
      routes: {
        __root__: {
          scripts: [{ attrs: { type: 'module', src: assets.script } }],
          css: assets.css,
        },
      },
    }),
  })(({ router, responseHeaders }) => renderRouterToString({
    router,
    responseHeaders,
    children: <RouterServer router={router} />,
  }))
}
