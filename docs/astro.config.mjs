import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://donbeave.github.io',
  base: '/jackin',
  integrations: [
    starlight({
      title: 'jackin',
      description: 'Matrix-inspired CLI for orchestrating AI coding agents at scale',
      logo: {
        dark: './src/assets/logo-dark.png',
        light: './src/assets/logo-light.png',
        replacesTitle: true,
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/donbeave/jackin' },
      ],
      editLink: {
        baseUrl: 'https://github.com/donbeave/jackin/edit/main/docs/',
      },
      customCss: ['./src/styles/custom.css'],
      head: [
        {
          tag: 'meta',
          attrs: {
            property: 'og:image',
            content: 'https://donbeave.github.io/jackin/og-image.png',
          },
        },
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Why Jackin?', slug: 'getting-started/why' },
            { label: 'Installation', slug: 'getting-started/installation' },
            { label: 'Quick Start', slug: 'getting-started/quickstart' },
            { label: 'Core Concepts', slug: 'getting-started/concepts' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Workspaces', slug: 'guides/workspaces' },
            { label: 'Mounts', slug: 'guides/mounts' },
            { label: 'Agent Repos', slug: 'guides/agent-repos' },
            { label: 'Security Model', slug: 'guides/security-model' },
            { label: 'Comparison with Alternatives', slug: 'guides/comparison' },
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
            { label: 'Creating an Agent', slug: 'developing/creating-agents' },
            { label: 'The Construct Image', slug: 'developing/construct-image' },
            { label: 'Agent Manifest', slug: 'developing/agent-manifest' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Configuration File', slug: 'reference/configuration' },
            { label: 'Architecture', slug: 'reference/architecture' },
            { label: 'Roadmap', slug: 'reference/roadmap' },
          ],
        },
      ],
    }),
  ],
});
