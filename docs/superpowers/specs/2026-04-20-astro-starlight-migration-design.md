# Docs Migration: Vocs to Astro Starlight

## Summary

Migrate the jackin' documentation site from [Vocs](https://vocs.dev/) (React + Vite) to [Astro Starlight](https://starlight.astro.build/) (Astro + islands). Goal: unify the stack on Astro, minimize React to components that genuinely need it, and preserve the current landing page's exact look and feel.

This spec reverses the prior migration captured in `2026-04-10-vocs-migration-design.md` — Astro Starlight → Vocs → Astro Starlight. The return is driven by stack-unification goals and the desire to push as much of the landing to pure Astro + Tailwind as possible.

## Decisions

- **Sibling directory** (`docs-astro/` alongside existing `docs/`) during migration; rename to `docs/` at cutover
- **Keep React only where it earns its place** — 10 of 15 landing components become static `.astro`; 4-5 stay as React islands
- **Search** — Starlight's built-in Pagefind (drop the current Vocs "Ask AI" CTA; no replacement)
- **Landing page routing** — `src/pages/index.astro` (plain Astro page outside Starlight's content collection) for full layout control
- **Tailwind v4** via `@astrojs/starlight-tailwind` (CSS-first configuration preserved)
- **Package manager** — Bun (unchanged)
- **Deployment** — `jackin.tailrocks.com` (CNAME preserved), GitHub Pages-compatible static output
- **Dark mode** — Starlight's built-in `defaultMode: 'dark'` replaces the current `default-dark.js` inline script
- **Codemod-driven MDX port** — one-shot migration script, deleted after use

## Project Structure

```
jackin/
├── docs/                              # existing Vocs site (untouched until cutover)
└── docs-astro/                        # new Astro + Starlight site
    ├── astro.config.mjs               # replaces vocs.config.ts
    ├── package.json                   # astro, @astrojs/starlight, @astrojs/react,
    │                                  # @astrojs/mdx, @astrojs/starlight-tailwind,
    │                                  # tailwindcss, react, react-dom
    ├── bun.lock
    ├── tsconfig.json
    ├── .gitignore                     # node_modules, dist, .astro
    ├── AGENTS.md                      # Updated for Astro + Starlight + Tailwind
    ├── CLAUDE.md                      # Pointer to AGENTS.md
    ├── public/
    │   └── CNAME                      # jackin.tailrocks.com
    ├── scripts/
    │   └── migrate-mdx.ts             # one-shot codemod (deleted after migration)
    └── src/
        ├── content/
        │   ├── config.ts              # Starlight content collection schema
        │   └── docs/                  # 25 MDX pages (codemodded from docs/pages/)
        │       ├── getting-started/   # why, installation, quickstart, concepts
        │       ├── guides/            # workspaces, mounts, authentication,
        │       │                      # agent-repos, security-model, comparison
        │       ├── commands/          # load, launch, hardline, eject, exile,
        │       │                      # purge, workspace, config
        │       ├── developing/        # creating-agents, construct-image, agent-manifest
        │       └── reference/         # configuration, architecture, roadmap
        ├── components/
        │   ├── landing/               # 15 components (React islands + .astro)
        │   └── overrides/
        │       └── Head.astro         # font injection + sidebar indicator script
        ├── pages/
        │   └── index.astro            # landing — plain Astro page, NOT in collection
        └── styles/
            ├── tempo-tokens.css       # copied unchanged from docs/
            ├── docs-theme.css         # rewritten for Starlight CSS vars
            ├── landing.css            # current styles.css (shrinks during Phase 4)
            └── global.css             # Tailwind v4 @theme + imports
```

## Component Inventory

### Landing page components (15 TSX components + 1 wrapper + support modules)

| # | Component | LOC | Target | Reasoning |
|---|-----------|-----|--------|-----------|
| 1 | `Landing` (wrapper) | 26 | `.astro` | Pure composition; just renders children in order |
| 2 | `HeroStage` | 22 | `.astro` | Layout wrapper only |
| 3 | `HeroContent` | 24 | `.astro` | Static copy + CTA |
| 4 | `DigitalRain` | 77 | React island (`client:idle`) | Canvas animation loop, ResizeObserver, reduced-motion handling |
| 5 | `CodePanel` | 120 | React island (`client:idle`) | Typewriter animation, async loop with cancellation tokens |
| 6 | `VocabularyDictionary` | 95 | React island (`client:visible`) | Scroll-driven active-index, rAF throttling |
| 7 | `PillCards` | 49 | `.astro` | Pure render |
| 8 | `ApproachCards` | 80 | `.astro` shell | Contains `TabbedBuilder` island |
| 9 | `TabbedBuilder` | 45 | React island (`client:idle`); optional port to Astro later | Simple tab state — candidate for vanilla JS port in PR #9 |
| 10 | `CastRoster` | 45 | `.astro` | Pure render |
| 11 | `CompositionMachine` | 143 | React island (`client:visible`) | 3-axis stateful composer, denied-state logic |
| 12 | `FocusCallout` | 25 | `.astro` | Pure render |
| 13 | `DailyLoop` | 35 | `.astro` | Pure render — **requires loopData refactor** (see note below) |
| 14 | `InstallBlock` | 23 | `.astro` | Pure render |
| 15 | `WordmarkFooter` | 15 | `.astro` | Pure render |

**Support modules (copied unchanged in Phase 3; refactored later as needed):**

| File | LOC | Target | Notes |
|------|-----|--------|-------|
| `rainEngine.ts` | 103 | Unchanged | Framework-agnostic TS; imported by the DigitalRain island |
| `rainEngine.test.ts` | 48 | Unchanged | `bun test` on the engine module |
| `loopData.tsx` | 97 | **Refactor during PR #4** | Currently returns `ReactNode` for `terminal` field (JSX spans for syntax coloring). Refactor to `Array<{text: string, cls: string}>` segments; Astro `DailyLoop` maps over them. |
| `machineData.ts` | 88 | Unchanged | Already plain data (strings) |
| `vocabularyData.ts` | 88 | Unchanged | Already plain data |
| `styles.css` | 930 | Copied in Phase 3; shrinks in Phase 4 | Rules migrate to Tailwind utilities per component |

**Final state**: 5 React islands (4 after optional PR #9), each hydrating independently; 10 static Astro components; ~0 KB JS for the static portion. The 5 islands cover all genuine interactivity: canvas animation, typewriter, scroll-driven state, tab switching, 3-axis composer.

### Data-refactor pattern (applies to `loopData.tsx` and `ApproachCards.tsx`'s inline manifest bodies)

Current Vocs pattern: JSX with className-tagged `<span>` for syntax highlighting passed as `ReactNode` props.

```tsx
// current: loopData.tsx
{ terminal: (<><span className="c"># comment</span>{'\n'}<span className="k">jackin</span>…</>) }
```

Refactored for Astro rendering:

```ts
// refactored: loopData.ts
{ terminal: [
    { cls: 'c', text: '# comment' },
    { cls: null, text: '\n' },
    { cls: 'k', text: 'jackin' },
    …
]}
```

```astro
<!-- DailyLoop.astro renders: -->
<pre class="landing-loop-term-body">{
  frame.terminal.map(seg =>
    seg.cls ? <span class={seg.cls}>{seg.text}</span> : seg.text
  )
}</pre>
```

Same visual output, no React dependency. Apply this pattern to `ApproachCards.tsx`'s inline `manifestBody` / `dockerfileBody` constants during PR #8.

### Hydration directives

| Component | Directive | Reasoning |
|-----------|-----------|-----------|
| `DigitalRain` | `client:idle` | Decorative; can wait until main thread is free |
| `CodePanel` | `client:idle` | Above-the-fold but non-critical animation |
| `VocabularyDictionary` | `client:visible` | Deep in the page; scroll-driven |
| `CompositionMachine` | `client:visible` | Deep in the page; hydrates when scrolled into view |
| `TabbedBuilder` | `client:idle` | Small state; idle is fine |

## Phased Migration Plan

### Phase 0 — Scaffold

- Create `docs-astro/` sibling directory
- `bun create astro` → Starlight template
- Install: `@astrojs/starlight`, `@astrojs/react`, `@astrojs/mdx`, `@astrojs/starlight-tailwind`, `tailwindcss`, `react`, `react-dom`
- Minimal `astro.config.mjs` with one placeholder page
- Verify `bun run build` succeeds, `bun run dev` serves
- Copy `CNAME` to `public/`
- **Ship criteria**: blank Starlight site builds and previews

### Phase 1 — Theme port (docs-only)

- Copy `tempo-tokens.css` unchanged to `src/styles/`
- Rewrite `docs-theme.css` to map **Starlight's CSS vars** (`--sl-color-*`, `--sl-font-*`) to the Radix tokens from `tempo-tokens.css`. This is the most tedious file — budget 1-2 hours.
- Wire both into Starlight config via `customCss`
- Create one throwaway docs page; verify chrome colors, typography, dark mode all match Vocs appearance
- **Ship criteria**: docs chrome is visually indistinguishable from Vocs on a placeholder page

### Phase 2 — MDX codemod (25 pages)

**Directive replacements:**

| Vocs syntax | Starlight replacement |
|-------------|----------------------|
| `:::note\n…\n:::` (2 occurrences) | `<Aside type="note">…</Aside>` |
| `:::tip\n…\n:::` (4 occurrences) | `<Aside type="tip">…</Aside>` |
| `:::warning\n…\n:::` (2 occurrences) | `<Aside type="caution">…</Aside>` |
| `::::steps\n…\n::::` (3 occurrences) | `<Steps><ol><li>…</li></ol></Steps>` |
| `:::code-group\n…\n:::` (2 occurrences) | `<Tabs><TabItem label="…"><Code … /></TabItem></Tabs>` |

**Process:**

1. Write `scripts/migrate-mdx.ts` (one-shot, deleted after use)
2. Script reads `docs/pages/**/*.mdx`, writes transformed output to `docs-astro/src/content/docs/**/*.mdx`
3. Adds import at top of every transformed file: `import { Aside, Steps, Tabs, TabItem, Code } from '@astrojs/starlight/components'` (only imports used)
4. Preserves existing frontmatter (`title:`), extracts from first `# Heading` if absent
5. Dry-run flag prints per-file diffs
6. Human review: open each of 25 files, verify rendering, fix regex misses
7. Delete migration script
8. **Ship criteria**: all 25 pages render correctly in Starlight, URLs unchanged

### Phase 3 — Landing, islands-only (Pass A)

- Copy all 15 React components byte-for-byte to `docs-astro/src/components/landing/`
- Copy `styles.css` byte-for-byte to `src/styles/landing.css`
- Copy `rainEngine.ts` + `rainEngine.test.ts` unchanged
- Create `src/pages/index.astro`:
  ```astro
  ---
  import '../styles/landing.css'
  import { Landing } from '../components/landing/Landing'
  ---
  <html lang="en" class="dark">
    <head>…fonts, meta, OG tags…</head>
    <body><Landing client:load /></body>
  </html>
  ```
- All 15 components hydrate together as a single React tree (matches current Vocs behavior)
- **Ship criteria**: visual checkpoint — side-by-side screenshot review against production `jackin.tailrocks.com`, zero visual drift

### Phase 4 — Incremental Astro-ification (Pass B)

One PR per component, in this order (lowest risk first):

| PR | Component | Change |
|----|-----------|--------|
| #0 | `Landing` wrapper | React → `.astro`; islands directly mounted by `src/pages/index.astro` instead of nested under a React parent |
| #1 | `WordmarkFooter` | React → `.astro`, CSS → Tailwind utilities |
| #2 | `InstallBlock` | React → `.astro`, CSS → Tailwind utilities |
| #3 | `FocusCallout` | React → `.astro`, CSS → Tailwind utilities |
| #4 | `DailyLoop` + `loopData` refactor | React → `.astro`; refactor `loopData.tsx` → `loopData.ts` with segment arrays (see data-refactor pattern) |
| #5 | `CastRoster` | React → `.astro`, CSS → Tailwind utilities |
| #6 | `PillCards` | React → `.astro`, CSS → Tailwind utilities |
| #7 | `HeroStage` + `HeroContent` | Outer shells → `.astro`; `DigitalRain` and `CodePanel` remain React islands with narrower mount boundaries |
| #8 | `ApproachCards` | Outer → `.astro`; inline `manifestBody`/`dockerfileBody` refactored to segment arrays; `TabbedBuilder` remains React island |
| #9 *(optional)* | `TabbedBuilder` | React → `.astro` + tiny vanilla JS for tab-switching (eliminates one island) |

**CSS migration strategy:**

- Each PR moves one component's CSS rules from `styles.css` → Tailwind utilities on the `.astro` markup
- `--landing-*` tokens added to Tailwind's `@theme` block in `global.css` so `bg-landing-panel`-style utilities work
- `styles.css` shrinks PR-by-PR; after PR #8, only the 5 React islands (4 after PR #9) reference it
- Final state: ~300 lines of CSS scoped to the islands, everything else utility-class-driven

**Hydration optimization** (done as part of PR #7 or separate):

- Switch Landing root from `client:load` → per-island directives (`client:idle` for `DigitalRain`/`CodePanel`/`TabbedBuilder`; `client:visible` for `VocabularyDictionary`/`CompositionMachine`)
- Each island becomes its own hydration boundary

**Visual enforcement:**

- Every Pass B PR includes a screenshot of the ported section side-by-side with current
- "Exactly the same look and feel" is enforced by review, not automation

### Phase 5 — Layout polish

- Port `layout.tsx`'s `enhanceOutline()` (sliding sidebar indicator, 40 LOC) to a `<script>` tag injected via `Head.astro`
- Selector updates: `.vocs_Outline` → `.sl-sidebar` (verify during Phase 1)
- Font injection: `layout.tsx`'s `ensureFontsLink()` becomes a `<link>` tag in `Head.astro`; optional self-host fonts in `public/fonts/`
- Delete `default-dark.js` (replaced by Starlight's `defaultMode: 'dark'`)
- Delete the "Ask AI" CTA rename logic (the CTA itself doesn't exist in Starlight)
- **Ship criteria**: sidebar indicator behaves identically to current Vocs implementation; dark mode on first visit works with no FOUC

### Phase 6 — Cutover

1. Final PR on branch `feature/astro-starlight-migration`:
   - `git mv docs docs-vocs-legacy`
   - `git mv docs-astro docs`
   - Update any `.github/workflows/` paths referencing `docs/`
   - Update documentation that references `docs/` structure (e.g., `PROJECT_STRUCTURE.md`)
2. Deploy; verify `jackin.tailrocks.com` serves the Astro build
3. Smoke test: click every sidebar link, verify Pagefind search works, verify landing visually matches
4. Keep `docs-vocs-legacy/` for one release cycle
5. Follow-up PR: delete `docs-vocs-legacy/`

## Configuration

### `astro.config.mjs` (sketch)

```js
import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import react from '@astrojs/react'
import mdx from '@astrojs/mdx'
import starlightTailwind from '@astrojs/starlight-tailwind'

export default defineConfig({
  site: 'https://jackin.tailrocks.com',
  integrations: [
    react(),
    mdx(),
    starlight({
      title: "jackin'",
      description: 'CLI for orchestrating AI coding agents in isolated containers',
      defaultMode: 'dark',
      social: [{ icon: 'github', href: 'https://github.com/jackin-project/jackin' }],
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
      components: {
        Head: './src/components/overrides/Head.astro',
      },
      customCss: [
        './src/styles/global.css',
        './src/styles/tempo-tokens.css',
        './src/styles/docs-theme.css',
      ],
      plugins: [starlightTailwind()],
    }),
  ],
})
```

### `src/content/config.ts`

```ts
import { defineCollection } from 'astro:content'
import { docsSchema } from '@astrojs/starlight/schema'

export const collections = {
  docs: defineCollection({ schema: docsSchema() }),
}
```

## Theme Migration Details

### `tempo-tokens.css` (185 LOC)

Copied unchanged. It's Radix color ramps with `@theme` block — framework-agnostic.

### `docs-theme.css` (164 LOC)

Rewritten. Currently maps Vocs CSS vars to Radix tokens via `light-dark()`. Target: map Starlight's CSS vars to the same Radix tokens.

Starlight CSS var surface area (reference during rewrite):
- `--sl-color-bg`, `--sl-color-bg-nav`, `--sl-color-bg-sidebar`, `--sl-color-bg-inline-code`
- `--sl-color-text`, `--sl-color-text-accent`, `--sl-color-text-invert`
- `--sl-color-hairline`, `--sl-color-hairline-light`, `--sl-color-hairline-shade`
- `--sl-color-accent`, `--sl-color-accent-low`, `--sl-color-accent-high`
- `--sl-font`, `--sl-font-mono`
- `--sl-nav-height`, `--sl-sidebar-width`, `--sl-content-width`

### `default-dark.js`

Deleted. Replaced by Starlight's `defaultMode: 'dark'` config + built-in theme script.

### Fonts

Currently injected at runtime via `layout.tsx`'s `ensureFontsLink()`. Replaced by a `<link>` tag in `src/components/overrides/Head.astro`. Optional future improvement: self-host Inter, JetBrains Mono, and Fraunces in `public/fonts/`.

### Sliding sidebar indicator

Port `layout.tsx`'s `enhanceOutline()` (lines 41-93) as a vanilla `<script>` in `Head.astro`. Logic unchanged; only the selector swaps (`.vocs_Outline` → `.sl-sidebar` or Starlight's equivalent — verify during Phase 1).

## Testing

### Unit tests

- `rainEngine.test.ts` runs via `bun test` on the rainEngine module (framework-agnostic)
- No new unit tests for Astro components (they're declarative markup; visual review is the regression test)

### Visual regression

- Manual screenshot diffs at Phase 3 (Pass A complete) checkpoint
- Manual screenshot diff on every Pass B PR
- No automated pixel-diff tool

### Smoke tests

- `bun run build` must succeed with zero warnings
- `bun run preview` + click every sidebar link
- Pagefind index must include all 25 pages (check `dist/pagefind/` after build)
- Dark mode on first visit: no FOUC
- `prefers-reduced-motion` honored in `DigitalRain` and `CodePanel`

### CI / build verification

- Each PR's preview deploy (GitHub Actions or equivalent) is the primary gate
- Before Phase 6 cutover: verify CI workflows that reference `docs/` still work after rename

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Visual regression on landing | Two-pass port with checkpoints; `styles.css` preserved intact during Phase 3 |
| Sidebar chrome differs from Vocs | Phase 1 throwaway-page verification before any content migration |
| Dark-mode FOUC on first paint | Rely on Starlight's built-in init script; verify in Phase 1 |
| Codemod misses edge cases | Dry-run mode, human review of all 25 files before commit |
| Functional loss of "Ask AI" CTA | Accepted — replaced by Pagefind built-in search per user decision |
| CI workflow paths break at cutover | Grep `.github/workflows/` for `docs/` references before Phase 6 |
| Roll-back difficulty | `docs-vocs-legacy/` retained for one release cycle after cutover |

## Effort Estimate

- **Phase 0**: 2-3 hours (scaffold, verify build)
- **Phase 1**: 3-4 hours (`docs-theme.css` rewrite is the bulk)
- **Phase 2**: 3-4 hours (codemod + review)
- **Phase 3**: 2-3 hours (copy components, verify visual parity)
- **Phase 4**: 9-14 hours spread across 9-10 PRs (PR #0 wrapper + #1-#8 component ports + optional #9; PR #4 slightly longer due to loopData refactor)
- **Phase 5**: 2-3 hours (sidebar indicator + polish)
- **Phase 6**: 1-2 hours (rename + deploy + verify)

**Total**: 21-31 hours, ~12-15 PRs if sized small for reviewability. First 3 phases can ship in isolation without blocking anything.

## Open Questions

None at spec time. All decisions resolved during brainstorming:

- Landing approach: ship-of-Theseus (Phase 3 islands-only, Phase 4 incremental Astro port) ✓
- Search: Pagefind built-in, drop "Ask AI" ✓
- Directory approach: sibling `docs-astro/` until cutover ✓
- Landing routing: `src/pages/index.astro` outside content collection ✓
