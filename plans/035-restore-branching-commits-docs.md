# Plan 035: Restore the deleted `BRANCHING.md` / `COMMITS.md` and fix the six dangling references

> **Executor instructions**: Recovers governance docs deleted by accident. Run every verification command.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- .github/AGENTS.md .github/CLAUDE.md PROJECT_STRUCTURE.md RULES.md .github/workflows/docs.yml`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`BRANCHING.md` and `COMMITS.md` were **deleted** in commit `4c8b94bd0` ("feat(runtime): implement instant
launch fast paths (#576)") — an unrelated runtime PR, so almost certainly an accidental drop — and the
content wasn't relocated anywhere. **Six references dangle**, including the always-loaded force-push safety
rule: `.github/AGENTS.md`/`.github/CLAUDE.md` say the "full rule lives in `BRANCHING.md`" (dead link) for
the branch-rewrite/force-push approval policy that auto-loads every agent session. So the canonical source
for the force-push rule and the commit/DCO policy is gone, and the summary points agents at a 404.

## Current state

- Deleted in `4c8b94bd0`; recoverable via `git show 4c8b94bd0^:BRANCHING.md` and `git show 4c8b94bd0^:COMMITS.md`.
- Dangling references:
  - `.github/AGENTS.md:35,158` and `.github/CLAUDE.md:35,157` — "Full rule lives in [`BRANCHING.md`](../BRANCHING.md)".
  - `PROJECT_STRUCTURE.md:51-52` — `[BRANCHING.md](BRANCHING.md)` / `[COMMITS.md](COMMITS.md)` dead links.
  - `RULES.md:11` — cites `BRANCHING.md` as an example topic file.
  - `.github/workflows/docs.yml:192-193` — codebook spellcheck enumerates `'BRANCHING.md' 'COMMITS.md'`
    (silently no-ops).

## Scope

**In scope:** re-create `BRANCHING.md` + `COMMITS.md` at repo root (or fold their content into an existing
governance doc — see Step 1), and fix the six references. **Out of scope:** rewriting the branch/commit
policy itself (recover it as-was); the `CLAUDE.md` symlink issue (that's plan 034 — but note the `.github/CLAUDE.md`
edit here must be consistent with 034's resolution).

## Steps

### Step 1: Decide — restore files vs inline the content

- **Restore (recommended, matches the doc structure):** recover both files from history and re-add them at
  root. This is the lowest-risk option and makes every existing reference resolve.
- **Inline:** fold `BRANCHING.md` content into `CONTRIBUTING.md`/`.github/AGENTS.md` and `COMMITS.md` into
  `CONTRIBUTING.md`, then repoint all six references. More edits, higher chance of missing one.

Recommend **Restore**. Recover:
```sh
git show 4c8b94bd0^:BRANCHING.md > BRANCHING.md
git show 4c8b94bd0^:COMMITS.md   > COMMITS.md
```
Read both recovered files and reconcile them against the *current* branching/commit rules in
`CONTRIBUTING.md`/`.github/AGENTS.md` — if the policy evolved since #576, update the recovered files to
match current reality (don't restore a stale policy verbatim; a stale governance doc is its own finding).

### Step 2: Verify the six references resolve

Confirm each dangling reference now points at an existing file. If Step 1 chose inline instead, repoint each
of the six references to the new home.

**Verify**: `test -f BRANCHING.md && test -f COMMITS.md && echo OK` → `OK` (restore path);
`for t in "BRANCHING.md" "COMMITS.md"; do grep -rn "$t" .github/AGENTS.md PROJECT_STRUCTURE.md RULES.md; done`
→ every hit resolves to an existing file (spot-check the links).

### Step 3: Confirm the always-loaded force-push rule is whole

The `.github/AGENTS.md` force-push/branch-rewrite summary must have its "full rule" target resolve. Read
`.github/AGENTS.md:35,158` and confirm the linked `BRANCHING.md` now exists and contains the force-push
approval rule.

## Done criteria

- [x] `BRANCHING.md` and `COMMITS.md` exist at root (restored + reconciled to current policy), or their
      content is inlined and all references repointed
- [x] All six dangling references resolve to existing files/sections
- [x] The force-push/branch-rewrite "full rule" link in `.github/AGENTS.md` resolves
- [x] `plans/README.md` row updated
- [x] (Coordinates with plan 038: once restored, add `BRANCHING.md`/`COMMITS.md` to the repo-link checker's
      allowlist so this can't recur — see plan 038.)

## Completion notes

- Restored root `BRANCHING.md` and `COMMITS.md`.
- Reconciled the restored branch policy with current repo rules: sync from `main` uses a normal merge commit by default; rebases, amends, squashes, and force-pushes require explicit operator approval.
- Reconciled commit verification guidance with current aggregate gates: `cargo xtask ci`, `mise run ci`, `cargo xtask ci --fast`, and `cargo xtask ci --e2e`.
- The `.github/AGENTS.md` force-push links now resolve through `BRANCHING.md`; `.github/CLAUDE.md` follows the same content via the Plan 034 symlink.

## STOP conditions

- The recovered files describe a policy that **conflicts** with the current `CONTRIBUTING.md`/`.github/AGENTS.md`
  rules — report the conflict; the operator must decide which is authoritative (don't silently restore a
  superseded policy).

## Maintenance notes

- Root cause: no link checker covers these files (plan 038 fixes that). Restoring without 038 risks recurrence.
- Reviewer: confirm the restored branch/commit rules match how the team *actually* works today (DCO sign-off,
  push-after-commit, force-push approval), not the #576-era snapshot.
