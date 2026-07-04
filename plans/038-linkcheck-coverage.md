# Plan 038: Extend the repo-link checker to `TODO.md`/`PLAN.md` and fix the stale `findings.md` reference

> **Executor instructions**: Closes the guardrail gap that let docs rot undetected. **Sequence after plans
> 035 + 036** (turning the checker on those files fails CI until their dead links are fixed). Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-xtask/src/docs.rs crates/AGENTS.md crates/CLAUDE.md test-layout-allowlist.toml .github/workflows/docs.yml`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P2
- **Effort**: S
- **Risk**: MED (enabling the check fails CI until 035/036 land)
- **Depends on**: plans 035, 036
- **Category**: docs / dx (DOCS-04 + DEBT-07)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The doc-freshness guardrail structurally **skips the two most agent-facing planning docs**. The repo-link
checker's allowlist (`REPO_LINK_ROOT_DOCS` in `crates/jackin-xtask/src/docs.rs`) omits `TODO.md` and
`PLAN.md`, so their dead paths (fixed in plans 036/039) were never link-checked; and `REPO_TOP_LEVEL_FILES`
omits `BRANCHING.md`/`COMMITS.md`, so `PROJECT_STRUCTURE.md`'s dead links to them (plan 035) passed the
checker. Fixing the docs without fixing the checker guarantees recurrence. Separately, `crates/AGENTS.md`
points at a **`findings.md`** that doesn't exist — the mechanism it described was replaced by
`test-layout-allowlist.toml` (which is empty), a doubly-stale reference the removed link-checkers would have
caught.

## Current state

- `crates/jackin-xtask/src/docs.rs:31-39` — `REPO_LINK_ROOT_DOCS = [AGENTS.md, ENGINEERING.md,
  PROJECT_STRUCTURE.md, PULL_REQUESTS.md, README.md, RULES.md, TESTING.md]` — **no TODO.md, no PLAN.md**.
- `crates/jackin-xtask/src/docs.rs:40-53` — `REPO_TOP_LEVEL_FILES` allowlist omits `BRANCHING.md`/`COMMITS.md`.
- `crates/AGENTS.md:49` / `crates/CLAUDE.md:49` — "Existing violations are tracked in `findings.md` (section
  'Test Module Layout Violations')"; **no `findings.md` exists**; `test-layout-allowlist.toml` is empty
  (`files = [ ]`).

## Scope

**In scope:** `crates/jackin-xtask/src/docs.rs` (the two const arrays), `crates/AGENTS.md` (the `findings.md`
reference; `CLAUDE.md` follows via plan 034's symlink), and the docs.yml codebook list if it references the
now-restored files. **Out of scope:** the link *targets* (fixed by 035/036/039); the file-size gate.

## Steps

### Step 1 (sequence gate): confirm 035 + 036 have landed

The dead links in `TODO.md`/`PLAN.md`/`PROJECT_STRUCTURE.md` must already be fixed (plans 035, 036, and 039
for PLAN.md) — otherwise adding these files to the checker fails CI. If they haven't landed, either land them
first or do this plan in the **same PR** as those fixes. STOP if you'd be enabling a check that's red.

### Step 2: Extend the checker's allowlists

In `crates/jackin-xtask/src/docs.rs`:
- add `"TODO.md"` and `"PLAN.md"` to `REPO_LINK_ROOT_DOCS` (if PLAN.md is relocated by plan 039, add its new
  path instead / to the docs-scoped checker);
- add `"BRANCHING.md"` and `"COMMITS.md"` to `REPO_TOP_LEVEL_FILES` (assuming plan 035 restored them at root).

**Verify**: `cargo run --bin xtask -- docs repo-links` (or `cargo xtask docs repo-links`) → exit 0 (green
because 035/036/039 fixed the targets). Temporarily reintroduce a dead link in `TODO.md` → the check now
fails (then revert).

### Step 3: Fix the `findings.md` reference

In `crates/AGENTS.md:49`, replace the `findings.md` reference with `test-layout-allowlist.toml` (the actual
mechanism), and drop the "must be fixed before adding new splits" clause now that the allowlist is empty. (Do
**not** edit `crates/CLAUDE.md` directly if plan 034 made it a symlink — the edit flows through `AGENTS.md`;
if 034 hasn't landed, edit both identically.)

**Verify**: `grep -rn "findings.md" crates/AGENTS.md crates/CLAUDE.md` → **no matches**;
`grep -n "test-layout-allowlist.toml" crates/AGENTS.md` → ≥1 match.

### Step 4: Reconcile the codebook list

If `.github/workflows/docs.yml:192-193` still enumerates `BRANCHING.md`/`COMMITS.md` for spellcheck, confirm
they now exist (plan 035) so the enumeration isn't a silent no-op — or update the list.

## Done criteria

- [x] `TODO.md`, `BRANCHING.md`, `COMMITS.md` are covered by the repo-link checker; stale root `PLAN.md` was deleted by plan 039
- [x] `cargo xtask docs repo-links` exits 0 (all targets resolve)
- [x] A reintroduced dead link in a newly-covered file makes the check fail (proven, then reverted)
- [x] `crates/AGENTS.md` no longer references the non-existent `findings.md`; points at `test-layout-allowlist.toml`
- [x] `plans/README.md` row updated

## Completion notes

- Added `TODO.md` to the root-doc scan and added `BRANCHING.md`/`COMMITS.md` to the top-level repo-path allowlist.
- Did not add `PLAN.md` because plan 039 proved the redesign had shipped and deleted the stale root file.
- Converted newly-exposed inline repo paths in `TODO.md` and `PROJECT_STRUCTURE.md` to Markdown links.
- Replaced the stale `findings.md` reference in `crates/AGENTS.md`; `crates/CLAUDE.md` follows via the symlink restored in plan 034.
- Negative proof: a temporary raw `` `BRANCHING.md` `` path in `TODO.md` made `cargo xtask docs repo-links` fail, then the temporary line was removed.

## STOP conditions

- Enabling the check surfaces a dead link that plans 035/036/039 didn't cover — fix it here if trivial, else
  report (it's an additional rot site).
- `PLAN.md` was relocated by plan 039 to a docs-scoped path the *root* checker shouldn't own — add it to the
  docs-scoped checker instead; don't force a docs file into the root-docs allowlist.

## Maintenance notes

- This is the structural fix (DEBT-07): the docs rotted because the guardrail didn't cover the steering docs.
  A reviewer should ensure any *new* root planning doc is added to the checker at creation.
- #664 also deleted `docs/scripts/check-repo-links.ts`/`check-roadmap-sidebar.ts`; the xtask commands replaced
  them — confirm no doc still points at the deleted TS scripts.
