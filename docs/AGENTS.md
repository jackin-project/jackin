# Docs AGENTS.md

## Stack

- This directory is an Astro + Starlight documentation site.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro` with `@astrojs/starlight`, `@astrojs/react`, and MDX auto-provided by Starlight.
- Styling: `tailwindcss` v4 (CSS-first configuration in `src/styles/global.css`).

## Language

**Write TypeScript, not JavaScript.** This applies to every file that isn't
MDX or CSS:

- Astro config: `astro.config.ts` (not `.mjs`/`.js`)
- Build scripts: `scripts/*.ts` (not `*.mjs`/`*.js`). Run via `bun run <file.ts>`.
- Route endpoints (e.g. `src/pages/og/[...slug].png.ts`): `.ts`
- React components: `.tsx`
- Inline `<script>` in `.astro` files: keep TypeScript by default; use
  `is:inline` for pre-hydration scripts only when the timing absolutely
  requires running before Astro's module loader.

Rationale: TypeScript catches integration errors between Starlight's
route data, Astro config shapes, and our own helpers at edit time
rather than at build time. Bun runs `.ts` natively — there is no
transpile step or build-config cost to choose TS over JS/MJS.

Do not introduce new `.mjs` or plain `.js` files. If you encounter one
left over from an older version of this site, port it to `.ts` as part
of whatever task brought you there.

### Strict mode

The docs `tsconfig.json` extends `astro/tsconfigs/strict`, which turns
on `strict: true` plus Astro's recommended strict defaults
(`forceConsistentCasingInFileNames`, `isolatedModules`, etc.). Treat
strict mode as non-negotiable:

- Do not add `tsconfig` overrides that weaken it (e.g. `strict: false`,
  `strictNullChecks: false`, `noImplicitAny: false`).
- Do not use `// @ts-ignore` or `// @ts-nocheck` to silence errors.
  If a third-party type is genuinely wrong, use `// @ts-expect-error`
  with a one-line explanation and a link/issue reference so the escape
  hatch is auditable.
- New code should pass `bunx tsc --noEmit` cleanly.

Running `astro/tsconfigs/strictest` (adds `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, and similar) is a desirable follow-up
goal but not a current requirement — some existing code (rainEngine
indexed access, astro-og-canvas optional-property types) would need
targeted cleanup first.

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- Do not use `npm`, `pnpm`, or `yarn` in this directory.
- Keep `bun.lock` as the only lockfile in `docs/`.

## Common Commands

**First-time setup (and after pulling dependency changes):**

```sh
bun install --frozen-lockfile
```

This populates `node_modules/` with the platform-specific optional native binaries (e.g. `@rollup/rollup-darwin-arm64` on Apple Silicon). Skipping it makes `bun run dev` fail with `Cannot find module '@rollup/rollup-...'`.

**Day-to-day:**

- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Check links in the existing production build: `bun run check:links`
- Rebuild and then check links: `bun run check:links:fresh`
- Run tests: `bun test`

`bun run check:links` and `bun run check:links:fresh` require the `lychee` CLI (for example, `brew install lychee` on macOS).

**If `node_modules` was last installed on a different OS** (e.g. an agent built from a Linux container shared the working tree with a macOS host), Bun won't always re-resolve optional native binaries on its own. Reset with:

```sh
rm -rf node_modules
bun install --frozen-lockfile
```

## Naming conventions

### Operator console

- **`jackin console` is the canonical name for the TUI.** Use it
  everywhere in prose, comparisons, examples, and screenshots —
  including the second and subsequent mentions on the same page.
- **The bare `jackin` shortcut is explained exactly once**, in the
  console command page (`docs/src/content/docs/commands/console.mdx`),
  as syntactic sugar that opens the console on an interactive terminal.
  No other doc page should explain the shortcut, mention it inline,
  or use the dual-form `(\`jackin\` or \`jackin console\`)` /
  `\`jackin\` / \`jackin console\``. Both forms confuse readers and
  imply they're separate commands.
- **`jackin console` is deliberately less featured than `jackin load`**
  (and the other CLI subcommands). The console is the simplified,
  intuitive front for common day-to-day flows. New niche flags and
  detail-level options land on the CLI first, and often exclusively.
  When comparing the two, make that relationship explicit — do not
  imply they are feature-equivalent front-ends.
- **`jackin launch`** is the deprecated pre-console spelling. Mention it
  only in a deprecation context; never recommend it for new users or
  use it in new examples.

## Content Notes

- Docs content lives under `src/content/docs/` as MDX files.
- Content collection defined in `src/content.config.ts` using the Starlight `docsLoader()` (Astro 6 Content Layer API).
- File-based routing: `src/content/docs/foo/bar.mdx` → `/foo/bar`.
- Link to docs pages with site-absolute routes such as `/guides/mounts/`.
  The generated-site lychee check is the authoritative guard for these links,
  including fragments.
- Link to repository files with `<RepoFile path="src/runtime/image.rs" />`
  rather than plain code spans when the reader should be able to open the file.
  Import it from `src/components/RepoFile.astro` with the appropriate relative
  path for the MDX file. The component renders a GitHub `blob/main` URL, and the
  CI link check remaps that URL to the PR checkout, so file renames and deletions
  fail before merge. Avoid `tree/main` directory links unless there is a specific
  reason they cannot point at a concrete file.
- Plain inline-code references to existing repo files under `src/`, `docs/`,
  `docker/`, or `.github/` fail `bun run check:repo-links`; link them instead.
  Proposed future files that do not exist yet may stay as code spans.
- `check:repo-links` exists because lychee only checks real links in rendered
  HTML. Plain code spans like `src/runtime/launch.rs` are not links, so lychee
  cannot detect when those files are renamed or deleted. The source check makes
  those references use `<RepoFile />`, then lychee verifies the generated URL.
- `mailto:` links are included in lychee checks, so use real, intentional email
  addresses rather than placeholders.
- Sidebar and top-nav are configured in `astro.config.ts`.
- **Roadmap sidebar discipline.** Every MDX file under
  `src/content/docs/reference/roadmap/` must have a matching entry in
  the sidebar in `astro.config.ts` (under `Reference → Roadmap → Open
  items`, `Resolved`, or `Codebase health` as appropriate for its
  `**Status**` field). Whenever you add, rename, delete, or change the
  status of a roadmap item — or restructure the directory — verify
  the sidebar still matches the directory contents in the same PR.
  Operators rely on the sidebar (not the overview prose) to discover
  open work, so an item reachable only via direct URL or the
  overview page is effectively hidden. To audit:

  ```sh
  ls docs/src/content/docs/reference/roadmap/*.mdx \
    | xargs -n1 basename -s .mdx | sort > /tmp/roadmap-files
  grep -oE "reference/roadmap/[a-z0-9-]+" docs/astro.config.ts \
    | sed 's|reference/roadmap/||' | sort -u > /tmp/roadmap-sidebar
  diff /tmp/roadmap-files /tmp/roadmap-sidebar
  ```

  The diff must be empty.
- Use Starlight components for callouts (`<Aside type="note|tip|caution">`),
  steps (`<Steps>` around an `<ol>`), and tabs (`<Tabs><TabItem>`). Import from
  `@astrojs/starlight/components`.
- Every page requires a `title:` field in its frontmatter.
- Self-hosted fonts via fontsource (`src/styles/fonts.css`) — no third-party font CDN.
- Keep docs and code behavior aligned; when they differ, code is the source of truth.

## Landing Page

- Landing is at `src/pages/index.astro` — a plain Astro page, NOT inside
  the Starlight content collection. It has full control over its layout.
- React components in `src/components/landing/` are mounted as islands.

## Theme

- Starlight chrome uses CSS custom properties. Our mappings live in
  `src/styles/docs-theme.css` (Starlight `--sl-*` → Radix tokens from
  `tempo-tokens.css`).
- Brand accent token `--jk-brand` is theme-aware (bright #00ff41 in dark,
  muted #16a34a in light) — used for tabs underline, sidebar active pill,
  right-rail TOC, pagination hover.
- Code blocks always use a dark surface (`--jk-code-bg`) regardless of
  page theme. Shiki theme: github-dark in dark mode, one-dark-pro in light.
