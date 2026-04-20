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
        background: '#0a0b0a',
        background2: '#0f1110',
        background3: '#121413',
        background4: '#15181a',
        background5: '#1a1d1f',
        backgroundDark: '#050605',
        border: 'rgba(244, 247, 245, 0.1)',
        border2: 'rgba(244, 247, 245, 0.22)',
        text: '#f4f7f5',
        text2: '#9ca8a1',
        text3: '#9ca8a1',
        text4: '#5e6a64',
        heading: '#f4f7f5',
        codeInlineText: '#f4f7f5',
        codeInlineBackground: 'rgba(244, 247, 245, 0.04)',
        codeInlineBorder: 'rgba(244, 247, 245, 0.1)',
        codeBlockBackground: 'rgba(0, 5, 3, 0.85)',
        codeTitleBackground: 'rgba(255, 255, 255, 0.018)',
      },
      fontFamily: {
        default: "'Inter', system-ui, -apple-system, sans-serif",
        mono: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace",
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
