# Plan 030: Fix the `bans.skip` posture so the duplicate-version gate actually enforces

> **Executor instructions**: Small policy/config change. Decide the posture, apply it consistently. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- deny.toml`

## Status

- **Result**: DONE — Option A, duplicate-version gate promoted to deny
- **Priority**: P3
- **Effort**: S
- **Risk**: MED (flipping to deny fails CI until transitive dups are skipped)
- **Depends on**: none (cleaner after 028)
- **Category**: migration / dx
- **Planned at**: commit `46511939d`, 2026-07-03

## Completion notes

Chose Option A. `multiple-versions = "deny"` now makes the duplicate-version skip list an enforced allowlist. The list was reconciled to the current transitive graph after plans 028 and 029: first-party workspace duplicates are gone, current transitive duplicates are skipped with the existing rationale style, and unmatched target-specific `windows-sys` skips were removed.

`cargo deny check bans` is green in deny mode. `cargo deny check advisories bans licenses sources` also exits 0; it still prints the pre-existing unmatched `ring@0.17.14` license-exception warning, but reports `advisories ok, bans ok, licenses ok, sources ok`. `cargo audit` exits 0.

## Why this matters

`deny.toml` sets `multiple-versions = "warn"`, so the 30-line pinned `bans.skip` list **never fails** the
gate — and `cargo deny check bans` still warns on ~20 *un-skipped* transitive duplicates
(strum/strum_macros, itertools 0.10+0.13, bindgen 0.69+0.72, rand/rand_core 0.8+0.9, two toml/winnow
stacks, windows-sys, rustix). Because the gate is warn-only, the skip list reads as an enforcement
mechanism while enforcing nothing — misleading a future maintainer into thinking duplicates are gated. The
duplicates are mostly transitive (unresolvable from this workspace), so the fix is about **honesty of the
posture**, not deduping.

## Current state

- `deny.toml:107-108` — `[bans] multiple-versions = "warn"`.
- `deny.toml:116-151` — 30-entry `skip = [...]` list, each "Existing duplicate-version debt … keep highest
  current version visible for future drift."
- `cargo deny check bans` → passes (warn-only) but prints ~20 un-skipped duplicate warnings.

## Scope

**In scope:** `deny.toml` (the `[bans]` posture + skip list + a clarifying comment) and, if plan 028
already removed the first-party duplicates, reconciling the list. **Out of scope:** actually deduping
transitive deps (needs upstream bumps — out of this workspace's control).

## Steps

### Step 1: Pick a posture (record the choice)

Two coherent options:
- **(A) Promote to `deny` + make the skip list real.** Set `multiple-versions = "deny"`, then add the
  currently-warning transitive duplicates to `skip` so the gate is green *today* but any **new** duplicate
  fails CI. This is the stronger posture — the skip list becomes a real allowlist that catches drift.
- **(B) Keep `warn`, but document it as advisory-only.** Add a comment at `[bans]` stating the gate is
  advisory (warn) and the skip list is informational, so no one mistakes it for enforcement.

Recommend **(A)** — it turns the existing (already-maintained) skip list into an actual gate. Do (B) only
if the operator wants to avoid CI churn from transitive bumps.

### Step 2a (if A): promote and complete the skip list

Set `multiple-versions = "deny"`. Run `cargo deny check bans`, add each reported un-skipped duplicate to
`skip` with the same rationale-comment style. Iterate until the check is green. Keep the pinned versions as
the *highest* current, matching the existing convention.

**Verify**: `cargo deny check bans` → exit 0 (now actually enforcing);
introduce a fake duplicate mentally / confirm the gate would now fail a *new* duplicate (the skip list is
exhaustive for current state).

### Step 2b (if B): document advisory-only

Add the clarifying comment. `cargo deny check bans` stays warn.

**Verify**: `grep -n "advisory\|informational" deny.toml` → the clarifying comment is present.

### Step 3: Keep `.cargo/audit.toml` in sync

The repo mandates (`crates/AGENTS.md`) that `.cargo/audit.toml` mirrors `deny.toml` advisory ignores. This
plan touches bans, not advisories, but confirm no ignore drift was introduced:
`cargo audit` → exit 0.

## Done criteria

- [x] A recorded posture decision (A or B) in the row note
- [x] Option A: `multiple-versions = "deny"` and `cargo deny check bans` green (skip list complete); **or**
      Option B: warn documented as advisory-only
- [x] `cargo deny check advisories bans licenses sources` exits 0
- [x] `cargo audit` exits 0
- [x] `plans/README.md` row updated

## STOP conditions

- Option A: a transitive duplicate can't be skipped (deny reports something the skip syntax won't match) —
  report the exact crate; may need a different `deny` knob.
- Landing this alongside plan 028 double-counts first-party duplicates — sequence after 028 so the skip list
  only covers genuine *transitive* dups.

## Maintenance notes

- If Option A: every new transitive duplicate now fails CI and must be consciously skipped (with rationale)
  or resolved — that's the intended friction. A reviewer should require a rationale comment on any new skip.
- Keep the `deny.toml` ↔ `.cargo/audit.toml` rationale comments in sync (repo rule).
