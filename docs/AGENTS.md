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

- Install dependencies: `bun install --frozen-lockfile`
- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Run tests: `bun test`

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

## Historical reference

The spec + plan that guided this migration live under `superpowers/`:
- `superpowers/specs/2026-04-20-astro-starlight-migration-design.md`
- `superpowers/plans/2026-04-20-astro-starlight-migration-phase-0-3.md`
