import { OGImageRoute } from 'astro-og-canvas'
import { getCollection } from 'astro:content'

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
    // Landing palette — black bg, Matrix-green accent bar, near-white text.
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
    fonts: [
      'https://api.fontsource.org/v1/fonts/inter/latin-800-normal.woff2',
      'https://api.fontsource.org/v1/fonts/inter/latin-500-normal.woff2',
    ],
  }),
})

export const getStaticPaths = route.getStaticPaths
export const GET = route.GET
