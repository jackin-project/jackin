import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'
import rehypeExternalLinks from 'rehype-external-links'

export default defineConfig({
  site: 'https://jackin.tailrocks.com',
  markdown: {
    rehypePlugins: [
      // Every external link in MDX content opens in a new tab with
      // a no-opener/no-referrer relationship for safety.
      [
        rehypeExternalLinks,
        {
          target: '_blank',
          rel: ['noopener', 'noreferrer'],
          protocols: ['http', 'https'],
        },
      ],
    ],
  },
  integrations: [
    starlight({
      title: "jackin'",
      description:
        "Jack your AI coding agents in. Isolated worlds, scoped access, full autonomy. You're the Operator. They're already inside.",
      // Two dark shiki themes — code blocks stay dark in both page modes
      // (see --jk-code-bg in docs-theme.css), but the syntax palette
      // differs: github-dark is the subdued dark-page default, and
      // one-dark-pro gives a more vibrant palette in light-page context
      // where the dark code surface benefits from saturated accents.
      expressiveCode: {
        themes: ['github-dark', 'one-dark-pro'],
      },
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' }],
      editLink: {
        baseUrl: 'https://github.com/jackin-project/jackin/edit/main/docs/src/content/docs/',
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
            { label: 'Environment Variables', slug: 'guides/environment-variables' },
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
            { label: 'console', slug: 'commands/console' },
            { label: 'launch (deprecated)', slug: 'commands/launch' },
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
            {
              label: 'Roadmap',
              collapsed: false,
              items: [
                { label: 'Overview', slug: 'reference/roadmap' },
                {
                  label: 'Open items',
                  collapsed: true,
                  items: [
                    { label: 'Construct user creation', slug: 'reference/roadmap/construct-user-creation' },
                    { label: '1Password integration', slug: 'reference/roadmap/onepassword-integration' },
                    { label: 'Reliable Claude authentication strategy', slug: 'reference/roadmap/claude-auth-strategy' },
                    { label: 'Bollard migration', slug: 'reference/roadmap/bollard-migration' },
                    { label: 'Rootless DinD', slug: 'reference/roadmap/rootless-dind' },
                    { label: 'Selectable sandbox backends', slug: 'reference/roadmap/selectable-sandbox-backends' },
                    { label: 'Reproducibility & provenance pinning', slug: 'reference/roadmap/reproducibility-pinning' },
                    { label: 'Per-mount isolation', slug: 'reference/roadmap/per-mount-isolation' },
                    { label: 'Devcontainer parity', slug: 'reference/roadmap/devcontainer-parity' },
                    { label: 'Open review findings', slug: 'reference/roadmap/open-review-findings' },
                  ],
                },
                {
                  label: 'Resolved',
                  collapsed: true,
                  items: [
                    { label: 'Env var interpolation', slug: 'reference/roadmap/env-var-interpolation' },
                    { label: 'Orphaned DinD cleanup', slug: 'reference/roadmap/orphaned-dind-cleanup' },
                    { label: 'Sensitive mount warnings', slug: 'reference/roadmap/sensitive-mount-warnings' },
                    { label: 'Custom plugin marketplace', slug: 'reference/roadmap/custom-plugin-marketplace' },
                    { label: 'DinD hostname env var', slug: 'reference/roadmap/dind-hostname-env-var' },
                    { label: 'Agent source trust', slug: 'reference/roadmap/agent-source-trust' },
                    { label: 'DinD TLS', slug: 'reference/roadmap/dind-tls' },
                  ],
                },
              ],
            },
          ],
        },
      ],
      components: {
        Head: './src/components/overrides/Head.astro',
        PageSidebar: './src/components/overrides/PageSidebar.astro',
        SiteTitle: './src/components/overrides/SiteTitle.astro',
        SocialIcons: './src/components/overrides/SocialIcons.astro',
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
