# Plan 037: Consolidate the two colliding ADR sets

> **Executor instructions**: Docs-structure fix. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- docs/adr docs/content/docs/reference/adrs docs/content/docs/roadmap/index.mdx`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

There are **two divergent ADR sets with colliding numbers**. The root `docs/adr/` set (0001 launch-never-
reconnects, 0002 role-hooks-are-author's-domain, 0003 cleanup-requires-approval) records runtime-behavior
decisions but is **orphaned** — zero inbound references anywhere, never rendered on the site. The published
set `docs/content/docs/reference/adrs/` (adr-001 single-crate-vs-workspace … adr-005 capsule-single-render-path)
records build/TUI decisions, with numbers 001-003 **clashing** with the root set's. So the launch/role/
cleanup decisions the maintainer treats as settled are invisible on the docs site, and an agent told to
"write an ADR" faces two schemes with clashing numbers. (`roadmap/index.mdx:15` also says "four foundational
ADRs" but five are published.)

## Current state

- `docs/adr/0001-launch-never-reconnects-to-a-live-instance.md`, `0002-role-hooks-are-the-role-authors-domain.md`,
  `0003-cleanup-requires-explicit-operator-approval.md` — runtime ADRs, **zero** inbound references, not on site.
- `docs/content/docs/reference/adrs/` — published: `adr-001-single-crate-vs-workspace.mdx`,
  `adr-002-rust-toolchain.mdx`, `adr-003-ratatui.mdx`, `adr-004-pane-body-rendering.mdx`,
  `adr-005-capsule-single-render-path.mdx`, plus `index.mdx`, `meta.json`.
- `docs/content/docs/roadmap/index.mdx:15` — "four foundational ADRs" (should be five).

## Scope

**In scope:** `docs/adr/*`, `docs/content/docs/reference/adrs/` (index + meta.json), the "four/five" count.
**Out of scope:** the *content* of the decisions (they stay authoritative — this is about location/numbering/
visibility).

## Steps

### Step 1: Decide the single scheme

Two coherent options:
- **(A) Migrate root ADRs into the published set** as `adr-006-launch-never-reconnects`,
  `adr-007-role-hooks-authors-domain`, `adr-008-cleanup-explicit-approval` (next numbers after 005),
  convert to `.mdx`, add to `meta.json`, and delete `docs/adr/`. One scheme, no collisions, all visible.
- **(B) Keep `docs/adr/` as a distinct "runtime ADRs" set** but explicitly scope it (a README stating it's
  runtime-behavior ADRs, renumber to avoid collision, e.g. `R001`), and **link it from the published ADR
  index** so it's discoverable.

Recommend **(A)** — one set, one numbering, everything rendered. It also pairs with plan 024, which wants to
add a container-backend ADR to the published set.

### Step 2: Execute the chosen scheme

For (A): convert the three root ADRs to `.mdx` (match the frontmatter/format of an existing
`docs/content/docs/reference/adrs/adr-00X.mdx`), place them as adr-006/007/008, add entries to the adrs
`meta.json` (in order), delete `docs/adr/`. Preserve the decision text verbatim (only reformat).

**Verify**: `ls docs/content/docs/reference/adrs/*.mdx` → shows adr-001…008;
`test -d docs/adr && echo "STILL EXISTS" || echo "removed"` → `removed` (for option A);
the adrs `meta.json` lists all eight.

### Step 3: Fix the count

Update `docs/content/docs/roadmap/index.mdx:15` "four foundational ADRs" to the correct number (five before
migration, eight after option A — set it to whatever is true after your change).

**Verify**: `grep -rn "four foundational\|five foundational" docs/content/docs/roadmap/index.mdx` → the count
matches the actual number of published ADRs.

### Step 4: Run the docs sidebar/link audits

**Verify**: `cd docs && bun run check:roadmap-sidebar` (or `cargo xtask roadmap audit`) → no diff;
if a build is cheap, `cd docs && bun run build` → succeeds (ADR pages render).

## Done criteria

- [x] One ADR scheme with no colliding numbers; the runtime ADRs are visible on the site (option A) or
      explicitly scoped + linked (option B)
- [x] `docs/adr/` removed (A) or given a scoping README + renumber (B)
- [x] The "N foundational ADRs" count matches reality
- [x] Docs sidebar audit passes; ADR index lists all ADRs
- [x] `plans/README.md` row updated

## Completion notes

- Chose option A: one published ADR scheme under `docs/content/docs/reference/adrs/`.
- Migrated the three orphaned runtime ADRs to `adr-008`, `adr-009`, and `adr-010`; ADR-006 and ADR-007 already existed from earlier PR #713 work.
- Removed the old `docs/adr/` files so there are no colliding root ADR numbers.
- Updated the ADR `meta.json`, ADR index, and roadmap count to ten published foundational ADRs.

## STOP conditions

- Some code/doc *does* reference `docs/adr/000X` after all (the "zero references" was wrong) — find and
  repoint those links before deleting; don't break a live link.
- The published-set frontmatter schema can't represent a runtime ADR cleanly — report; may need a small
  frontmatter addition.

## Maintenance notes

- Plan 024 will add a container-backend ADR — coordinate numbering so it's the next after this plan's result.
- Reviewer: confirm the launch/role/cleanup decisions are now discoverable from `/reference/adrs/` so they
  stop getting re-derived.
