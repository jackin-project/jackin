import { defineConfig } from 'vocs'

export default defineConfig({
  title: "jackin'",
  titleTemplate: "%s — jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated containers',
  rootDir: '.',
  baseUrl: 'https://jackin.tailrocks.com',
  theme: {
    colorScheme: 'dark',
    accentColor: '#00ff41',
    variables: {
      // Route Vocs chrome through the Tempo-style Radix ramps in tempo-tokens.css.
      color: {
        background: 'var(--color-gray2)',
        background2: 'var(--color-gray3)',
        background3: 'var(--color-gray3)',
        background4: 'var(--color-gray4)',
        background5: 'var(--color-gray5)',
        backgroundDark: 'var(--color-gray1)',
        border: 'var(--color-grayA4)',
        border2: 'var(--color-grayA5)',
        text: 'var(--color-gray12)',
        text2: 'var(--color-gray11)',
        text3: 'var(--color-gray10)',
        text4: 'var(--color-gray9)',
        heading: 'var(--color-gray12)',
        codeInlineText: 'var(--color-gray12)',
        codeInlineBackground: 'var(--color-grayA3)',
        codeInlineBorder: 'var(--color-grayA4)',
        codeBlockBackground: 'var(--color-gray1)',
        codeTitleBackground: 'var(--color-gray2)',
      },
      fontWeight: {
        regular: '400',
        medium: '500',
        semibold: '600',
      },
    },
  },
  editLink: {
    pattern:
      'https://github.com/jackin-project/jackin/edit/main/docs/pages/:path',
    text: 'Edit on GitHub',
  },
  socials: [
    { icon: 'github', link: 'https://github.com/jackin-project/jackin' },
  ],
  topNav: [
    { text: 'Docs', link: '/getting-started/why', match: '/getting-started' },
    { text: 'Guides', link: '/guides/workspaces', match: '/guides' },
    { text: 'Commands', link: '/commands/load', match: '/commands' },
    { text: 'Reference', link: '/reference/configuration', match: '/reference' },
  ],
  sidebar: [
    {
      text: 'Getting Started',
      items: [
        { text: "Why jackin'?", link: '/getting-started/why' },
        { text: 'Installation', link: '/getting-started/installation' },
        { text: 'Quick Start', link: '/getting-started/quickstart' },
        { text: 'Concepts', link: '/getting-started/concepts' },
      ],
    },
    {
      text: 'Guides',
      items: [
        { text: 'Workspaces', link: '/guides/workspaces' },
        { text: 'Mounts', link: '/guides/mounts' },
        { text: 'Authentication', link: '/guides/authentication' },
        { text: 'Agent Repos', link: '/guides/agent-repos' },
        { text: 'Security Model', link: '/guides/security-model' },
        { text: 'Comparison', link: '/guides/comparison' },
      ],
    },
    {
      text: 'Commands',
      items: [
        { text: 'load', link: '/commands/load' },
        { text: 'launch', link: '/commands/launch' },
        { text: 'hardline', link: '/commands/hardline' },
        { text: 'eject', link: '/commands/eject' },
        { text: 'exile', link: '/commands/exile' },
        { text: 'purge', link: '/commands/purge' },
        { text: 'workspace', link: '/commands/workspace' },
        { text: 'config', link: '/commands/config' },
      ],
    },
    {
      text: 'Developing Agents',
      items: [
        { text: 'Creating Agents', link: '/developing/creating-agents' },
        { text: 'Construct Image', link: '/developing/construct-image' },
        { text: 'Agent Manifest', link: '/developing/agent-manifest' },
      ],
    },
    {
      text: 'Reference',
      items: [
        { text: 'Configuration', link: '/reference/configuration' },
        { text: 'Architecture', link: '/reference/architecture' },
        { text: 'Roadmap', link: '/reference/roadmap' },
      ],
    },
  ],
})
