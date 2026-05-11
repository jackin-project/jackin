import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'
import rehypeExternalLinks from 'rehype-external-links'
import rehypeJk from './scripts/rehype-jk'

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
      // Wrap every prose mention of `jackin'` (the project name) with
      // a brand-styled span so the name picks up `--jk-brand` in both
      // light and dark modes. Skips code blocks and command lines.
      rehypeJk,
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
      // Audience-split sidebar.
      //
      // The first three groups (Getting Started, Operator Guide,
      // Commands) are written for **operators** — people who use
      // jackin' to run agents. Those pages describe behaviour,
      // CLI/TUI flows, and operator-level concepts. They never tell
      // the reader to hand-edit `~/.config/jackin/config.toml`.
      //
      // The "Behind jackin'" group is written for **contributors and
      // role authors** — people who want to understand how jackin' is
      // built, hand-edit a config file for debugging, author a role
      // repository, or work on jackin' itself. Internals, schema
      // references, and the development roadmap live there.
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: "Why jackin'?", slug: 'getting-started/why' },
            { label: 'Installation', slug: 'getting-started/installation' },
            { label: 'Quick Start', slug: 'getting-started/quickstart' },
            { label: 'Concepts', slug: 'getting-started/concepts' },
            { label: 'Design Principles', slug: 'getting-started/design-principles' },
          ],
        },
        {
          label: 'Operator Guide',
          items: [
            { label: 'Workspaces', slug: 'guides/workspaces' },
            { label: 'Mounts', slug: 'guides/mounts' },
            { label: 'Environment Variables', slug: 'guides/environment-variables' },
            {
              label: 'Authentication',
              items: [
                { label: 'Overview', slug: 'guides/authentication' },
                { label: 'Agent Authentication', slug: 'guides/authentication/agents' },
                { label: 'GitHub CLI Authentication', slug: 'guides/authentication/github-cli' },
              ],
            },
            { label: 'Security Model', slug: 'guides/security-model' },
            { label: 'Comparison', slug: 'guides/comparison' },
          ],
        },
        {
          // Commands are ordered by the operator's day-to-day flow, not
          // alphabetically. `console` (the TUI) leads because it is the
          // simplest, most convenient surface and the one most people
          // use most of the time. `load` and the rest of the CLI follow
          // — they are the more advanced, scriptable surface, where
          // niche flags land first and there is deliberately no feature
          // parity with the console.
          label: 'Commands',
          items: [
            { label: 'console (TUI)', slug: 'commands/console' },
            { label: 'load', slug: 'commands/load' },
            { label: 'hardline', slug: 'commands/hardline' },
            { label: 'eject', slug: 'commands/eject' },
            { label: 'exile', slug: 'commands/exile' },
            { label: 'purge', slug: 'commands/purge' },
            { label: 'workspace', slug: 'commands/workspace' },
            { label: 'config', slug: 'commands/config' },
          ],
        },
        {
          // Role authoring is a *user-facing* activity — a normal
          // jackin' user creating a backend-engineer / docs-writer /
          // security-reviewer role with their preferred toolchain
          // and plugins. It is NOT a contributor activity, so it
          // gets its own top-level group separate from the
          // contributor-facing Internals group below. Pages are
          // ordered by gradually increasing complexity: what a role
          // is → how to create one → the manifest schema → the base
          // image every role extends.
          label: 'Role Authoring',
          items: [
            { label: 'Role Repositories', slug: 'guides/role-repos' },
            { label: 'Creating a Role', slug: 'developing/creating-roles' },
            { label: 'Role Manifest', slug: 'developing/role-manifest' },
            { label: 'Construct Image', slug: 'developing/construct-image' },
          ],
        },
        {
          label: "Behind jackin' — Internals",
          items: [
            { label: 'Architecture', slug: 'reference/architecture' },
            { label: 'Configuration File', slug: 'reference/configuration' },
            { label: 'Codebase Map', slug: 'reference/codebase-map' },
            { label: 'Claude Token Orchestrator', slug: 'reference/claude-token-orchestrator' },
            {
              label: 'Goal prompts',
              collapsed: true,
              items: [{ label: 'Jackin Desktop Agent Hub', slug: 'reference/goals/jackin-desktop-agent-hub' }],
            },
            {
              // Roadmap groups are flat — every group below is open
              // work. Some groups happen to be phased programs that
              // should be read together (Agent Orchestrator
              // Research, Codebase health); others are loose
              // categories of standalone items. The shape difference
              // is informational, not a separate "active programs" /
              // "open items" distinction. The only real status axis
              // here is open vs. resolved, which is why "Resolved"
              // stays its own bottom group.
              label: 'Roadmap',
              collapsed: false,
              items: [
                { label: 'Overview', slug: 'reference/roadmap' },
                {
                  label: 'Agent Orchestrator Research',
                  collapsed: true,
                  items: [
                    { label: 'Overview', slug: 'reference/roadmap/agent-orchestrator-research' },
                    {
                      label: 'Fleet phase 1 — Foundation gaps',
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
                      label: 'Fleet phase 2 — Live operator surface',
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
                      label: 'Fleet phase 3 — Persistence & telemetry',
                      collapsed: true,
                      items: [
                        { label: 'Persistent storage layer', slug: 'reference/roadmap/persistent-storage-layer' },
                        { label: 'Token & cost telemetry', slug: 'reference/roadmap/token-cost-telemetry' },
                      ],
                    },
                    {
                      label: 'Fleet phase 4 — Fleet operations',
                      collapsed: true,
                      items: [
                        { label: 'Task source abstraction', slug: 'reference/roadmap/task-source-abstraction' },
                        { label: 'Autonomous task queue', slug: 'reference/roadmap/autonomous-task-queue' },
                        { label: 'Idle runtime cleanup', slug: 'reference/roadmap/idle-runtime-cleanup' },
                      ],
                    },
                    {
                      label: 'Fleet phase 5 — Distributed & extensibility',
                      collapsed: true,
                      items: [
                        { label: 'jackin-remote', slug: 'reference/roadmap/jackin-remote' },
                        { label: 'Credential source pattern', slug: 'reference/roadmap/credential-source-pattern' },
                        { label: 'Workspace skills mount', slug: 'reference/roadmap/workspace-skills-mount' },
                      ],
                    },
                    {
                      label: 'Containment — Boundary contract',
                      collapsed: true,
                      items: [
                        { label: 'Session contract and explain mode', slug: 'reference/roadmap/session-contract-explain-mode' },
                        { label: 'Stack integration contracts', slug: 'reference/roadmap/stack-integration-contracts' },
                      ],
                    },
                    {
                      label: 'Containment — Egress & recovery',
                      collapsed: true,
                      items: [
                        { label: 'Network egress policy', slug: 'reference/roadmap/network-egress-policy' },
                        { label: 'Session snapshot and rollback', slug: 'reference/roadmap/session-snapshot-rollback' },
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
                        { label: 'Behavioral spec: runtime/launch.rs', slug: 'reference/roadmap/behavioral-spec-runtime-launch' },
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
                        { label: 'Cargo workspace split', slug: 'reference/roadmap/cargo-workspace-split' },
                        { label: 'rustdoc JSON → Starlight', slug: 'reference/roadmap/rustdoc-json-starlight' },
                      ],
                    },
                  ],
                },
                {
                  label: 'Reactive daemon program',
                  collapsed: true,
                  items: [
                    { label: 'Overview', slug: 'reference/roadmap/jackin-daemon' },
                    { label: 'Jackin Desktop Agent Hub', slug: 'reference/roadmap/jackin-desktop-agent-hub' },
                    {
                      label: 'Phase 2 — First reactive adapters',
                      collapsed: false,
                      items: [
                        { label: 'Live bidirectional auth sync', slug: 'reference/roadmap/live-auth-sync' },
                        { label: 'Agent attention prompts', slug: 'reference/roadmap/agent-attention-prompts' },
                      ],
                    },
                    {
                      label: 'Phase 3 — Operator-mediated host bridge',
                      collapsed: false,
                      items: [
                        { label: 'Host bridge — secrets and approved host actions', slug: 'reference/roadmap/host-bridge' },
                        { label: 'Container credential exposure — beyond env injection', slug: 'reference/roadmap/container-credential-exposure' },
                      ],
                    },
                  ],
                },
                {
                  label: 'Agent runtimes & authentication',
                  collapsed: true,
                  items: [
                    { label: 'Multi-runtime support (Codex & Amp)', slug: 'reference/roadmap/multi-runtime-support' },
                    { label: 'Reliable Claude authentication strategy', slug: 'reference/roadmap/claude-auth-strategy' },
                    { label: 'Workspace Claude token setup', slug: 'reference/roadmap/workspace-claude-token-setup' },
                    { label: 'GitHub CLI authentication strategy', slug: 'reference/roadmap/github-cli-auth-strategy' },
                    { label: '1Password integration', slug: 'reference/roadmap/onepassword-integration' },
                  ],
                },
                {
                  label: 'Isolation & security',
                  collapsed: true,
                  items: [
                    { label: 'Rootless DinD', slug: 'reference/roadmap/rootless-dind' },
                    { label: 'Selectable sandbox backends', slug: 'reference/roadmap/selectable-sandbox-backends' },
                    { label: 'Reproducibility & provenance pinning', slug: 'reference/roadmap/reproducibility-pinning' },
                    { label: 'Devcontainer parity', slug: 'reference/roadmap/devcontainer-parity' },
                    { label: 'Open review findings', slug: 'reference/roadmap/open-review-findings' },
                  ],
                },
                {
                  label: 'Infrastructure',
                  collapsed: true,
                  items: [
                    { label: 'Bollard migration', slug: 'reference/roadmap/bollard-migration' },
                    { label: 'Construct user creation', slug: 'reference/roadmap/construct-user-creation' },
                  ],
                },
                {
                  label: 'Documentation tooling',
                  collapsed: true,
                  items: [
                    { label: 'Docs markdown linting', slug: 'reference/roadmap/docs-markdown-linting' },
                    { label: 'Move documentation to a separate repository', slug: 'reference/roadmap/docs-separate-repository' },
                  ],
                },
                {
                  label: 'Configuration ergonomics',
                  collapsed: true,
                  items: [
                    { label: 'Split config.toml into per-workspace files', slug: 'reference/roadmap/split-workspace-config-files' },
                  ],
                },
                {
                  label: 'Resolved',
                  collapsed: true,
                  items: [
                    { label: 'Agent source trust', slug: 'reference/roadmap/agent-source-trust' },
                    { label: 'Custom plugin marketplace', slug: 'reference/roadmap/custom-plugin-marketplace' },
                    { label: 'DinD hostname env var', slug: 'reference/roadmap/dind-hostname-env-var' },
                    { label: 'DinD TLS', slug: 'reference/roadmap/dind-tls' },
                    { label: 'Env var interpolation', slug: 'reference/roadmap/env-var-interpolation' },
                    { label: 'JACKIN_DEBUG env var', slug: 'reference/roadmap/jackin-debug-env-var' },
                    { label: 'Orphaned DinD cleanup', slug: 'reference/roadmap/orphaned-dind-cleanup' },
                    { label: 'Per-mount isolation', slug: 'reference/roadmap/per-mount-isolation' },
                    { label: 'Sensitive mount warnings', slug: 'reference/roadmap/sensitive-mount-warnings' },
                    { label: 'Worktree cleanup assessment', slug: 'reference/roadmap/worktree-cleanup-assessment' },
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
