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
      color: {
        background: '#191919',
        background2: '#1d1d1d',
        background3: '#222222',
        background4: '#2a2a2a',
        background5: '#313131',
        backgroundDark: '#111111',
        border: 'rgba(255, 255, 255, 0.12)',
        border2: 'rgba(255, 255, 255, 0.18)',
        text: '#eeeeee',
        text2: '#b4b4b4',
        text3: '#8d8d8d',
        text4: '#6e6e6e',
        heading: '#eeeeee',
        codeInlineText: '#eeeeee',
        codeInlineBackground: 'rgba(255, 255, 255, 0.06)',
        codeInlineBorder: 'rgba(255, 255, 255, 0.1)',
        codeBlockBackground: '#111111',
        codeTitleBackground: '#1d1d1d',
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
