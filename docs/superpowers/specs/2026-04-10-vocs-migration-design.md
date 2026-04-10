# Docs Migration: Astro Starlight to Vocs

## Summary

Rebuild the jackin' documentation site using [Vocs](https://vocs.dev/) (React + Vite) with Tailwind CSS 4, replacing the current Astro + Starlight setup. Clean rebuild approach — fresh Vocs project, migrate all 25 MDX pages.

## Decisions

- **Fresh start on visuals** — no logos, no Matrix theme, Vocs defaults
- **Keep deployment** — `jackin.tailrocks.com` via GitHub Pages (CNAME preserved)
- **Package manager** — Bun first, pnpm as fallback
- **Landing page** — Vocs built-in `HomePage` component
- **Tailwind CSS 4** — CSS-first configuration (no `tailwind.config.js`)
- **Reference project** — [tempoxyz/docs](https://github.com/tempoxyz/docs) for patterns

## Project Structure

```
jackin/docs/
├── vocs.config.ts            # Vocs configuration
├── vite.config.ts            # Vite 7 + vocs() + react()
├── package.json              # vocs, react, react-dom, tailwindcss
├── tsconfig.json             # References app + node configs
├── tsconfig.app.json         # React JSX, strict, vocs/globals types
├── tsconfig.node.json        # For vite.config.ts
├── .gitignore                # node_modules, dist, .vocs, pages.gen.ts
├── .mise.toml                # bun = "latest"
├── AGENTS.md                 # Updated for Vocs + Tailwind
├── CLAUDE.md                 # Pointer to AGENTS.md
├── public/
│   └── CNAME                 # jackin.tailrocks.com
├── src/
│   ├── env.d.ts              # Vocs type declarations
│   ├── pages/
│   │   ├── _root.css         # Tailwind v4 + Vocs dark mode variant
│   │   ├── index.mdx         # Landing (Vocs HomePage layout)
│   │   ├── getting-started/  # why, installation, quickstart, concepts
│   │   ├── guides/           # workspaces, mounts, agent-repos, security-model, comparison
│   │   ├── commands/         # load, launch, hardline, eject, exile, purge, workspace, config
│   │   ├── developing/       # creating-agents, construct-image, agent-manifest
│   │   └── reference/        # configuration, architecture, roadmap
│   └── ...
└── scripts/                   # Empty (logo gen removed)
```

Key convention: `rootDir: '.'` with pages in `src/pages/` (following tempoxyz/docs pattern).

## Configuration

### `vocs.config.ts`

- `title: "jackin'"`
- `titleTemplate: "%s — jackin'"`
- `rootDir: '.'`
- `baseUrl: 'https://jackin.tailrocks.com'`
- Edit link pointing to GitHub `docs/src/pages/:path`
- GitHub social link
- 5-section sidebar preserving current hierarchy:
  - Getting Started (4 pages)
  - Guides (5 pages)
  - Commands (8 pages)
  - Developing Agents (3 pages)
  - Reference (3 pages)

### `vite.config.ts`

Vite 7 with `vocs()` and `@vitejs/plugin-react` plugins.

### `_root.css` (Tailwind v4)

```css
@import "tailwindcss";
@source "./";
@custom-variant dark (&:where([style*="color-scheme: dark"], [style*="color-scheme: dark"] *));
```

The `@custom-variant` is required because Vocs uses inline `color-scheme` style, not a `.dark` class.

## Content Migration

### Frontmatter

Strip Starlight-specific fields (`title`, `sidebar.order`, `template`). Keep `description`. Page title comes from first `#` heading.

### Component Mapping

| Starlight | Vocs |
|---|---|
| `<Aside type="note">` | `:::note` |
| `<Aside type="caution">` | `:::warning` |
| `<Aside type="tip">` | `:::tip` |
| `<Steps>` + `<ol><li>` | `::::steps` + `### Step title` |
| `<Tabs>/<TabItem>` | `:::code-group` (code) or manual |
| `<Card>/<CardGrid>` | Plain sections or Vocs `<Cards>`/`<Card>` |
| `<LinkCard>` | Standard markdown links |

### Landing Page

Replace Starlight splash template with Vocs `HomePage` component (`Tagline`, `Description`, `InstallPackage`, `Buttons`). Layout frontmatter: `layout: landing`.

### Content Text

No changes to actual documentation prose — only component wrappers change.

## Build & Scripts

```json
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  }
}
```

## Dependencies

| Package | Version | Purpose |
|---|---|---|
| `vocs` | ^1.4.1 | Docs framework (bundles Vite 7 internally) |
| `react` | ^19.2.5 | Peer dependency |
| `react-dom` | ^19.2.5 | Peer dependency |
| `@vitejs/plugin-react` | ^5.2.0 | React Vite plugin (last version supporting Vite 7) |
| `tailwindcss` | ^4.2.2 | Styling (CSS-first v4) |
| `typescript` | ^6.0.2 | Type checking |
| `vite` | ^7.1.11 | Build tool (pinned to 7.x — vocs requires it) |

## Files to Delete

All Astro/Starlight-specific files:
- `astro.config.mjs`
- `src/content/` (entire directory)
- `src/content.config.ts`
- `src/assets/` (logos — starting fresh)
- `src/styles/custom.css`
- `bun.lock` (regenerated)
- `scripts/generate-logos.mjs`
