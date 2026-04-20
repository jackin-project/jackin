import { createElement } from 'react'
import { defineConfig } from 'vocs'

export default defineConfig({
  title: "jackin'",
  titleTemplate: "%s — jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated containers',
  rootDir: '.',
  baseUrl: 'https://jackin.tailrocks.com',
  // No `theme` key — mirrors Tempo's setup. All colors + sizing come
  // from docs/tempo-tokens.css (Radix @theme) and docs/docs-theme.css
  // (mapping Vocs tokens to Radix via light-dark()). Light/dark/system
  // is handled by Vocs's built-in init script (.dark class on <html>);
  // CSS translates that class into `color-scheme` so light-dark() tokens
  // resolve correctly.
  //
  // default-dark.js runs synchronously from <head> to default new visitors
  // to dark mode (Vocs's built-in would otherwise fall back to
  // prefers-color-scheme on first visit).
  head: createElement('script', { src: '/default-dark.js' }),
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
