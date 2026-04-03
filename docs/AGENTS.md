# Docs AGENTS.md

## Stack

- This directory is a Bun-only docs app.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro`.
- Docs theme/system: `@astrojs/starlight`.

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- Do not use `npm`, `pnpm`, or `yarn` in this directory.
- Keep `bun.lock` as the only lockfile in `docs/`.

## Common Commands

- Install dependencies: `bun install --frozen-lockfile`
- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Run Astro directly: `bun run astro ...`

## Content Notes

- Treat this as an Astro Starlight documentation site.
- Main docs content lives under `docs/src/content/docs/`.
- Keep docs and code behavior aligned; when they differ, code is the source of truth.
