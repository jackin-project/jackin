import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'

export default defineConfig({
  site: 'https://jackin.tailrocks.com',
  integrations: [
    starlight({
      title: "jackin'",
      description: 'CLI for orchestrating AI coding agents in isolated containers',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' }],
      sidebar: [
        { label: 'Placeholder', items: [{ label: 'Placeholder', slug: 'placeholder' }] },
      ],
      customCss: [
        './src/styles/global.css',
        './src/styles/tempo-tokens.css',
        './src/styles/docs-theme.css',
      ],
    }),
    react(),
  ],
})
