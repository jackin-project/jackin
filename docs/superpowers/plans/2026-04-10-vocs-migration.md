# Vocs Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Astro + Starlight docs site with a Vocs (React + Vite) site using Tailwind CSS 4, migrating all 24 MDX content pages.

**Architecture:** Clean rebuild in the existing `docs/` directory. Delete all Astro-specific files, scaffold the Vocs project structure with `vite.config.ts` + `vocs.config.ts`, then migrate each MDX file by converting Starlight component syntax to Vocs equivalents (directives like `:::note`, `::::steps`, `:::code-group`). Landing page uses Vocs `HomePage` component.

**Tech Stack:** Vocs 1.4.1, React 19, Vite 7, Tailwind CSS 4, TypeScript 6

**Spec:** `docs/superpowers/specs/2026-04-10-vocs-migration-design.md`

---

## File Structure

### Files to Delete

- `docs/astro.config.mjs` — Astro configuration
- `docs/src/content/` — entire Starlight content directory
- `docs/src/content.config.ts` — Starlight content collections config
- `docs/src/assets/` — SVG logos (starting fresh)
- `docs/src/styles/custom.css` — Starlight theme overrides
- `docs/bun.lock` — old lockfile
- `docs/scripts/generate-logos.mjs` — logo generation script (no longer needed)

### Files to Create

- `docs/vocs.config.ts` — Vocs sidebar/metadata configuration
- `docs/vite.config.ts` — Vite 7 with vocs() and react() plugins
- `docs/tsconfig.json` — references file
- `docs/tsconfig.app.json` — React/Vocs app config
- `docs/tsconfig.node.json` — Vite config typing
- `docs/src/env.d.ts` — Vocs type declarations
- `docs/src/pages/_root.css` — Tailwind v4 + Vocs dark mode
- `docs/src/pages/index.mdx` — landing page (Vocs HomePage)
- `docs/src/pages/getting-started/*.mdx` — 4 pages
- `docs/src/pages/guides/*.mdx` — 5 pages
- `docs/src/pages/commands/*.mdx` — 8 pages
- `docs/src/pages/developing/*.mdx` — 3 pages
- `docs/src/pages/reference/*.mdx` — 3 pages

### Files to Modify

- `docs/package.json` — replace Astro deps with Vocs deps, update scripts
- `docs/.gitignore` — update for Vocs artifacts
- `docs/AGENTS.md` — update stack documentation
- `docs/public/CNAME` — keep as-is (no change needed)

---

### Task 1: Delete Astro/Starlight Files

**Files:**
- Delete: `docs/astro.config.mjs`
- Delete: `docs/src/content/` (entire directory)
- Delete: `docs/src/content.config.ts`
- Delete: `docs/src/assets/` (entire directory)
- Delete: `docs/src/styles/` (entire directory)
- Delete: `docs/bun.lock`
- Delete: `docs/scripts/generate-logos.mjs`
- Delete: `docs/node_modules/` (will reinstall)

- [ ] **Step 1: Remove all Astro-specific files and directories**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
rm -f astro.config.mjs bun.lock src/content.config.ts scripts/generate-logos.mjs
rm -rf src/content src/assets src/styles node_modules
```

- [ ] **Step 2: Verify only scaffolding remains**

```bash
ls -la /Users/donbeave/Projects/jackin-project/jackin/docs/
```

Expected: `package.json`, `tsconfig.json`, `public/`, `src/` (empty or near-empty), `.mise.toml`, `AGENTS.md`, `CLAUDE.md`, `.gitignore`, `superpowers/`

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "docs: remove Astro/Starlight files

Prepares for Vocs migration by removing all Astro-specific config,
Starlight content collections, logo assets, and generated lockfile."
```

---

### Task 2: Scaffold Vocs Project

**Files:**
- Modify: `docs/package.json`
- Create: `docs/vocs.config.ts`
- Create: `docs/vite.config.ts`
- Create: `docs/tsconfig.json`
- Create: `docs/tsconfig.app.json`
- Create: `docs/tsconfig.node.json`
- Create: `docs/src/env.d.ts`
- Create: `docs/src/pages/_root.css`
- Modify: `docs/.gitignore`

- [ ] **Step 1: Replace package.json**

Replace the contents of `docs/package.json` with:

```json
{
  "name": "jackin-docs",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.2.5",
    "react-dom": "^19.2.5",
    "vocs": "^1.4.1"
  },
  "devDependencies": {
    "@vitejs/plugin-react": "^5.2.0",
    "tailwindcss": "^4.2.2",
    "typescript": "^6.0.2",
    "vite": "^7.1.11"
  }
}
```

- [ ] **Step 2: Create vocs.config.ts**

Create `docs/vocs.config.ts`:

```ts
import { defineConfig } from 'vocs'

export default defineConfig({
  title: "jackin'",
  titleTemplate: "%s — jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated containers',
  rootDir: '.',
  baseUrl: 'https://jackin.tailrocks.com',
  editLink: {
    pattern:
      'https://github.com/jackin-project/jackin/edit/main/docs/src/pages/:path',
    text: 'Edit on GitHub',
  },
  socials: [
    { icon: 'github', link: 'https://github.com/jackin-project/jackin' },
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
```

- [ ] **Step 3: Create vite.config.ts**

Create `docs/vite.config.ts`:

```ts
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import vocs from 'vocs/vite'

export default defineConfig({
  plugins: [vocs(), react()],
})
```

- [ ] **Step 4: Create TypeScript configs**

Create `docs/tsconfig.json`:

```json
{
  "references": [
    { "path": "./tsconfig.app.json" },
    { "path": "./tsconfig.node.json" }
  ]
}
```

Create `docs/tsconfig.app.json`:

```json
{
  "compilerOptions": {
    "target": "ES2024",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "paths": { "~/*": ["./src/*"] },
    "types": ["vite/client", "vocs/globals"]
  },
  "include": ["src/"]
}
```

Create `docs/tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "target": "ES2024",
    "module": "ESNext",
    "moduleResolution": "bundler"
  },
  "include": ["vite.config.ts", "vocs.config.ts"]
}
```

- [ ] **Step 5: Create env.d.ts**

Create `docs/src/env.d.ts`:

```ts
/// <reference types="vite/client" />
/// <reference types="vocs/globals" />
```

- [ ] **Step 6: Create _root.css with Tailwind v4**

Create `docs/src/pages/_root.css`:

```css
@import "tailwindcss";

@source "./";

@custom-variant dark (&:where([style*="color-scheme: dark"], [style*="color-scheme: dark"] *));
```

- [ ] **Step 7: Update .gitignore**

Replace `docs/.gitignore` with:

```
node_modules/
dist/
.vocs/
src/pages.gen.ts
*.local
package-lock.json
```

- [ ] **Step 8: Install dependencies**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun install
```

If bun fails to resolve dependencies, fall back to pnpm:

```bash
rm -rf node_modules bun.lock
npm install -g pnpm
pnpm install
```

- [ ] **Step 9: Verify dev server starts**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Expected: Vite dev server starts on http://localhost:5173 (may show 404 since no pages exist yet — that's fine).

- [ ] **Step 10: Commit**

```bash
git add -A && git commit -m "docs: scaffold Vocs project with Tailwind CSS 4

Adds vocs.config.ts, vite.config.ts, TypeScript configs, Tailwind v4
setup with Vocs dark mode variant, and updated package.json with all
dependencies."
```

---

### Task 3: Create Landing Page

**Files:**
- Create: `docs/src/pages/index.mdx`

The landing page replaces the Starlight splash template with Vocs `HomePage` component.

- [ ] **Step 1: Create the landing page**

Create `docs/src/pages/index.mdx`:

```mdx
---
layout: landing
---

import { HomePage } from 'vocs/components'

<HomePage>
  <HomePage.Tagline>jackin'</HomePage.Tagline>
  <HomePage.Description>
    Isolate AI coding agents in Docker containers.
    You're the Operator. They're already inside.
  </HomePage.Description>
  <HomePage.InstallPackage name="jackin" type="brew" />
  <HomePage.Buttons>
    <HomePage.Button href="/getting-started/why" variant="accent">
      Get Started
    </HomePage.Button>
    <HomePage.Button href="https://github.com/jackin-project/jackin">
      GitHub
    </HomePage.Button>
  </HomePage.Buttons>
</HomePage>

## Why jackin'?

AI coding agents like Claude Code are most productive when they can run **without permission prompts** — reading files, executing commands, installing packages freely. But giving an agent unrestricted access to your host machine means it can see your entire filesystem, access your credentials, and modify anything.

**jackin' solves this** by giving each agent its own isolated Docker container with Docker-in-Docker enabled. The agent thinks it has free rein — but it's operating inside a construct you defined.

## Think in two dimensions

- **Agent class** = the tool profile. Which image, plugins, defaults, and runtime behavior should this agent have?
- **Workspace** = the access boundary. Which project files should this agent be allowed to see?

That separation is the point. A backend agent and frontend agent can work on the same repository workspace without sharing one giant all-knowing image.

:::note
jackin' is intentionally transparent about its tradeoffs. It is a container-based isolation model, not a microVM sandbox. If you want the honest comparison with Docker Sandboxes and related approaches, read [Comparison with Alternatives](/guides/comparison/).
:::

## Quick start

```bash
# Install via Homebrew
brew tap jackin-project/tap

# Stable
brew install jackin

# Or install the rolling preview channel
brew install jackin@preview

# Load an agent into the current directory
jackin load agent-smith

# Or use the interactive launcher
jackin launch
```

`agent-smith` is just the default starter agent class name in this project. Your own classes might be named `frontend`, `backend`, or `review-only`.

## Explore

- [Installation](/getting-started/installation) — All the ways to install jackin'
- [Core Concepts](/getting-started/concepts) — Understand operators, agents, and constructs
- [Creating Agents](/developing/creating-agents) — Build your own agent repos
- [Security Model](/guides/security-model) — How jackin' keeps your system safe
- [Comparison](/guides/comparison) — jackin' vs Docker Sandboxes and alternatives

## Ecosystem

| Repository | Description |
|---|---|
| [jackin](https://github.com/jackin-project/jackin) | CLI source code |
| [jackin-agent-smith](https://github.com/jackin-project/jackin-agent-smith) | Default general-purpose agent |
| [jackin-the-architect](https://github.com/jackin-project/jackin-the-architect) | Rust development agent (used for jackin' development) |
| [homebrew-tap](https://github.com/jackin-project/homebrew-tap) | Homebrew formulae for installing jackin' |
```

- [ ] **Step 2: Verify the landing page renders**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Open http://localhost:5173 — should see the HomePage component with tagline, install command, and buttons.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "docs: add Vocs landing page with HomePage component"
```

---

### Task 4: Migrate Getting Started Section (4 pages)

**Files:**
- Create: `docs/src/pages/getting-started/why.mdx`
- Create: `docs/src/pages/getting-started/installation.mdx`
- Create: `docs/src/pages/getting-started/quickstart.mdx`
- Create: `docs/src/pages/getting-started/concepts.mdx`

**Migration rules applied:**
- Strip `title` from frontmatter, keep `description`
- Add `# Title` as first heading (Vocs uses first heading as page title)
- Remove all `import` statements for Starlight components
- `<Aside type="note">` or `<Aside>` → `:::note` / `:::`
- `<Aside type="tip">` → `:::tip` / `:::`
- `<Aside type="caution">` → `:::warning` / `:::`
- `<Steps>` + numbered list → `::::steps` + `### Step N` headings
- `<Tabs>/<TabItem>` with code blocks → `:::code-group` with labeled code blocks

- [ ] **Step 1: Create getting-started directory**

```bash
mkdir -p /Users/donbeave/Projects/jackin-project/jackin/docs/src/pages/getting-started
```

- [ ] **Step 2: Migrate why.mdx**

Copy `docs/src/content/docs/getting-started/why.mdx` to `docs/src/pages/getting-started/why.mdx` and transform:

1. Replace frontmatter: keep only `description`, remove `title`
2. Add `# Why jackin'?` as first line after frontmatter
3. Remove `import { Aside, Steps } from '@astrojs/starlight/components';`
4. Replace `<Steps>` block (lines 31-37) with:

```mdx
::::steps

### You define the boundaries

Which directories to mount, whether they're read-only or read-write, what tools are available in the container image

### The agent runs freely inside those boundaries

No permission prompts, no restrictions, full `--dangerously-skip-permissions` mode

### Your host system stays untouched

The agent can't see your home directory, can't access your credentials, can't modify files outside its mounts

::::
```

5. Replace `<Aside type="caution">` (lines 122-124) with:

```mdx
:::warning
jackin' is not a general-purpose container orchestrator. It's specifically designed for AI coding agents.
:::
```

6. Replace `<Aside>` (lines 154-156) with:

```mdx
:::note
jackin' is currently a proof of concept with Claude Code as its only supported agent runtime. Support for Codex and Amp Code is planned.
:::
```

- [ ] **Step 3: Migrate installation.mdx**

Copy and transform `docs/src/content/docs/getting-started/installation.mdx` to `docs/src/pages/getting-started/installation.mdx`:

1. Frontmatter: keep only `description`
2. Add `# Installation` as first heading
3. Remove import statement
4. Replace `<Tabs>/<TabItem>` block (lines 15-41) with:

```mdx
:::code-group

```bash [Homebrew]
brew tap jackin-project/tap

# Stable
brew install jackin
```

```bash [From source]
# Requires Rust 1.87 or newer
cargo install --git https://github.com/jackin-project/jackin.git
```

:::
```

Note: The Homebrew tab had additional prose ("The easiest way..." and "Or install the rolling preview..."). Move this prose outside the code group — before it as an intro paragraph, and after it as a separate paragraph about the preview channel.

5. Replace `<Aside type="tip">` (lines 63-65) with:

```mdx
:::tip
On macOS, [OrbStack](https://orbstack.dev/) is a lightweight alternative to Docker Desktop that works well with jackin'.
:::
```

- [ ] **Step 4: Migrate quickstart.mdx**

Copy and transform `docs/src/content/docs/getting-started/quickstart.mdx` to `docs/src/pages/getting-started/quickstart.mdx`:

1. Frontmatter: keep only `description`
2. Add `# Quick Start` as first heading
3. Remove import statement
4. Replace `<Steps>` block (lines 12-39) with:

```mdx
::::steps

### Navigate to your project directory

```bash
cd ~/Projects/my-app
```

### Load an agent

```bash
jackin load agent-smith
```

The first time you run this, jackin' will:
- Pull the construct base image
- Clone the agent-smith repository
- Build a derived Docker image with Claude Code installed
- Create an isolated Docker network
- Launch the container with your current directory mounted

### You're inside

Claude Code starts automatically with full permissions inside the container. Your project directory is mounted at the same path, so the agent sees the same file layout you do.

### Exit when done

Press `Ctrl+C` or type `/exit` in Claude Code to leave. The container stops, but its state is persisted for next time.

::::
```

5. Replace `<Aside type="tip">` (lines 56-58) with:

```mdx
:::tip
`launch` is the human-first flow — visual and interactive. `load` is the terminal-first flow — explicit and scriptable.
:::
```

6. Replace `<Tabs>/<TabItem>` block (lines 82-125) with:

```mdx
:::code-group

```bash [Daily development]
# Start of day — load agent into your project
jackin load agent-smith ~/Projects/my-app

# Work with Claude Code inside the container...

# End of day — stop the agent
jackin eject agent-smith
```

```bash [Multiple projects]
# Save workspaces for your projects
jackin workspace add frontend --workdir ~/Projects/frontend
jackin workspace add backend --workdir ~/Projects/backend

# Load agents into different workspaces
jackin load agent-smith frontend
jackin load agent-smith backend

# Both agents run simultaneously in isolated containers
```

```bash [One workspace, different agents]
# Same project access boundary, different tool profiles
jackin workspace add monorepo --workdir ~/Projects/monorepo

jackin load agent-smith monorepo
jackin load the-architect monorepo

# Same files, different environment and plugins
```

```bash [Read-only access]
# Mount a reference codebase as read-only
jackin load agent-smith ~/Projects/my-app \
  --mount ~/Projects/shared-lib:/shared:ro
```

:::
```

- [ ] **Step 5: Migrate concepts.mdx**

Copy and transform `docs/src/content/docs/getting-started/concepts.mdx` to `docs/src/pages/getting-started/concepts.mdx`:

1. Frontmatter: keep only `description`
2. Add `# Core Concepts` as first heading
3. Remove import statement
4. Replace `<Aside type="note">` with `:::note` / `:::`

- [ ] **Step 6: Verify all 4 pages render**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Navigate to `/getting-started/why`, `/getting-started/installation`, `/getting-started/quickstart`, `/getting-started/concepts` — all should render correctly with callouts, steps, and code groups.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "docs: migrate Getting Started section to Vocs

Converts 4 pages: why, installation, quickstart, concepts.
Replaces Starlight Aside/Steps/Tabs with Vocs directives."
```

---

### Task 5: Migrate Guides Section (5 pages)

**Files:**
- Create: `docs/src/pages/guides/workspaces.mdx`
- Create: `docs/src/pages/guides/mounts.mdx`
- Create: `docs/src/pages/guides/agent-repos.mdx`
- Create: `docs/src/pages/guides/security-model.mdx`
- Create: `docs/src/pages/guides/comparison.mdx`

**Migration rules:** Same as Task 4. All guide pages use only `<Aside>` — no Steps or Tabs.

- [ ] **Step 1: Create guides directory**

```bash
mkdir -p /Users/donbeave/Projects/jackin-project/jackin/docs/src/pages/guides
```

- [ ] **Step 2: Migrate all 5 guide pages**

For each file in `docs/src/content/docs/guides/`:
1. Copy to `docs/src/pages/guides/`
2. Strip `title` from frontmatter, keep `description`
3. Add `# {Title}` as first heading
4. Remove all Starlight import statements
5. Convert all `<Aside>` instances:
   - `<Aside type="note">content</Aside>` → `:::note\ncontent\n:::`
   - `<Aside type="tip">content</Aside>` → `:::tip\ncontent\n:::`
   - `<Aside type="caution">content</Aside>` → `:::warning\ncontent\n:::`
   - `<Aside>content</Aside>` (no type) → `:::note\ncontent\n:::`

Files and their Aside usage:
- **workspaces.mdx**: 1x `<Aside type="tip">`
- **mounts.mdx**: 1x `<Aside type="caution">`
- **agent-repos.mdx**: 1x `<Aside type="tip">`, 1x `<Aside type="caution">`
- **security-model.mdx**: 1x `<Aside type="note">`, 1x `<Aside type="caution">`
- **comparison.mdx**: 1x `<Aside type="note">`

- [ ] **Step 3: Verify all 5 pages render**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Navigate to each guide page and verify callouts render correctly.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: migrate Guides section to Vocs

Converts 5 pages: workspaces, mounts, agent-repos, security-model,
comparison. Replaces Starlight Aside with Vocs callout directives."
```

---

### Task 6: Migrate Commands Section (8 pages)

**Files:**
- Create: `docs/src/pages/commands/load.mdx`
- Create: `docs/src/pages/commands/launch.mdx`
- Create: `docs/src/pages/commands/hardline.mdx`
- Create: `docs/src/pages/commands/eject.mdx`
- Create: `docs/src/pages/commands/exile.mdx`
- Create: `docs/src/pages/commands/purge.mdx`
- Create: `docs/src/pages/commands/workspace.mdx`
- Create: `docs/src/pages/commands/config.mdx`

**Migration rules:** Same Aside conversion. Most command pages are pure markdown with no Starlight components. Only `load.mdx` has Asides (3x note, 1x caution, 1x tip).

- [ ] **Step 1: Create commands directory**

```bash
mkdir -p /Users/donbeave/Projects/jackin-project/jackin/docs/src/pages/commands
```

- [ ] **Step 2: Migrate all 8 command pages**

For each file in `docs/src/content/docs/commands/`:
1. Copy to `docs/src/pages/commands/`
2. Strip `title` from frontmatter, keep `description`
3. Add `# jackin {command}` as first heading (matching existing titles)
4. Remove any Starlight import statements
5. Convert any `<Aside>` instances (only in `load.mdx`)

For the 7 pages without Starlight components (config, eject, exile, hardline, launch, purge, workspace): the only changes are frontmatter and removing the title field.

- [ ] **Step 3: Verify all 8 pages render**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Navigate to each command page.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: migrate Commands section to Vocs

Converts 8 command reference pages. Most are pure markdown;
load.mdx Asides converted to Vocs callout directives."
```

---

### Task 7: Migrate Developing Agents Section (3 pages)

**Files:**
- Create: `docs/src/pages/developing/creating-agents.mdx`
- Create: `docs/src/pages/developing/construct-image.mdx`
- Create: `docs/src/pages/developing/agent-manifest.mdx`

**Migration rules:** Same Aside conversion. `creating-agents.mdx` also has a `<Steps>` block.

- [ ] **Step 1: Create developing directory**

```bash
mkdir -p /Users/donbeave/Projects/jackin-project/jackin/docs/src/pages/developing
```

- [ ] **Step 2: Migrate creating-agents.mdx**

Copy and transform:
1. Frontmatter: keep only `description`
2. Add `# Creating an Agent` as first heading
3. Remove import statement
4. Replace `<Steps>` block (lines 14-55) with:

```mdx
::::steps

### Create a GitHub repository

Name it `jackin-{your-agent-name}`. For example, `jackin-rustacean` for a Rust-focused agent.

### Create the manifest file

```toml [jackin.agent.toml]
dockerfile = "Dockerfile"

[claude]
plugins = []

[identity]
name = "Rustacean"
```

### Create the Dockerfile

```dockerfile [Dockerfile]
FROM projectjackin/construct:trixie

# Install Rust via mise
RUN mise install rust@stable && mise use --global rust@stable

# Install additional Rust tools
RUN cargo install cargo-nextest cargo-watch
```

### Push to GitHub

```bash
git add -A && git commit -m "Initial agent setup"
git push origin main
```

### Load your agent

```bash
jackin load rustacean
```

::::
```

5. Replace `<Aside type="caution">` (lines 67-69) with:

```mdx
:::warning
This is the literal string jackin' checks for. `FROM projectjackin/construct:latest` or any other tag will fail validation.
:::
```

- [ ] **Step 3: Migrate construct-image.mdx and agent-manifest.mdx**

For each file:
1. Frontmatter: keep only `description`
2. Add appropriate `# Title` heading
3. Remove import statements
4. Convert all `<Aside>` instances to `:::note`/`:::tip`/`:::warning` directives

construct-image.mdx: 1x note, 1x caution
agent-manifest.mdx: 2x tip

- [ ] **Step 4: Verify all 3 pages render**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Navigate to each developing page. Check that steps render as numbered sections in creating-agents.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "docs: migrate Developing Agents section to Vocs

Converts 3 pages: creating-agents, construct-image, agent-manifest.
Steps and Asides converted to Vocs directives."
```

---

### Task 8: Migrate Reference Section (3 pages)

**Files:**
- Create: `docs/src/pages/reference/configuration.mdx`
- Create: `docs/src/pages/reference/architecture.mdx`
- Create: `docs/src/pages/reference/roadmap.mdx`

**Migration rules:** Same Aside conversion. `configuration.mdx` has no components. `architecture.mdx` has 1 note. `roadmap.mdx` has 1 untyped Aside.

- [ ] **Step 1: Create reference directory**

```bash
mkdir -p /Users/donbeave/Projects/jackin-project/jackin/docs/src/pages/reference
```

- [ ] **Step 2: Migrate all 3 reference pages**

For each file:
1. Frontmatter: keep only `description`
2. Add appropriate `# Title` heading
3. Remove any import statements
4. Convert Asides:
   - architecture.mdx: `<Aside type="note">` → `:::note`
   - roadmap.mdx: `<Aside>` (untyped) → `:::note`
   - configuration.mdx: no changes needed (pure markdown)

- [ ] **Step 3: Verify all 3 pages render**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run dev
```

Navigate to each reference page.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: migrate Reference section to Vocs

Converts 3 pages: configuration, architecture, roadmap.
Aside callouts converted to Vocs note directives."
```

---

### Task 9: Update AGENTS.md and Final Verification

**Files:**
- Modify: `docs/AGENTS.md`

- [ ] **Step 1: Update AGENTS.md**

Replace `docs/AGENTS.md` with:

```markdown
# Docs AGENTS.md

## Stack

- This directory is a Vocs documentation site (React + Vite).
- Package manager and lockfile: `bun` preferred, `pnpm` as fallback.
- Framework: `vocs` with `vite`.
- Styling: `tailwindcss` v4 (CSS-first configuration in `src/pages/_root.css`).

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- If bun has compatibility issues with Vocs, use `pnpm` instead.

## Common Commands

- Install dependencies: `bun install --frozen-lockfile`
- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`

## Content Notes

- Treat this as a Vocs documentation site.
- Main docs content lives under `docs/src/pages/`.
- File-based routing: `src/pages/foo/bar.mdx` → `/foo/bar`.
- Sidebar is configured in `vocs.config.ts`.
- Use Vocs directives for callouts (`:::note`, `:::tip`, `:::warning`), steps (`::::steps`), and code groups (`:::code-group`).
- Keep docs and code behavior aligned; when they differ, code is the source of truth.
```

- [ ] **Step 2: Run a full build**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run build
```

Expected: Build succeeds with no errors. Output goes to `dist/`.

- [ ] **Step 3: Preview the built site**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run preview
```

Spot-check:
- Landing page renders with HomePage component
- Sidebar shows all 5 sections with correct links
- Each page renders content correctly
- Callouts (note/tip/warning) display properly
- Steps render as numbered sections
- Code groups show tabs
- Edit links point to correct GitHub paths

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: update AGENTS.md for Vocs stack

Updates stack documentation to reflect the migration from
Astro/Starlight to Vocs with Tailwind CSS 4."
```

---

### Task 10: Clean Up and Delete Old Content Directory

**Files:**
- Verify deletion: `docs/src/content/` should already be deleted in Task 1
- Verify deletion: `docs/scripts/generate-logos.mjs` should already be deleted
- Delete: any remaining empty directories

- [ ] **Step 1: Verify no Astro/Starlight remnants remain**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
# Should find nothing:
grep -r "astro" --include="*.ts" --include="*.json" --include="*.mjs" . 2>/dev/null || echo "Clean"
grep -r "starlight" --include="*.ts" --include="*.json" --include="*.mjs" . 2>/dev/null || echo "Clean"
```

- [ ] **Step 2: Verify no Starlight imports in migrated MDX**

```bash
grep -r "@astrojs/starlight" docs/src/pages/ 2>/dev/null || echo "Clean"
```

Expected: "Clean" — no Starlight imports remain.

- [ ] **Step 3: Remove any empty directories**

```bash
find /Users/donbeave/Projects/jackin-project/jackin/docs/src -type d -empty -delete 2>/dev/null
find /Users/donbeave/Projects/jackin-project/jackin/docs/scripts -type d -empty -delete 2>/dev/null
```

- [ ] **Step 4: Final commit if any cleanup was needed**

```bash
git add -A && git status
# Only commit if there are changes:
git diff --cached --quiet || git commit -m "docs: clean up remaining Astro artifacts"
```
