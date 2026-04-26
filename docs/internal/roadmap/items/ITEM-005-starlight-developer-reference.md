# ITEM-005: Set up Starlight "Developer Reference" section

**Phase:** 1  
**Risk:** low  
**Effort:** medium (1–2 days)  
**Requires confirmation:** yes — sidebar structure and URL changes need sign-off  
**Depends on:** none (but items 002, 003 depend on this being done first)

## Summary

All internal docs (specs, ADRs, architecture, roadmap items, code tour) are browsable on the live Starlight site at `jackin.tailrocks.com/internal/`. This requires:
1. Creating `docs/src/content/docs/internal/` directory with placeholder pages
2. Adding a collapsed "Developer Reference" sidebar group to `docs/astro.config.ts`

## What gets created

```
docs/src/content/docs/internal/
  index.mdx              ← "Developer Reference" landing page
  architecture.mdx       ← high-level module architecture (from roadmap research)
  code-tour.mdx          ← stub: key call chains (load, console launch, hardline)
  contributing.mdx       ← moved from root CONTRIBUTING.md
  testing.mdx            ← moved from root TESTING.md
  specs/                 ← behavioral + feature specs (ITEM-002, 003 land here)
  decisions/             ← ADRs (ITEM-010 lands here)
  roadmap/               ← roadmap index + items (this file lives here)
```

## astro.config.ts sidebar addition (lines 50–103 today)

```typescript
{
  label: 'Developer Reference',
  collapsed: true,
  items: [
    { label: 'Architecture', link: '/internal/architecture/' },
    { label: 'Code Tour', link: '/internal/code-tour/' },
    { label: 'Contributing', link: '/internal/contributing/' },
    { label: 'Testing', link: '/internal/testing/' },
    { autogenerate: { directory: 'internal/decisions', label: 'Decisions' } },
    { autogenerate: { directory: 'internal/specs', label: 'Specs' } },
    { autogenerate: { directory: 'internal/roadmap', label: 'Roadmap' } },
  ]
}
```

## Steps

1. Create the directory structure with stub MDX files (frontmatter + one-paragraph placeholder).
2. Add the sidebar group to `docs/astro.config.ts`.
3. Verify `bun run build` passes — no broken links from the new pages.
4. Verify the lychee link check still passes (`bun run check:links:fresh`).

## What needs confirmation

- The URL structure (`/internal/...`) — confirm this is the right prefix.
- Whether to move CONTRIBUTING.md and TESTING.md now (in this item) or separately (ITEM-009).
- `collapsed: true` for the sidebar group — confirm this is the right default UX.

## Caveats

- Draft MDX pages with broken links will fail the `docs-link-check` CI gate. Use only working links or no links in placeholder pages.
- `docs/internal/` on the filesystem is NOT the same as `docs/src/content/docs/internal/` — the filesystem path is the roadmap items directory; the Starlight path is the site content.
