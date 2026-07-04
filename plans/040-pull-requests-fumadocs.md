# Plan 040: Update `PULL_REQUESTS.md` off the Astro/Starlight docs workflow

> **Executor instructions**: Docs-accuracy fix on a doc that governs agent PR behavior. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- PULL_REQUESTS.md`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`PULL_REQUESTS.md` (30KB, directly governs agent PR behavior) still describes the **Astro/Starlight** docs
workflow the repo migrated away from. It tells a PR author to edit `docs/astro.config.ts` (deleted) and to
run a sidebar audit procedure that's superseded, and references how "Starlight renders" pages. These stale
bits are prose/inline-code (not markdown links), so the repo-link checker doesn't catch them — a PR author
following this doc edits a file that doesn't exist.

## Current state

- `PULL_REQUESTS.md:63` — "changes under `docs/**` … `astro.config.ts` sidebar, theme/CSS".
- `PULL_REQUESTS.md:65` — "how **Starlight** renders page, whether Fumadocs repository-file links resolve"
  (half-migrated wording).
- `PULL_REQUESTS.md:219` — "Sidebar audit must show no diff after deleting entry from `docs/astro.config.ts`".
- Reality: sidebar is `meta.json` + `cargo xtask roadmap audit`; the site is Fumadocs on TanStack Start.

## Scope

**In scope:** `PULL_REQUESTS.md`. **Out of scope:** the docs site config; other stale docs (separate plans).

## Steps

### Step 1: Replace the Astro/Starlight references

- `:63` — replace `astro.config.ts` sidebar reference with `meta.json` (the Fumadocs sidebar mechanism).
- `:65` — drop "Starlight"; describe Fumadocs rendering / repo-link resolution in current terms.
- `:219` — replace the "delete entry from `docs/astro.config.ts`" sidebar-audit procedure with the current
  one: `cargo xtask roadmap audit` (already referenced correctly elsewhere in the same file — reuse that
  wording for consistency).

**Verify**: `grep -n "astro.config.ts\|Starlight" PULL_REQUESTS.md` → **no matches**.

### Step 2: Sweep the file for other migration residue

`grep -in "astro\|starlight\|src/content" PULL_REQUESTS.md` → fix any remaining stale references (e.g.
`docs/src/content` paths → `docs/content/docs`).

**Verify**: `grep -in "astro\|starlight" PULL_REQUESTS.md` → no matches.

## Done criteria

- [x] `grep -n "astro.config.ts\|Starlight" PULL_REQUESTS.md` → no matches
- [x] Sidebar-audit procedure references `meta.json` + `cargo xtask roadmap audit` (matching the rest of the file)
- [x] Any `docs/src/content` path references corrected to `docs/content/docs`
- [x] `plans/README.md` row updated

## Completion notes

- Replaced the docs-only PR guidance's `astro.config.ts` sidebar wording with Fumadocs `meta.json` sidebar files.
- Replaced the Starlight rendering wording with current Fumadocs rendering and repository-file link language.
- Updated roadmap freshness and retirement references from the old reference-roadmap path to `docs/content/docs/roadmap/` and its overview.
- Updated the sidebar-retirement procedure to edit the relevant roadmap `meta.json` entry and run `cargo xtask roadmap audit`.

## STOP conditions

- A referenced procedure (e.g. a specific audit command) doesn't actually exist under the new toolchain —
  report; find the real command via `cd docs && grep "check:" package.json` and `cargo xtask --help`.

## Maintenance notes

- This file is scanned by `repo-links` but the stale bits are prose (not links), so they slipped through —
  the fix is manual. A reviewer should sweep for migration residue when the docs toolchain changes again.
