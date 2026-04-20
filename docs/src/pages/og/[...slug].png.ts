import { OGImageRoute } from 'astro-og-canvas'
import { getCollection } from 'astro:content'
import { createRequire } from 'node:module'

// Resolve the fontsource woff2 files to absolute file paths so the
// build runs hermetically — no network calls to api.fontsource.org
// during static generation. createRequire + resolve finds the exact
// node_modules path bun installed, so the paths stay correct under
// pnpm's symlinked store or a hoisted layout. astro-og-canvas passes
// these through to satori, which accepts absolute paths.
const req = createRequire(import.meta.url)
const fontFile = (weight: 500 | 800): string =>
  req.resolve(`@fontsource/inter/files/inter-latin-${weight}-normal.woff2`)

// Collect every docs page and key by its slug so the route file
// resolves /og/<slug>.png → generated card for that page.
const entries = await getCollection('docs')
const pages = Object.fromEntries(
  entries.map(({ id, data }) => [id, { data }])
)

const route = await OGImageRoute({
  pages,
  param: 'slug',
  // astro-og-canvas's default appends the image extension to the slug —
  // combined with our [...slug].png.ts filename it produced dead
  // /og/<path>.png.png URLs. Strip only image extensions (NOT any dot
  // — a slug like `reference/next.js-integration` must keep its dot)
  // so the final URL is a clean /og/<path>.png.
  getSlug: (path) => path.replace(/\.(png|jpe?g|webp)$/i, ''),
  getImageOptions: (_path, { data }: { data: { title: string; description?: string } }) => ({
    title: data.title,
    description: data.description ?? '',
    // Landing palette — black bg, phosphor-green accent bar, near-white text.
    bgGradient: [[10, 11, 10]],
    border: { color: [0, 255, 65], width: 12, side: 'inline-start' },
    padding: 80,
    logo: undefined,
    font: {
      title: {
        color: [244, 247, 245],
        families: ['Inter'],
        weight: 'Bold',
        size: 72,
        lineHeight: 1.1,
      },
      description: {
        color: [156, 168, 161],
        families: ['Inter'],
        weight: 'Normal',
        size: 32,
        lineHeight: 1.4,
      },
    },
    fonts: [fontFile(800), fontFile(500)],
  }),
})

export const getStaticPaths = route.getStaticPaths
export const GET = route.GET
