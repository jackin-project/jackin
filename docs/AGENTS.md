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
- Run tests: `bun test`

**If `node_modules` was last installed on a different OS** (e.g. an agent built from a Linux container shared the working tree with a macOS host), Bun won't always re-resolve optional native binaries on its own. Reset with:

```sh
rm -rf node_modules
bun install --frozen-lockfile
```

## Naming conventions

### Operator console

- **`jackin console`** is the canonical name for the TUI. Use it in prose,
  comparisons, and examples.
- **`jackin`** (no subcommand) is a convenience shortcut that opens the
  console on an interactive terminal. Mention the shortcut **once per
  page** — ideally in the synopsis or first-run walkthrough — then use
  `jackin console` everywhere else. **Do not write `jackin / jackin console`
  inline**; it reads as two separate commands and confuses readers.
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
- Sidebar and top-nav are configured in `astro.config.ts`.
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

