import ImageResponse from '@takumi-rs/image-response'
import { source } from '@/lib/source'
import { createFileRoute, notFound } from '@tanstack/react-router'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..', '..', '..')
const interBold = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-800-normal.woff'))
const interRegular = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-500-normal.woff'))

function slugsFromParam(value: string | undefined) {
  if (!value) return []
  return value
    .split('/')
    .filter(Boolean)
    .map((segment, index, segments) => (index === segments.length - 1 ? segment.replace(/\.webp$/, '') : segment))
}

function Card({ title, description }: { title: string; description?: string }) {
  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'center',
        backgroundColor: '#0a0a0a',
        borderLeft: '12px solid #5cf07a',
        padding: 80,
        fontFamily: 'Inter',
      }}
    >
      <div
        style={{
          display: 'flex',
          color: 'rgb(244,247,245)',
          fontSize: 72,
          fontWeight: 800,
          lineHeight: 1.1,
          letterSpacing: -1.2,
          marginBottom: description ? 32 : 0,
        }}
      >
        {title}
      </div>
      {description ? (
        <div
          style={{
            display: 'flex',
            color: 'rgb(156,168,161)',
            fontSize: 32,
            fontWeight: 500,
            lineHeight: 1.4,
          }}
        >
          {description}
        </div>
      ) : null}
    </div>
  )
}

export const Route = createFileRoute('/og/{$}.webp')({
  server: {
    handlers: {
      GET: async ({ params }) => {
        const page = source.getPage(slugsFromParam(params._splat))
        if (!page) throw notFound()

        const response = new ImageResponse(<Card title={page.data.title} description={page.data.description} />, {
          width: 1200,
          height: 630,
          format: 'webp',
          fonts: [
            { name: 'Inter', data: interBold, weight: 800, style: 'normal' },
            { name: 'Inter', data: interRegular, weight: 500, style: 'normal' },
          ],
          headers: {
            'Cache-Control': 'public, immutable, max-age=31536000',
          },
        })
        await response.ready
        return response
      },
    },
  },
})
