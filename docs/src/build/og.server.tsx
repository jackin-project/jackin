import ImageResponse from '@takumi-rs/image-response'
import { source } from '@/lib/source'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = process.cwd()
const interBold = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-800-normal.woff2'))
const interRegular = readFileSync(join(root, 'node_modules', '@fontsource', 'inter', 'files', 'inter-latin-500-normal.woff2'))

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

export async function renderOg(page: (typeof source)['$inferPage']) {
  const response = new ImageResponse(<Card title={page.data.title} description={page.data.description} />, {
    width: 1200,
    height: 630,
    format: 'webp',
    fonts: [
      { name: 'Inter', data: interBold, weight: 800, style: 'normal' },
      { name: 'Inter', data: interRegular, weight: 500, style: 'normal' },
    ],
    headers: { 'Cache-Control': 'public, immutable, max-age=31536000' },
  })
  await response.ready
  return response
}
