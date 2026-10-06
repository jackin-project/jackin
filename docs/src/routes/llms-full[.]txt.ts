import { getLLMText, source } from '@/lib/source'
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/llms-full.txt')({
  server: {
    handlers: {
      GET: async () => {
        const scanned = await Promise.all(source.getPages().map(getLLMText))
        return new Response(scanned.join('\n\n'), {
          headers: { 'Content-Type': 'text/plain; charset=utf-8' },
        })
      },
    },
  },
})
