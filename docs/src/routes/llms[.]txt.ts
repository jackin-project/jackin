import { source } from '@/lib/source'
import { createFileRoute } from '@tanstack/react-router'
import { llms } from 'fumadocs-core/source'

export const Route = createFileRoute('/llms.txt')({
  server: {
    handlers: {
      GET: () =>
        new Response(llms(source).index(), {
          headers: { 'Content-Type': 'text/plain; charset=utf-8' },
        }),
    },
  },
})
