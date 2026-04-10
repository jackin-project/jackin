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
- Main docs content lives under `docs/pages/`.
- File-based routing: `pages/foo/bar.mdx` → `/foo/bar`.
- Sidebar is configured in `vocs.config.ts`.
- Use Vocs directives for callouts (`:::note`, `:::tip`, `:::warning`), steps (`::::steps`), and code groups (`:::code-group`).
- Keep docs and code behavior aligned; when they differ, code is the source of truth.
