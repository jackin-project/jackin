import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import mdx from 'fumadocs-mdx/vite'
import { nitro } from 'nitro/vite'
import { defineConfig } from 'vite'
import { readdirSync } from 'node:fs'
import { join, relative, sep } from 'node:path'

const CONTENT_ROOT = join(import.meta.dirname, 'content', 'docs')

function docsSlugs(dir = CONTENT_ROOT): string[] {
  const entries = readdirSync(dir, { withFileTypes: true })
  return entries.flatMap((entry) => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return docsSlugs(path)
    if (!entry.isFile() || !entry.name.endsWith('.mdx')) return []

    const rel = relative(CONTENT_ROOT, path)
      .split(sep)
      .filter((part) => !part.startsWith('(') || !part.endsWith(')'))
      .join('/')
    const withoutExt = rel.replace(/\.mdx$/, '')
    return withoutExt.endsWith('/index')
      ? [withoutExt.slice(0, -'/index'.length)]
      : [withoutExt]
  })
}

function pagePath(slug: string): string {
  return `/${slug}`.replace(/\/+/g, '/')
}

function prerenderPages() {
  const pages = docsSlugs().flatMap((slug) => {
    const path = pagePath(slug)
    return [
      { path },
      { path: `${path}.md` },
      { path: `/og/${slug}.webp` },
    ]
  })

  return [
    { path: '/' },
    { path: '/404' },
    { path: '/api/search' },
    { path: '/llms.txt' },
    { path: '/llms-full.txt' },
    { path: '/sitemap.xml' },
    ...pages,
  ]
}

export default defineConfig({
  build: {
    reportCompressedSize: false,
  },
  server: {
    port: 3000,
  },
  plugins: [
    mdx(),
    tailwindcss(),
    tanstackStart({
      prerender: {
        enabled: false,
      },
      sitemap: {
        enabled: false,
      },
      pages: prerenderPages(),
    }),
    react(),
    nitro(),
  ],
  resolve: {
    tsconfigPaths: true,
  },
  ssr: {
    external: ['@takumi-rs/image-response'],
  },
})
