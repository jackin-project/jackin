# Docs AGENTS.md

## Stack

- This directory is a Vocs documentation site (React + Vite).
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `vocs` with `vite`.
- Styling: `tailwindcss` v4 (CSS-first configuration in `pages/_root.css`).

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- Do not use `npm`, `pnpm`, or `yarn` in this directory.
- Keep `bun.lock` as the only lockfile in `docs/`.

## Common Commands

- Install dependencies: `bun install --frozen-lockfile`
- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`

## Content Notes

- Treat this as a Vocs documentation site.
- Main docs content lives under `docs/pages/`.
- File-based routing: `pages/foo/bar.mdx` → `/foo/bar`.
- Sidebar is configured in `vocs.config.ts`.
- Use Vocs directives for callouts (`:::note`, `:::tip`, `:::warning`), steps (`::::steps`), and code groups (`:::code-group`).
- Keep docs and code behavior aligned; when they differ, code is the source of truth.
