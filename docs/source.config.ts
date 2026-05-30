import { defineConfig, defineDocs } from 'fumadocs-mdx/config'
import rehypeExternalLinks from 'rehype-external-links'
import rehypeJk from './scripts/rehype-jk'

export const docs = defineDocs({
  dir: 'content/docs',
  docs: {
    postprocess: {
      includeProcessedMarkdown: true,
      extractLinkReferences: true,
    },
  },
})

export default defineConfig({
  mdxOptions: {
    rehypePlugins: (plugins) => [
      ...plugins,
      [
        rehypeExternalLinks,
        {
          target: '_blank',
          rel: ['noopener', 'noreferrer'],
          protocols: ['http', 'https'],
        },
      ],
      rehypeJk,
    ],
    rehypeCodeOptions: {
      themes: {
        light: 'github-dark',
        dark: 'github-dark',
      },
    },
  },
})
