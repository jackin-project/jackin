import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import mdx from 'fumadocs-mdx/vite'
import { defineConfig, type Plugin } from 'vite'
import * as sourceConfig from './source.config'

function metadataModuleType(): Plugin {
  return {
    name: 'fumadocs-meta-module-type',
    enforce: 'pre' as const,
    resolveId(source, importer) {
      const [file, query = ''] = source.split('?', 2)
      const parameters = new URLSearchParams(query)
      if (!file.endsWith('/meta.json') || !parameters.has('collection') || !parameters.has('lang.js')) return null
      parameters.delete('lang.js')
      return this.resolve(`${file}?${parameters}`, importer, { skipSelf: true })
    },
  }
}

export default defineConfig(() => ({
  build: { reportCompressedSize: false },
  plugins: [mdx(sourceConfig), metadataModuleType(), tailwindcss(), react()],
  resolve: { tsconfigPaths: true },
  ssr: { external: ['@takumi-rs/image-response'] },
}))
