# Plan 039: Relocate/version the root `PLAN.md` docs-CSS redesign plan

> **Executor instructions**: Small docs-hygiene fix. First determine whether the plan is already executed.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- PLAN.md docs/src/styles/docs-theme.css docs/src/components/landing/styles.css`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (coordinate with 038 if PLAN.md gets added to the checker)
- **Category**: docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`PLAN.md` at the **Rust repo root** is a docs-site light-mode CSS redesign plan — it scopes
`docs/src/styles/docs-theme.css` and `docs/src/components/landing/styles.css` (both **live** — `docs/src/`
is the current TanStack Start app, not stale Astro). It has **no `Status`/date header**, so an agent
scanning root planning docs can't tell if it's active, done, or abandoned, and may act on a superseded
design. Root is the wrong altitude for a docs-only concern, sitting alongside `README`/`AGENTS.md`.

## Current state

- `PLAN.md:7` — scopes `docs/src/styles/docs-theme.css` + `docs/src/components/landing/styles.css` (both exist).
- `PLAN.md:130` — points at `docs/AGENTS.md` Theme section.
- No `Status:`/date; git shows it last touched by `3bdfa3414` (brand PR #496) — execution state unknowable
  from the doc.

## Scope

**In scope:** `PLAN.md` (relocate or delete + add status header). **Out of scope:** actually doing the
light-mode redesign (that's the plan's *content*, a separate task); the CSS files.

## Steps

### Step 1: Determine execution state

Compare the plan's §2 target tokens against the current stylesheet. Read the light scope in
`docs/src/styles/docs-theme.css` (`grep -n "data-theme='light'\|--jk-panel\|--jk-accent\|--jk-ui" docs/src/styles/docs-theme.css`)
and check whether the neutral-grey palette + `#0b774e` accent + single border family that `PLAN.md` §2
prescribes are already in place.
- If **already executed** (tokens match) → the plan is done; go to Step 2a.
- If **not executed** (tokens still green-tinted / old) → the plan is live; go to Step 2b.

### Step 2a: Delete the executed plan

If the redesign already shipped, delete `PLAN.md` (git history preserves it). Record in the row note that it
was executed (with the token evidence).

### Step 2b: Relocate + add a status header

If still live, move it under `docs/` (e.g. `docs/PLAN-light-mode.md`) and add a header:
`**Status**: Open | **Last updated**: <date from git> | **Scope**: docs-site light mode`. Update the
reference at old `PLAN.md:130` context and anything pointing at root `PLAN.md`
(`grep -rn "PLAN.md" . --include=*.md --include=*.rs --include=*.toml | grep -v node_modules`).

**Verify**: `test -e PLAN.md && echo "root PLAN.md still exists" || echo "moved/deleted"` → moved or deleted;
if relocated, `test -f docs/PLAN-light-mode.md` → exists and has a `Status:` header.

### Step 3: Coordinate with the link checker

If plan 038 adds `PLAN.md` to a checker, ensure it points at the **new** location (or is removed if deleted).

## Done criteria

- [x] A recorded decision (executed→deleted, or live→relocated) with token evidence in the row note
- [x] Root `PLAN.md` no longer exists; if live, it lives under `docs/` with a `Status:`/date header
- [x] No reference points at a now-missing root `PLAN.md`
- [x] `plans/README.md` row updated

## Completion notes

- Determined the light-mode redesign had already shipped, so the root `PLAN.md` was deleted rather than relocated.
- Token evidence: `docs/src/styles/docs-theme.css` light scope uses `--jk-panel: #f4f5f7`, `--jk-accent: #0b774e`, and the neutral `rgba(17, 24, 39, ...)` border family; `docs/src/components/landing/styles.css` mirrors those values and uses neutral card shadows.
- `git log -- PLAN.md` showed the stale root plan was last touched by `3bdfa3414` on 2026-06-28 (`feat(brand): ship identity system (#496)`), matching the executed redesign.

## STOP conditions

- The stylesheet is *partially* migrated (some §2 tokens applied, others not) — report the partial state;
  the plan is neither done nor untouched, and the maintainer should decide whether to finish it.

## Maintenance notes

- Root-level planning docs for docs-only concerns are mis-filed; a reviewer should keep docs-site plans under
  `docs/`.
- This plan (`plans/039`) and the improve-skill `plans/` dir are a different thing from `PLAN.md` — don't
  confuse them.
