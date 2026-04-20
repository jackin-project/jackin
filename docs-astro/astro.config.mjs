import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'

export default defineConfig({
  site: 'https://jackin.tailrocks.com',
  integrations: [
    starlight({
      title: "jackin'",
      description: 'CLI for orchestrating AI coding agents in isolated containers',
      // Single dark theme for code blocks regardless of page light/dark —
      // code stays readable against a dark surface in either mode.
      expressiveCode: {
        themes: ['github-dark'],
      },
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' }],
      editLink: {
        baseUrl: 'https://github.com/jackin-project/jackin/edit/main/docs-astro/src/content/docs/',
      },
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: "Why jackin'?", slug: 'getting-started/why' },
            { label: 'Installation', slug: 'getting-started/installation' },
            { label: 'Quick Start', slug: 'getting-started/quickstart' },
            { label: 'Concepts', slug: 'getting-started/concepts' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Workspaces', slug: 'guides/workspaces' },
            { label: 'Mounts', slug: 'guides/mounts' },
            { label: 'Authentication', slug: 'guides/authentication' },
            { label: 'Agent Repos', slug: 'guides/agent-repos' },
            { label: 'Security Model', slug: 'guides/security-model' },
            { label: 'Comparison', slug: 'guides/comparison' },
          ],
        },
        {
          label: 'Commands',
          items: [
            { label: 'load', slug: 'commands/load' },
            { label: 'launch', slug: 'commands/launch' },
            { label: 'hardline', slug: 'commands/hardline' },
            { label: 'eject', slug: 'commands/eject' },
            { label: 'exile', slug: 'commands/exile' },
            { label: 'purge', slug: 'commands/purge' },
            { label: 'workspace', slug: 'commands/workspace' },
            { label: 'config', slug: 'commands/config' },
          ],
        },
        {
          label: 'Developing Agents',
          items: [
            { label: 'Creating Agents', slug: 'developing/creating-agents' },
            { label: 'Construct Image', slug: 'developing/construct-image' },
            { label: 'Agent Manifest', slug: 'developing/agent-manifest' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Configuration', slug: 'reference/configuration' },
            { label: 'Architecture', slug: 'reference/architecture' },
            { label: 'Roadmap', slug: 'reference/roadmap' },
          ],
        },
      ],
      components: {
        Head: './src/components/overrides/Head.astro',
        PageSidebar: './src/components/overrides/PageSidebar.astro',
        SiteTitle: './src/components/overrides/SiteTitle.astro',
        ThemeSelect: './src/components/overrides/ThemeSelect.astro',
      },
      customCss: [
        './src/styles/fonts.css',
        './src/styles/global.css',
        './src/styles/tempo-tokens.css',
        './src/styles/docs-theme.css',
      ],
    }),
    react(),
  ],
})
