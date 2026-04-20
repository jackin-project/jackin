# Docs AGENTS.md

## Stack

- This directory is an Astro + Starlight documentation site.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro` with `@astrojs/starlight`, `@astrojs/react`, and MDX auto-provided by Starlight.
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
- Content collection defined in `src/content.config.ts` using the Starlight `docsLoader()` (Astro 6 Content Layer API).
- File-based routing: `src/content/docs/foo/bar.mdx` → `/foo/bar`.
- Sidebar and top-nav are configured in `astro.config.mjs`.
- Use Starlight components for callouts (`<Aside type="note|tip|caution">`),
  steps (`<Steps>` around an `<ol>`), and tabs (`<Tabs><TabItem>`). Import from
  `@astrojs/starlight/components`.
- Every page requires a `title:` field in its frontmatter.
- Keep docs and code behavior aligned; when they differ, code is the source of truth.

## Landing Page

- Landing is at `src/pages/index.astro` — a plain Astro page, NOT inside
  the Starlight content collection. It has full control over its layout.
- React components in `src/components/landing/` are mounted as islands.
  Phase 4 of the Astro migration (tracked in superpowers/plans/) is
  incrementally porting 10 of 15 landing components to native `.astro`.
- The `Landing` wrapper is currently mounted with `client:load`, which
  hydrates the full React tree. Phase 4 will split this into per-component
  hydration directives.

## Theme

- Starlight chrome uses CSS custom properties. Our mappings live in
  `src/styles/docs-theme.css` (Starlight `--sl-*` → Radix tokens from
  `tempo-tokens.css`).
- Font injection: `src/components/overrides/Head.astro` overrides
  Starlight's default `Head` to inject Google Fonts links.

## Migration Status

This directory is the replacement for the Vocs-based `docs/` directory.
Until Phase 6 cutover, both directories coexist. After cutover, `docs/`
will be renamed to `docs-vocs-legacy/` and this directory will be
renamed to `docs/`. See:
- `docs/superpowers/specs/2026-04-20-astro-starlight-migration-design.md`
- `docs/superpowers/plans/2026-04-20-astro-starlight-migration-phase-0-3.md`
