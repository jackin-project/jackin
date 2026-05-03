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
        baseUrl: 'https://github.com/jackin-project/jackin/edit/main/docs/',
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
            { label: 'Role Repos', slug: 'guides/role-repos' },
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
          label: 'Developing Roles',
          items: [
            { label: 'Creating Roles', slug: 'developing/creating-roles' },
            { label: 'Construct Image', slug: 'developing/construct-image' },
            { label: 'Role Manifest', slug: 'developing/role-manifest' },
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
                    { label: 'Multi-runtime support (Codex & Amp)', slug: 'reference/roadmap/multi-runtime-support' },
                    { label: 'Bollard migration', slug: 'reference/roadmap/bollard-migration' },
                    { label: 'Rootless DinD', slug: 'reference/roadmap/rootless-dind' },
                    { label: 'Selectable sandbox backends', slug: 'reference/roadmap/selectable-sandbox-backends' },
                    { label: 'Reproducibility & provenance pinning', slug: 'reference/roadmap/reproducibility-pinning' },
                    { label: 'Per-mount isolation', slug: 'reference/roadmap/per-mount-isolation' },
                    { label: 'Worktree cleanup assessment', slug: 'reference/roadmap/worktree-cleanup-assessment' },
                    { label: 'Devcontainer parity', slug: 'reference/roadmap/devcontainer-parity' },
                    { label: 'Docs markdown linting', slug: 'reference/roadmap/docs-markdown-linting' },
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
                    { label: 'JACKIN_DEBUG env var', slug: 'reference/roadmap/jackin-debug-env-var' },
                  ],
                },
                {
                  label: 'Operator surface',
                  collapsed: true,
                  items: [
                    { label: 'Overview', slug: 'reference/roadmap/multicode-inspired-features' },
                    {
                      label: 'Phase 1 — Foundation gaps',
                      collapsed: true,
                      items: [
                        { label: 'Workspace description', slug: 'reference/roadmap/workspace-description' },
                        { label: 'Operator handler system', slug: 'reference/roadmap/operator-handler-system' },
                        { label: 'Workspace archive', slug: 'reference/roadmap/workspace-archive' },
                        { label: 'Declarative resource limits', slug: 'reference/roadmap/declarative-resource-limits' },
                        { label: 'Ephemeral mount modes', slug: 'reference/roadmap/ephemeral-mount-modes' },
                      ],
                    },
                    {
                      label: 'Phase 2 — Live operator surface',
                      collapsed: true,
                      items: [
                        { label: 'Agent runtime status', slug: 'reference/roadmap/agent-runtime-status' },
                        { label: 'Console resource panel', slug: 'reference/roadmap/console-resource-panel' },
                        { label: 'Agent tag protocol', slug: 'reference/roadmap/agent-tag-protocol' },
                        { label: 'GitHub link tracking', slug: 'reference/roadmap/github-link-tracking' },
                        { label: 'Custom operator tools', slug: 'reference/roadmap/custom-operator-tools' },
                      ],
                    },
                    {
                      label: 'Phase 3 — Persistence & telemetry',
                      collapsed: true,
                      items: [
                        { label: 'Persistent storage layer', slug: 'reference/roadmap/persistent-storage-layer' },
                        { label: 'Token & cost telemetry', slug: 'reference/roadmap/token-cost-telemetry' },
                      ],
                    },
                    {
                      label: 'Phase 4 — Fleet operations',
                      collapsed: true,
                      items: [
                        { label: 'Task source abstraction', slug: 'reference/roadmap/task-source-abstraction' },
                        { label: 'Autonomous task queue', slug: 'reference/roadmap/autonomous-task-queue' },
                        { label: 'Idle runtime cleanup', slug: 'reference/roadmap/idle-runtime-cleanup' },
                      ],
                    },
                    {
                      label: 'Phase 5 — Distributed & extensibility',
                      collapsed: true,
                      items: [
                        { label: 'jackin-remote', slug: 'reference/roadmap/jackin-remote' },
                        { label: 'Credential source pattern', slug: 'reference/roadmap/credential-source-pattern' },
                        { label: 'Workspace skills mount', slug: 'reference/roadmap/workspace-skills-mount' },
                      ],
                    },
                  ],
                },
                {
                  label: 'Codebase health',
                  collapsed: true,
                  items: [
                    { label: 'Overview', slug: 'reference/roadmap/codebase-readability' },
                    {
                      label: 'Phase 1 — Documentation & setup',
                      collapsed: true,
                      items: [
                        { label: 'Module contracts', slug: 'reference/roadmap/module-contracts' },
                        { label: 'Behavioral spec: launch.rs', slug: 'reference/roadmap/behavioral-spec-launch' },
                        { label: 'Behavioral spec: op_picker', slug: 'reference/roadmap/behavioral-spec-op-picker' },
                        { label: 'Per-directory README + AGENTS.md', slug: 'reference/roadmap/per-directory-readme' },
                        { label: 'Developer Reference setup', slug: 'reference/roadmap/developer-reference-setup' },
                        { label: 'Update PROJECT_STRUCTURE.md', slug: 'reference/roadmap/project-structure-update' },
                        { label: 'CI gate: PROJECT_STRUCTURE.md', slug: 'reference/roadmap/ci-project-structure-gate' },
                        { label: 'pub(crate) visibility', slug: 'reference/roadmap/pub-crate-visibility' },
                        { label: 'MSRV & toolchain', slug: 'reference/roadmap/msrv-toolchain' },
                        { label: 'Architecture Decision Records', slug: 'reference/roadmap/architecture-decision-records' },
                        { label: 'Snapshot tests for TUI', slug: 'reference/roadmap/snapshot-tests-tui' },
                        { label: 'Agent workflow: cc-sdd', slug: 'reference/roadmap/agent-workflow-cc-sdd' },
                        { label: 'Move CONTRIBUTING + TESTING', slug: 'reference/roadmap/move-contributing-testing' },
                      ],
                    },
                    {
                      label: 'Phase 2 — File splits',
                      collapsed: true,
                      items: [
                        { label: 'Split input/editor.rs', slug: 'reference/roadmap/split-input-editor' },
                        { label: 'Split app/mod.rs', slug: 'reference/roadmap/split-app-mod' },
                        { label: 'Split operator_env.rs', slug: 'reference/roadmap/split-operator-env' },
                        { label: 'Split runtime/launch.rs', slug: 'reference/roadmap/split-runtime-launch' },
                      ],
                    },
                    {
                      label: 'Phase 3 — Future',
                      collapsed: true,
                      items: [
                        { label: 'Greenfield workspace split', slug: 'reference/roadmap/greenfield-workspace' },
                        { label: 'rustdoc JSON → Starlight', slug: 'reference/roadmap/rustdoc-json-starlight' },
                      ],
                    },
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
