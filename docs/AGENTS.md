# Docs AGENTS.md

## Stack

- This directory is an Astro + Starlight documentation site.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro` with `@astrojs/starlight`, `@astrojs/react`, and MDX auto-provided by Starlight.
- Styling: `tailwindcss` v4 (CSS-first configuration in `src/styles/global.css`).

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
- Sidebar and top-nav are configured in `astro.config.mjs`.
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
