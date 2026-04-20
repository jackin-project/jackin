# Astro Starlight Migration — Plan 1 (Phases 0-3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a working Astro + Starlight documentation site at `jackin/docs-astro/` that serves all 25 current docs pages plus the landing page, with visual output indistinguishable from the current Vocs site at `jackin.tailrocks.com`. Ship at the Phase 3 checkpoint — Pass A (islands-only) landing is live in the new site; Phase 4's incremental Astro port of landing components is a separate plan.

**Architecture:** Sibling directory approach (`docs-astro/` alongside existing `docs/`). All work on branch `feature/astro-starlight-migration`. Phase 0 scaffolds a blank Starlight site; Phase 1 ports the theme (Radix tokens + Starlight CSS vars); Phase 2 codemods 25 MDX pages; Phase 3 copies landing React components byte-for-byte and mounts them via a single Astro page. Current `docs/` (Vocs) remains untouched throughout.

**Tech Stack:** Astro 5, Starlight 0.30+, `@astrojs/react`, `@astrojs/mdx`, `@astrojs/starlight-tailwind`, Tailwind v4 (CSS-first), Bun, React 19, TypeScript. Pagefind for search (built into Starlight).

**Reference spec:** `docs/superpowers/specs/2026-04-20-astro-starlight-migration-design.md`

---

## File Structure (end state after this plan)

```
jackin/
├── docs/                                  # Vocs site, unchanged
└── docs-astro/
    ├── .gitignore
    ├── AGENTS.md                          # Astro + Starlight conventions
    ├── CLAUDE.md                          # @AGENTS.md pointer
    ├── astro.config.mjs                   # Full config (starlight + react + mdx + tailwind)
    ├── package.json
    ├── bun.lock
    ├── tsconfig.json
    ├── public/
    │   └── CNAME                          # jackin.tailrocks.com
    └── src/
        ├── content/
        │   ├── config.ts                  # Starlight collection schema
        │   └── docs/                      # 25 codemodded MDX pages
        │       ├── getting-started/
        │       │   ├── why.mdx
        │       │   ├── installation.mdx
        │       │   ├── quickstart.mdx
        │       │   └── concepts.mdx
        │       ├── guides/
        │       │   ├── workspaces.mdx
        │       │   ├── mounts.mdx
        │       │   ├── authentication.mdx
        │       │   ├── agent-repos.mdx
        │       │   ├── security-model.mdx
        │       │   └── comparison.mdx
        │       ├── commands/
        │       │   ├── load.mdx
        │       │   ├── launch.mdx
        │       │   ├── hardline.mdx
        │       │   ├── eject.mdx
        │       │   ├── exile.mdx
        │       │   ├── purge.mdx
        │       │   ├── workspace.mdx
        │       │   └── config.mdx
        │       ├── developing/
        │       │   ├── creating-agents.mdx
        │       │   ├── construct-image.mdx
        │       │   └── agent-manifest.mdx
        │       └── reference/
        │           ├── configuration.mdx
        │           ├── architecture.mdx
        │           └── roadmap.mdx
        ├── components/
        │   ├── landing/                   # 15 React components (copied byte-for-byte)
        │   │   ├── Landing.tsx
        │   │   ├── HeroStage.tsx
        │   │   ├── HeroContent.tsx
        │   │   ├── DigitalRain.tsx
        │   │   ├── CodePanel.tsx
        │   │   ├── VocabularyDictionary.tsx
        │   │   ├── PillCards.tsx
        │   │   ├── ApproachCards.tsx
        │   │   ├── TabbedBuilder.tsx
        │   │   ├── CastRoster.tsx
        │   │   ├── CompositionMachine.tsx
        │   │   ├── FocusCallout.tsx
        │   │   ├── DailyLoop.tsx
        │   │   ├── InstallBlock.tsx
        │   │   ├── WordmarkFooter.tsx
        │   │   ├── rainEngine.ts
        │   │   ├── rainEngine.test.ts
        │   │   ├── loopData.tsx
        │   │   ├── machineData.ts
        │   │   ├── vocabularyData.ts
        │   │   └── styles.css
        │   └── overrides/
        │       └── Head.astro              # Font injection
        ├── pages/
        │   └── index.astro                 # Landing page (Pass A — islands-only)
        └── styles/
            ├── global.css                  # Tailwind v4 + @theme tokens
            ├── tempo-tokens.css            # Copied unchanged
            └── docs-theme.css              # Rewritten for Starlight CSS vars
```

**Migration artifacts (deleted before final commit):**
- `docs-astro/scripts/migrate-mdx.ts` — one-shot codemod, used in Phase 2 only

---

## Phase 0 — Scaffold

### Task 1: Create the `docs-astro/` directory and initial `package.json`

**Files:**
- Create: `docs-astro/package.json`
- Create: `docs-astro/.gitignore`

- [ ] **Step 1: Create the directory**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
mkdir -p docs-astro
cd docs-astro
```

- [ ] **Step 2: Create `package.json`**

Write `docs-astro/package.json`:

```json
{
  "name": "jackin-docs",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview",
    "astro": "astro",
    "test": "bun test"
  },
  "dependencies": {
    "@astrojs/mdx": "^4.0.0",
    "@astrojs/react": "^4.0.0",
    "@astrojs/starlight": "^0.30.0",
    "@astrojs/starlight-tailwind": "^3.0.0",
    "astro": "^5.0.0",
    "react": "^19.2.5",
    "react-dom": "^19.2.5",
    "sharp": "^0.33.0",
    "tailwindcss": "^4.2.2"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "typescript": "^6.0.2"
  }
}
```

**Note on versions:** If `bun install` reports a resolution error for any `^` range (e.g., Starlight 0.30 hasn't been published yet), update to the latest published minor. Check with `bun pm view @astrojs/starlight version` and adjust. Do not add lower bounds that conflict with peer deps.

- [ ] **Step 3: Create `.gitignore`**

Write `docs-astro/.gitignore`:

```
# build output
dist/
.astro/

# dependencies
node_modules/

# logs
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*

# environment variables
.env
.env.production

# macOS
.DS_Store

# editor
.vscode/
.idea/
```

- [ ] **Step 4: Install dependencies**

Run: `cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro && bun install`

Expected: resolves all deps, creates `bun.lock`, creates `node_modules/`.

If any peer-dep warning mentions Astro + Starlight version mismatch, upgrade the mismatched package to the version Starlight expects.

- [ ] **Step 5: Commit the scaffold**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/package.json docs-astro/.gitignore docs-astro/bun.lock
git commit -m "$(cat <<'EOF'
chore(docs-astro): scaffold package.json and gitignore

First step of Vocs → Astro Starlight migration. Sibling directory
docs-astro/ will be promoted to docs/ at Phase 6 cutover.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Create `astro.config.mjs` with minimal Starlight config

**Files:**
- Create: `docs-astro/astro.config.mjs`
- Create: `docs-astro/tsconfig.json`

- [ ] **Step 1: Write minimal `astro.config.mjs`**

Write `docs-astro/astro.config.mjs`:

```js
import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'
import mdx from '@astrojs/mdx'

export default defineConfig({
  site: 'https://jackin.tailrocks.com',
  integrations: [
    react(),
    mdx(),
    starlight({
      title: "jackin'",
      description: 'CLI for orchestrating AI coding agents in isolated containers',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' }],
      sidebar: [
        { label: 'Placeholder', items: [{ label: 'Placeholder', slug: 'placeholder' }] },
      ],
    }),
  ],
})
```

This is the *minimal* config; the full sidebar config lands in Task 15 after MDX files are in place.

- [ ] **Step 2: Write `tsconfig.json`**

Write `docs-astro/tsconfig.json`:

```json
{
  "extends": "astro/tsconfigs/strict",
  "include": [".astro/types.d.ts", "**/*"],
  "exclude": ["dist"],
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "react"
  }
}
```

- [ ] **Step 3: Create the placeholder page**

Write `docs-astro/src/content/docs/placeholder.mdx`:

```mdx
---
title: Placeholder
---

Scaffolding check — this page is deleted in Task 17.
```

Write `docs-astro/src/content/config.ts`:

```ts
import { defineCollection } from 'astro:content'
import { docsSchema } from '@astrojs/starlight/schema'

export const collections = {
  docs: defineCollection({ schema: docsSchema() }),
}
```

- [ ] **Step 4: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/astro.config.mjs docs-astro/tsconfig.json \
        docs-astro/src/content/config.ts docs-astro/src/content/docs/placeholder.mdx
git commit -m "$(cat <<'EOF'
chore(docs-astro): minimal astro + starlight config

Placeholder page verifies the build. Full sidebar config lands after
MDX codemod in Phase 2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Verify build and commit CNAME

**Files:**
- Create: `docs-astro/public/CNAME`

- [ ] **Step 1: Run the build**

Run: `cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro && bun run build`

Expected: completes with zero warnings, emits `dist/` containing `index.html` (redirects to first docs page), `placeholder/index.html`, `pagefind/` directory.

If build fails: read the error carefully. Most common causes: wrong Starlight peer-dep version, missing `astro/tsconfigs/strict` (check `node_modules/astro/tsconfigs/`).

- [ ] **Step 2: Run the preview**

Run: `bun run preview` (in same directory).

Open `http://localhost:4321/placeholder` in a browser. Verify: Starlight chrome renders (default theme is fine at this stage), sidebar shows "Placeholder", top nav has GitHub icon, the placeholder text is visible.

Kill the preview server when done.

- [ ] **Step 3: Copy CNAME from current site**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
cp docs/public/CNAME docs-astro/public/CNAME
cat docs-astro/public/CNAME
```

Expected output: `jackin.tailrocks.com`

- [ ] **Step 4: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/public/CNAME
git commit -m "$(cat <<'EOF'
chore(docs-astro): copy CNAME for jackin.tailrocks.com deployment

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Phase 0 ship criteria:** `bun run build` succeeds, `bun run preview` renders the placeholder page with Starlight chrome. ✓

---

## Phase 1 — Theme Port

### Task 4: Copy `tempo-tokens.css` unchanged

**Files:**
- Create: `docs-astro/src/styles/tempo-tokens.css`

- [ ] **Step 1: Copy the file**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
mkdir -p docs-astro/src/styles
cp docs/tempo-tokens.css docs-astro/src/styles/tempo-tokens.css
```

- [ ] **Step 2: Verify it was copied byte-for-byte**

```bash
diff docs/tempo-tokens.css docs-astro/src/styles/tempo-tokens.css
```

Expected output: empty (no diff).

- [ ] **Step 3: Commit**

```bash
git add docs-astro/src/styles/tempo-tokens.css
git commit -m "$(cat <<'EOF'
chore(docs-astro): copy tempo-tokens.css unchanged

Radix color ramps and semantic tokens — framework-agnostic, no changes
needed for Astro.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Write `global.css` with Tailwind v4 imports

**Files:**
- Create: `docs-astro/src/styles/global.css`

- [ ] **Step 1: Write the file**

Write `docs-astro/src/styles/global.css`:

```css
/* Tailwind v4 + Starlight theme tokens. Order matters: Tailwind must
   come before Starlight Tailwind plugin reset so Starlight wins for
   docs chrome, while utilities still apply inside MDX content. */

@import 'tailwindcss';

/* Forward-compatible @theme block for landing-specific tokens.
   These become utility classes like bg-landing-panel, text-landing-accent, etc.
   Source of truth: docs-astro/src/components/landing/styles.css — these
   utilities will be gradually adopted during Phase 4 per-component ports.
   During Phase 3 (islands-only), styles.css is still the sole source;
   this block is just laying the groundwork. */

@theme {
  --color-landing-bg: #0a0b0a;
  --color-landing-bg-deep: #050605;
  --color-landing-panel: #0f1110;
  --color-landing-text: #f4f7f5;
  --color-landing-text-dim: #9ca8a1;
  --color-landing-text-ghost: #5e6a64;
  --color-landing-accent: #00ff41;
  --color-landing-danger: #ff5e7a;
}
```

- [ ] **Step 2: Commit**

```bash
git add docs-astro/src/styles/global.css
git commit -m "$(cat <<'EOF'
chore(docs-astro): add global.css with Tailwind v4 + landing theme tokens

Landing @theme tokens mirror the CSS variables in components/landing/styles.css.
Utility classes are available now but not yet applied — Phase 4 per-component
ports will migrate markup from the old CSS to utilities.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Rewrite `docs-theme.css` for Starlight CSS vars

**Files:**
- Create: `docs-astro/src/styles/docs-theme.css`
- Read (reference): `docs/docs-theme.css`

This task maps Radix tokens from `tempo-tokens.css` to **Starlight's** CSS var surface instead of Vocs's. Starlight's vars are documented at https://starlight.astro.build/guides/css-and-tailwind/#theming.

- [ ] **Step 1: Read the current `docs/docs-theme.css` to understand what's being mapped**

Run: `cat /Users/donbeave/Projects/jackin-project/jackin/docs/docs-theme.css`

Expected: ~164 lines mapping Vocs var names (e.g., `--vocs-color_background`, `--vocs-color_text`) to Radix tokens from `tempo-tokens.css`. Note which Radix tokens each Vocs var uses.

- [ ] **Step 2: Write the Starlight equivalent**

Write `docs-astro/src/styles/docs-theme.css`:

```css
/* Maps Starlight's CSS custom properties to Radix tokens from
   tempo-tokens.css. This file is the Starlight-equivalent of the
   Vocs-targeted docs-theme.css in the old docs/ directory. */

:root {
  /* Surfaces */
  --sl-color-bg: var(--color-gray1);
  --sl-color-bg-nav: var(--color-gray1);
  --sl-color-bg-sidebar: var(--color-gray1);
  --sl-color-bg-inline-code: var(--color-grayA3);

  /* Text */
  --sl-color-white: var(--color-gray12);
  --sl-color-gray-1: var(--color-gray12);
  --sl-color-gray-2: var(--color-gray11);
  --sl-color-gray-3: var(--color-gray10);
  --sl-color-gray-4: var(--color-gray9);
  --sl-color-gray-5: var(--color-gray7);
  --sl-color-gray-6: var(--color-gray4);
  --sl-color-gray-7: var(--color-gray3);
  --sl-color-black: var(--color-gray1);
  --sl-color-text: var(--color-gray12);
  --sl-color-text-accent: var(--color-green11);
  --sl-color-text-invert: var(--color-gray1);

  /* Lines / borders */
  --sl-color-hairline: var(--color-grayA5);
  --sl-color-hairline-light: var(--color-grayA4);
  --sl-color-hairline-shade: var(--color-grayA6);

  /* Accent (Matrix green to match current landing) */
  --sl-color-accent-low: var(--color-green3);
  --sl-color-accent: var(--color-green9);
  --sl-color-accent-high: var(--color-green11);

  /* Fonts — injected via overrides/Head.astro in Task 18 */
  --sl-font: 'Inter', system-ui, sans-serif;
  --sl-font-mono: 'JetBrains Mono', 'SF Mono', monospace;

  /* Layout dimensions (match Vocs proportions) */
  --sl-sidebar-width: 19rem;
  --sl-content-width: 48rem;
  --sl-nav-height: 3.5rem;
}

/* Starlight toggles a data-theme attribute on <html>; Radix tokens in
   tempo-tokens.css already handle light/dark via light-dark() and the
   color-scheme property. Ensure color-scheme is set on both modes. */

:root[data-theme='dark'] {
  color-scheme: dark;
}

:root[data-theme='light'] {
  color-scheme: light;
}
```

**Note on green tokens:** If `--color-green3`/`green9`/`green11` are not defined in `tempo-tokens.css`, add them in Task 4 follow-up. Quick check: `grep -n "color-green" docs-astro/src/styles/tempo-tokens.css`. If absent, add Radix green ramp to `tempo-tokens.css` and commit as a separate follow-up.

- [ ] **Step 3: Wire both CSS files into Starlight config**

Edit `docs-astro/astro.config.mjs`. Find the `starlight({ ... })` block. Add a `customCss` array after the `sidebar` config:

```js
starlight({
  title: "jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated containers',
  social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' }],
  sidebar: [
    { label: 'Placeholder', items: [{ label: 'Placeholder', slug: 'placeholder' }] },
  ],
  customCss: [
    './src/styles/global.css',
    './src/styles/tempo-tokens.css',
    './src/styles/docs-theme.css',
  ],
}),
```

- [ ] **Step 4: Build and verify**

Run: `cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro && bun run build`

Expected: succeeds with zero warnings.

- [ ] **Step 5: Preview and visually verify**

Run: `bun run preview`, open `http://localhost:4321/placeholder`.

Checks:
- Background color matches current Vocs site (open `https://jackin.tailrocks.com/getting-started/why` in a second tab)
- Text color matches
- Accent color (link color, sidebar active indicator) is Matrix green
- Hairline borders render

If colors look wrong: read the Starlight generated HTML (view source), identify which `--sl-*` vars are unset or overridden. Adjust mapping in `docs-theme.css`.

Kill preview when done.

- [ ] **Step 6: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/src/styles/docs-theme.css docs-astro/astro.config.mjs
git commit -m "$(cat <<'EOF'
feat(docs-astro): port Radix theme tokens to Starlight CSS vars

Maps Starlight's --sl-color-* surface to the Radix tokens from
tempo-tokens.css. Preserves the Matrix-green accent used throughout
the current Vocs site.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Phase 1 ship criteria:** Starlight chrome on the placeholder page is visually indistinguishable from current Vocs site chrome. ✓

---

## Phase 2 — MDX Codemod

### Task 7: Write failing tests for the MDX codemod

**Files:**
- Create: `docs-astro/scripts/migrate-mdx.test.ts`

- [ ] **Step 1: Write the tests**

Write `docs-astro/scripts/migrate-mdx.test.ts`:

```ts
import { describe, test, expect } from 'bun:test'
import { transformMdx } from './migrate-mdx'

describe('transformMdx', () => {
  test('replaces :::note block with Aside component', () => {
    const input = [
      'Some text.',
      '',
      ':::note',
      'A note body.',
      ':::',
      '',
      'More text.',
    ].join('\n')

    const { content, imports } = transformMdx(input)

    expect(content).toContain('<Aside type="note">')
    expect(content).toContain('A note body.')
    expect(content).toContain('</Aside>')
    expect(imports).toContain('Aside')
  })

  test('replaces :::tip block with Aside type="tip"', () => {
    const input = ':::tip\nTip body.\n:::'
    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Aside type="tip">')
    expect(content).toContain('Tip body.')
    expect(imports).toContain('Aside')
  })

  test('replaces :::warning block with Aside type="caution"', () => {
    const input = ':::warning\nWarning body.\n:::'
    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Aside type="caution">')
    expect(imports).toContain('Aside')
  })

  test('replaces ::::steps with Steps wrapper', () => {
    const input = [
      '::::steps',
      '',
      '### Step one',
      'Do the first thing.',
      '',
      '### Step two',
      'Do the second thing.',
      '',
      '::::',
    ].join('\n')

    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Steps>')
    expect(content).toContain('</Steps>')
    expect(content).toContain('### Step one')
    expect(imports).toContain('Steps')
  })

  test('replaces :::code-group with Tabs/TabItem wrapper', () => {
    const input = [
      ':::code-group',
      '',
      '```sh [macOS]',
      'brew install jackin',
      '```',
      '',
      '```sh [Linux]',
      'brew install jackin',
      '```',
      '',
      ':::',
    ].join('\n')

    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Tabs>')
    expect(content).toContain('<TabItem label="macOS">')
    expect(content).toContain('<TabItem label="Linux">')
    expect(content).toContain('</Tabs>')
    expect(imports).toContain('Tabs')
    expect(imports).toContain('TabItem')
  })

  test('returns imports as a sorted unique set', () => {
    const input = ':::note\nA.\n:::\n\n:::tip\nB.\n:::'
    const { imports } = transformMdx(input)
    expect(imports).toEqual(['Aside'])
  })

  test('emits no imports when no directives are used', () => {
    const input = '# Heading\n\nJust plain markdown.'
    const { imports } = transformMdx(input)
    expect(imports).toEqual([])
  })

  test('preserves existing frontmatter', () => {
    const input = '---\ntitle: My Page\n---\n\nBody.'
    const { content } = transformMdx(input)
    expect(content).toMatch(/^---\ntitle: My Page\n---/)
  })

  test('extracts title from first H1 when frontmatter missing', () => {
    const input = '# Extracted Title\n\nBody.'
    const { content } = transformMdx(input)
    expect(content).toMatch(/^---\ntitle: Extracted Title\n---/)
  })

  test('injects import line after frontmatter', () => {
    const input = '---\ntitle: Page\n---\n\n:::note\nA.\n:::'
    const { content } = transformMdx(input)
    expect(content).toContain("import { Aside } from '@astrojs/starlight/components'")
    // Import line must be after closing --- and before body
    const frontmatterEnd = content.indexOf('---', 4) + 3
    const importIdx = content.indexOf('import')
    const asideIdx = content.indexOf('<Aside')
    expect(importIdx).toBeGreaterThan(frontmatterEnd)
    expect(importIdx).toBeLessThan(asideIdx)
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro && bun test scripts/migrate-mdx.test.ts`

Expected: FAIL with "Cannot find module './migrate-mdx'" or similar.

---

### Task 8: Implement the MDX codemod

**Files:**
- Create: `docs-astro/scripts/migrate-mdx.ts`

- [ ] **Step 1: Write the implementation**

Write `docs-astro/scripts/migrate-mdx.ts`:

```ts
// One-shot codemod: transforms Vocs MDX directives to Starlight components.
// Delete this file (and its test) after the migration commits in Task 12.

export interface TransformResult {
  content: string
  imports: string[]
}

export function transformMdx(input: string): TransformResult {
  const usedImports = new Set<string>()
  let body = input

  // Split frontmatter from body so transformations don't touch frontmatter
  let frontmatter = ''
  const fmMatch = body.match(/^---\n([\s\S]*?)\n---\n?/)
  if (fmMatch) {
    frontmatter = fmMatch[0]
    body = body.slice(fmMatch[0].length)
  } else {
    // No frontmatter — try to extract title from first H1
    const h1 = body.match(/^#\s+(.+?)\s*$/m)
    if (h1) {
      frontmatter = `---\ntitle: ${h1[1]}\n---\n\n`
    }
  }

  // Transform ::::steps ... :::: → <Steps>...</Steps>
  body = body.replace(/^::::steps\s*\n([\s\S]*?)^::::\s*$/gm, (_m, inner) => {
    usedImports.add('Steps')
    return `<Steps>\n${inner.trim()}\n</Steps>`
  })

  // Transform :::code-group ... ::: → <Tabs>...</Tabs>
  body = body.replace(/^:::code-group\s*\n([\s\S]*?)^:::\s*$/gm, (_m, inner) => {
    usedImports.add('Tabs')
    usedImports.add('TabItem')
    // Inside, find ```lang [Label]\n...\n``` blocks and wrap each in TabItem
    const tabItems = inner.replace(
      /```(\w+)\s+\[([^\]]+)\]\n([\s\S]*?)```/g,
      (_mm: string, _lang: string, label: string, code: string) => {
        return `<TabItem label="${label}">\n\n\`\`\`${_lang}\n${code}\`\`\`\n\n</TabItem>`
      }
    )
    return `<Tabs>\n${tabItems.trim()}\n</Tabs>`
  })

  // Transform :::note / :::tip / :::warning blocks → <Aside>
  body = body.replace(
    /^:::(note|tip|warning)\s*\n([\s\S]*?)^:::\s*$/gm,
    (_m, kind, inner) => {
      usedImports.add('Aside')
      const type = kind === 'warning' ? 'caution' : kind
      return `<Aside type="${type}">\n${inner.trim()}\n</Aside>`
    }
  )

  // Assemble output
  const imports = Array.from(usedImports).sort()
  const importLine =
    imports.length > 0
      ? `import { ${imports.join(', ')} } from '@astrojs/starlight/components'\n\n`
      : ''

  return {
    content: frontmatter + importLine + body,
    imports,
  }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro && bun test scripts/migrate-mdx.test.ts`

Expected: all 10 tests PASS.

If any fails: read the test message, adjust the regex or output assembly. Common traps:
- `^` in regex requires the `m` flag (already included)
- `[\s\S]*?` for "non-greedy match including newlines"
- Closing `:::` inside `:::code-group` won't accidentally match the outer `:::` because it's on its own line

- [ ] **Step 3: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/scripts/migrate-mdx.ts docs-astro/scripts/migrate-mdx.test.ts
git commit -m "$(cat <<'EOF'
chore(docs-astro): add MDX codemod for Vocs → Starlight directives

One-shot script. Tests cover :::note/:::tip/:::warning → Aside,
::::steps → Steps, :::code-group → Tabs/TabItem, frontmatter
preservation, and H1-title extraction.

Deleted in Task 13 after the 25-page migration lands.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: Add the CLI runner for the codemod

**Files:**
- Modify: `docs-astro/scripts/migrate-mdx.ts`

- [ ] **Step 1: Append the CLI runner to `migrate-mdx.ts`**

Append this block to `docs-astro/scripts/migrate-mdx.ts` (after the `transformMdx` function):

```ts
import { readdirSync, readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'

function walkMdx(root: string): string[] {
  const out: string[] = []
  for (const name of readdirSync(root)) {
    const full = join(root, name)
    const st = statSync(full)
    if (st.isDirectory()) out.push(...walkMdx(full))
    else if (name.endsWith('.mdx') || name.endsWith('.md')) out.push(full)
  }
  return out
}

function main() {
  const args = process.argv.slice(2)
  const dry = args.includes('--dry')
  const srcRoot = '../docs/pages'
  const dstRoot = './src/content/docs'

  const files = walkMdx(srcRoot)
    .filter((f) => !f.endsWith('/index.mdx')) // landing skipped — handled in Phase 3
    .sort()

  let changed = 0
  for (const src of files) {
    const rel = relative(srcRoot, src)
    const dst = join(dstRoot, rel)
    const input = readFileSync(src, 'utf8')
    const { content } = transformMdx(input)

    if (dry) {
      console.log(`\n=== ${rel} ===`)
      if (input === content) console.log('(unchanged)')
      else console.log(content.slice(0, 400) + (content.length > 400 ? '\n...' : ''))
      continue
    }

    mkdirSync(dirname(dst), { recursive: true })
    writeFileSync(dst, content)
    changed++
    console.log(`wrote ${dst}`)
  }

  if (!dry) console.log(`\n${changed} files written to ${dstRoot}`)
}

if (import.meta.main) main()
```

- [ ] **Step 2: Run the codemod in dry-run mode**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro
bun run scripts/migrate-mdx.ts --dry 2>&1 | head -200
```

Expected: 25 files listed, each with a preview of transformed content. Scan for any output that looks malformed (e.g., unclosed Aside, stray `:::`).

- [ ] **Step 3: Commit the runner**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/scripts/migrate-mdx.ts
git commit -m "$(cat <<'EOF'
chore(docs-astro): add CLI runner for MDX codemod

Reads docs/pages/**/*.{mdx,md}, writes transformed output to
docs-astro/src/content/docs/. --dry flag previews without writing.
Skips index.mdx (landing is handled in Phase 3).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: Run the codemod and review output

**Files:**
- Create: `docs-astro/src/content/docs/**/*.mdx` (25 files)
- Delete: `docs-astro/src/content/docs/placeholder.mdx`

- [ ] **Step 1: Delete the placeholder**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro
rm src/content/docs/placeholder.mdx
```

- [ ] **Step 2: Run the codemod for real**

```bash
bun run scripts/migrate-mdx.ts
```

Expected: `25 files written to ./src/content/docs`.

- [ ] **Step 3: Open every transformed file and read it**

For each file in `docs-astro/src/content/docs/`, open and verify:
- Frontmatter is valid YAML (starts with `---`, ends with `---`)
- Import line is correct (only imports used; `{ Aside }`, `{ Aside, Steps }`, etc.)
- No stray `:::` remains anywhere
- `<Aside type="...">` ... `</Aside>` blocks have matching tags
- `<Tabs>` / `<TabItem label="...">` / `</TabItem>` / `</Tabs>` are balanced
- `<Steps>` blocks contain `<ol>` or list markup

**List of files to review (25 total):**

```
getting-started/why.mdx              guides/workspaces.mdx
getting-started/installation.mdx     guides/mounts.mdx
getting-started/quickstart.mdx       guides/authentication.mdx
getting-started/concepts.mdx         guides/agent-repos.mdx
                                     guides/security-model.mdx
developing/creating-agents.mdx       guides/comparison.mdx
developing/construct-image.mdx
developing/agent-manifest.mdx        commands/load.mdx
                                     commands/launch.mdx
reference/configuration.mdx          commands/hardline.mdx
reference/architecture.mdx           commands/eject.mdx
reference/roadmap.mdx                commands/exile.mdx
                                     commands/purge.mdx
                                     commands/workspace.mdx
                                     commands/config.mdx
```

- [ ] **Step 4: Fix any regex misses by hand**

Common regex misses to watch for:
- A `:::` that ends inside a multi-line directive (rare)
- A `:::code-group` where a tab has no `[Label]` — the codemod will leave the `` ``` `` block untransformed, which is actually fine (it renders as a plain code block, just without the tab container)

If you find issues, edit the affected file directly. Do not re-run the codemod — it will overwrite your fixes.

- [ ] **Step 5: Run build to verify MDX parses correctly**

Run: `cd docs-astro && bun run build`

Expected: succeeds. If it fails, the error will name the offending MDX file and line. Fix by hand.

- [ ] **Step 6: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/src/content/docs/
git commit -m "$(cat <<'EOF'
feat(docs-astro): migrate 25 docs pages via MDX codemod

Ran scripts/migrate-mdx.ts against docs/pages/. Directives replaced:
- :::note|tip|warning → <Aside type="...">
- ::::steps → <Steps>
- :::code-group → <Tabs><TabItem>

Human-reviewed each file for regex misses.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Delete the codemod script

**Files:**
- Delete: `docs-astro/scripts/migrate-mdx.ts`
- Delete: `docs-astro/scripts/migrate-mdx.test.ts`

- [ ] **Step 1: Delete the files**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro
rm scripts/migrate-mdx.ts scripts/migrate-mdx.test.ts
rmdir scripts 2>/dev/null || true
```

- [ ] **Step 2: Verify no references remain**

Run: `grep -r "migrate-mdx" /Users/donbeave/Projects/jackin-project/jackin/docs-astro/ 2>/dev/null`

Expected: no output.

- [ ] **Step 3: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add -u docs-astro/
git commit -m "$(cat <<'EOF'
chore(docs-astro): delete one-shot MDX codemod

Script served its purpose during the 25-page migration. Not kept
as a permanent tool — no reuse expected.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: Wire full sidebar and top-nav config in `astro.config.mjs`

**Files:**
- Modify: `docs-astro/astro.config.mjs`

- [ ] **Step 1: Replace the sidebar config**

Edit `docs-astro/astro.config.mjs`. Replace the `starlight({ ... })` block with the full config:

```js
starlight({
  title: "jackin'",
  description: 'CLI for orchestrating AI coding agents in isolated containers',
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
  customCss: [
    './src/styles/global.css',
    './src/styles/tempo-tokens.css',
    './src/styles/docs-theme.css',
  ],
}),
```

- [ ] **Step 2: Build**

Run: `cd docs-astro && bun run build`

Expected: succeeds, emits 25+ HTML files in `dist/`.

- [ ] **Step 3: Preview and click-test every sidebar link**

Run: `bun run preview`, open `http://localhost:4321/getting-started/why`.

Click every link in the sidebar. Verify:
- Every page loads with no 404
- The current page is highlighted in the sidebar
- The "Edit on GitHub" link at page bottom points to the correct file path
- Pagefind search works (click search, type "jackin", results appear)

Kill preview when done.

- [ ] **Step 4: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/astro.config.mjs
git commit -m "$(cat <<'EOF'
feat(docs-astro): wire full sidebar config mirroring Vocs topNav + sidebar

All 25 docs pages navigable; edit-on-GitHub links point at the new
docs-astro/src/content/docs/ path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Phase 2 ship criteria:** All 25 docs pages render in Starlight, sidebar navigation works, Pagefind search works, edit-on-GitHub links resolve. ✓

---

## Phase 3 — Landing Page (Pass A — Islands-Only)

### Task 13: Copy all landing components and supporting modules

**Files:**
- Create: `docs-astro/src/components/landing/*` (21 files, byte-for-byte from `docs/components/landing/`)

- [ ] **Step 1: Copy the directory wholesale**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
mkdir -p docs-astro/src/components/landing
cp -r docs/components/landing/. docs-astro/src/components/landing/
```

- [ ] **Step 2: Verify the file count matches**

Run: `ls docs-astro/src/components/landing/ | wc -l`

Expected: 21 (15 `.tsx` components + `loopData.tsx` + `machineData.ts` + `vocabularyData.ts` + `rainEngine.ts` + `rainEngine.test.ts` + `styles.css`).

- [ ] **Step 3: Verify byte-for-byte match**

```bash
diff -r docs/components/landing/ docs-astro/src/components/landing/
```

Expected: empty (no diff).

- [ ] **Step 4: Commit**

```bash
git add docs-astro/src/components/landing/
git commit -m "$(cat <<'EOF'
feat(docs-astro): copy landing components byte-for-byte from Vocs site

Pass A of landing migration: all 15 React components, 4 support
modules, 1 test file, 1 stylesheet copied unchanged. Phase 4 will
incrementally port 10 of 15 to .astro; this checkpoint ensures zero
visual drift first.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: Run the rainEngine test in the new location

**Files:**
- Run: `docs-astro/src/components/landing/rainEngine.test.ts`

- [ ] **Step 1: Run the test**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro
bun test src/components/landing/rainEngine.test.ts
```

Expected: all tests PASS.

- [ ] **Step 2: If tests fail, check import paths**

The test file imports from `./rainEngine` (relative). If it fails with "module not found", the copy step missed something. Run the diff check from Task 13 again.

No commit for this task — verification only.

---

### Task 15: Create `Head.astro` override for font injection

**Files:**
- Create: `docs-astro/src/components/overrides/Head.astro`

- [ ] **Step 1: Write the Head override**

Write `docs-astro/src/components/overrides/Head.astro`:

```astro
---
import Default from '@astrojs/starlight/components/Head.astro'
---

<Default><slot /></Default>

<link rel="preconnect" href="https://fonts.googleapis.com" />
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
<link
  rel="stylesheet"
  href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Inter:wght@400;500;600;700;800;900&family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,700&display=swap"
/>
```

- [ ] **Step 2: Register the override in `astro.config.mjs`**

Edit the `starlight({ ... })` block — add a `components` key:

```js
starlight({
  title: "jackin'",
  // ... existing config ...
  components: {
    Head: './src/components/overrides/Head.astro',
  },
  customCss: [ /* ... */ ],
}),
```

- [ ] **Step 3: Build and verify fonts load**

Run: `cd docs-astro && bun run build && bun run preview`

Open DevTools Network tab on any docs page, filter for "font". Expected: Inter, JetBrains Mono, and Fraunces load from Google Fonts.

Kill preview.

- [ ] **Step 4: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/src/components/overrides/Head.astro docs-astro/astro.config.mjs
git commit -m "$(cat <<'EOF'
feat(docs-astro): inject Google Fonts via Starlight Head override

Replaces Vocs layout.tsx's runtime ensureFontsLink() with a native
<link> tag. Fonts: Inter, JetBrains Mono, Fraunces — same set as
current Vocs site.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 16: Create the landing page (Pass A — islands-only)

**Files:**
- Create: `docs-astro/src/pages/index.astro`

- [ ] **Step 1: Write the landing page**

Write `docs-astro/src/pages/index.astro`:

```astro
---
import '../components/landing/styles.css'
import { Landing } from '../components/landing/Landing'
---
<!DOCTYPE html>
<html lang="en" class="dark" data-theme="dark" style="color-scheme: dark;">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>jackin' — CLI for orchestrating AI coding agents in isolated containers</title>
    <meta name="description" content="You're the Operator. They're already inside. jackin' drops AI coding agents into isolated Docker containers — full autonomy inside, your host untouched outside." />

    <!-- Open Graph -->
    <meta property="og:title" content="jackin'" />
    <meta property="og:description" content="CLI for orchestrating AI coding agents in isolated containers" />
    <meta property="og:url" content="https://jackin.tailrocks.com" />
    <meta property="og:type" content="website" />

    <!-- Fonts (same set as docs Head override) -->
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link
      rel="stylesheet"
      href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Inter:wght@400;500;600;700;800;900&family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,700&display=swap"
    />
  </head>
  <body>
    <Landing client:load />
  </body>
</html>
```

**Note on `client:load`:** This hydrates the entire Landing tree as a single React island, matching current Vocs behavior. Phase 4 will split this into per-component hydration directives once the wrapper ports to `.astro`.

- [ ] **Step 2: Build**

Run: `cd docs-astro && bun run build`

Expected: succeeds. Verify `dist/index.html` exists.

**Known issue — Pagefind on landing:** The landing is a plain Astro page outside Starlight's content collection. Pagefind only indexes Starlight docs content by default, so the landing won't appear in search results. This matches current Vocs behavior (the landing isn't in Vocs's search index either). No action needed.

- [ ] **Step 3: Preview and visually compare against production**

Run: `bun run preview`, open `http://localhost:4321/`.

In a second browser tab, open `https://jackin.tailrocks.com/`.

Place the two side-by-side. Check each section:
1. **Hero** — digital rain animation visible, "You're the Operator" headline, CodePanel with typewriter animation cycling through `$ load` / `$ hardline` / `$ eject`
2. **Vocabulary** (section 02) — scroll-driven rail updates the detail card
3. **Pills** (section 03) — blue/red pill cards render
4. **Approach** (section 04) — TabbedBuilder switches between `jackin.agent.toml` and `Dockerfile`
5. **Cast** (section 05) — three agent cards + invite card
6. **Composition Machine** (section 06) — clicking orgs/classes/workspaces updates the preview
7. **Daily Loop** (section 07) — five frames render
8. **Install** (section 08) — brew commands visible
9. **Footer** — wordmark displays

If anything differs: check browser console for errors, verify `styles.css` loaded (Network tab), check that all components copied in Task 13.

Kill preview when done.

- [ ] **Step 4: Take reference screenshots**

Before committing, capture screenshots of every section on the current production site (`https://jackin.tailrocks.com/`) and save them somewhere convenient (not in git). These are the reference for Phase 4 visual-regression review.

Optional but recommended: store in a shared location (a Notion page, a GitHub PR description — wherever the team can reference them during Phase 4 reviews).

- [ ] **Step 5: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/src/pages/index.astro
git commit -m "$(cat <<'EOF'
feat(docs-astro): mount landing as single React island (Pass A)

Phase 3 checkpoint: landing renders from docs-astro/ identically to
the current Vocs site. All 15 React components hydrate together as
one island (matches current Vocs behavior). Phase 4 will split into
per-component islands + static .astro.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Phase 3 ship criteria:** Landing page at `http://localhost:4321/` is visually indistinguishable from current production. All interactive elements (digital rain, typewriter, vocab scroll, approach tabs, composition machine) work. ✓

---

## Task 17: Add AGENTS.md and CLAUDE.md for docs-astro

**Files:**
- Create: `docs-astro/AGENTS.md`
- Create: `docs-astro/CLAUDE.md`

- [ ] **Step 1: Write `docs-astro/AGENTS.md`**

```markdown
# Docs AGENTS.md

## Stack

- This directory is an Astro + Starlight documentation site.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro` with `@astrojs/starlight`, `@astrojs/react`, `@astrojs/mdx`.
- Styling: `tailwindcss` v4 (CSS-first configuration in `src/styles/global.css`).

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- Do not use `npm`, `pnpm`, or `yarn` in this directory.
- Keep `bun.lock` as the only lockfile in `docs-astro/`.

## Common Commands

- Install dependencies: `bun install --frozen-lockfile`
- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Run tests: `bun test`

## Content Notes

- Docs content lives under `src/content/docs/` as MDX files.
- File-based routing: `src/content/docs/foo/bar.mdx` → `/foo/bar`.
- Sidebar and top-nav are configured in `astro.config.mjs`.
- Use Starlight components for callouts (`<Aside type="note|tip|caution">`),
  steps (`<Steps>`), and tabs (`<Tabs><TabItem>`). Import from
  `@astrojs/starlight/components`.
- Keep docs and code behavior aligned; when they differ, code is the source of truth.

## Landing Page

- Landing is at `src/pages/index.astro` — a plain Astro page, NOT inside
  the Starlight content collection. It has full control over its layout.
- React components in `src/components/landing/` are mounted as islands.
  Phase 4 of the Astro migration (tracked in superpowers/plans/) is
  incrementally porting 10 of 15 landing components to native `.astro`.

## Migration Status

This directory is the replacement for the Vocs-based `docs/` directory.
Until Phase 6 cutover, both directories coexist. After cutover, `docs/`
will be renamed to `docs-vocs-legacy/` and this directory will be
renamed to `docs/`. See:
- `docs/superpowers/specs/2026-04-20-astro-starlight-migration-design.md`
- `docs/superpowers/plans/2026-04-20-astro-starlight-migration-phase-0-3.md`
```

- [ ] **Step 2: Write `docs-astro/CLAUDE.md`**

Write `docs-astro/CLAUDE.md`:

```markdown
@AGENTS.md
```

- [ ] **Step 3: Commit**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git add docs-astro/AGENTS.md docs-astro/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs(docs-astro): add AGENTS.md and CLAUDE.md for the new stack

Documents Astro + Starlight conventions, package management, common
commands, and landing page architecture. Points at the migration spec
and plan for full context.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Final Verification

### Task 18: End-to-end smoke test

- [ ] **Step 1: Full clean build**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs-astro
rm -rf dist .astro
bun run build
```

Expected: succeeds with zero warnings. Build time should be ~10-30 seconds.

- [ ] **Step 2: Verify `dist/` contents**

```bash
find dist -name "*.html" | wc -l
ls dist/pagefind 2>/dev/null | head
```

Expected:
- At least 26 HTML files (25 docs pages + landing)
- `dist/pagefind/` exists with `pagefind.js` and index chunks

- [ ] **Step 3: Preview and walk the full site**

```bash
bun run preview
```

Open `http://localhost:4321/` and walk:
1. Landing page — all 9 sections visible, interactions work
2. Click "Get Started" CTA → lands on `/getting-started/why`
3. Click every sidebar section — every page renders
4. Open search (top nav) — Pagefind search works on docs pages
5. Toggle light/dark mode (top nav) — both work, no FOUC on reload in dark mode
6. Verify dark mode is the default on first visit (clear localStorage, reload)

Kill preview when done.

- [ ] **Step 4: Confirm the current Vocs site still works**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun run build
```

Expected: succeeds — confirms nothing in the Vocs directory was accidentally changed. Ignore any warnings that existed before this plan.

- [ ] **Step 5: Final commit (if any trailing changes)**

If any files were modified during verification, commit them:

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
git status
# If changes exist:
git add -u
git commit -m "chore(docs-astro): verification fixes from end-to-end smoke test"
```

If no changes: skip.

**Plan 1 complete.** At this point:
- `docs-astro/` is a working Astro + Starlight site matching current Vocs visually
- Ready for Phase 4 (per-component ports) — planned in the follow-up plan
- Current `docs/` Vocs site still builds and remains untouched

---

## Self-Review Results

**Spec coverage:**

- Phase 0 (Scaffold) — Tasks 1-3 ✓
- Phase 1 (Theme port) — Tasks 4-6 ✓
- Phase 2 (MDX codemod) — Tasks 7-12 ✓
- Phase 3 (Landing Pass A) — Tasks 13-16 ✓
- AGENTS.md/CLAUDE.md convention — Task 17 ✓
- E2E smoke test — Task 18 ✓

Out-of-scope (deferred to Plan 2 and Plan 3):
- Phase 4 per-component Astro ports
- Phase 5 sliding sidebar indicator, layout polish
- Phase 6 cutover (`docs/` → `docs-vocs-legacy/`, `docs-astro/` → `docs/`)

**Placeholder scan:** No TBD/TODO placeholders in the plan. Tasks that depend on observation (e.g., "If `--color-green3` is absent") provide a specific command and a specific follow-up.

**Type consistency:** Function names used across tasks: `transformMdx` (defined Task 7, implemented Task 8, used Task 9). Return shape `{ content: string, imports: string[] }` is consistent across all mentions.

**Known fragile assumptions:**

1. **Starlight version availability.** Plan assumes `@astrojs/starlight@^0.30.0`. If that specific version isn't published when executing, Task 1 Step 2 notes the fallback: check `bun pm view` and use the latest published minor.
2. **`color-green*` Radix tokens.** Task 6 assumes they exist in `tempo-tokens.css`. Task 6 Step 2 notes the fallback if they don't: add them or swap to an existing ramp.
3. **Starlight's component override slot behavior.** `Head.astro` uses `<Default><slot /></Default>`. If Starlight's `Head` doesn't accept slotted children in the current version, Task 15 Step 3's preview check will catch it and the override needs adjustment (likely to `<slot />\n<Default />` or injecting via Astro's document `<head>` directly).
