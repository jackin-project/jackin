# Plan 023: Decompose `jackin-console` by responsibility and re-tighten the file-size cap

> **Executor instructions**: Large structural refactor — do it in behavior-preserving slices, verifying
> after each. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-console file-size-budget.toml`

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED
- **Depends on**: plan 022 (settle the generics first)
- **Category**: tech-debt
- **Completed at**: PR #713
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`jackin-console` is 28% of the whole codebase (86.5K lines) in one crate, and the file-size budget was
**relaxed from 1500→2000 lines** to accommodate hot-path files; the ledger is empty only because of that
relaxation. At least one file (`state_impl.rs`, 1798 lines) exists purely to satisfy the budget — its own
header says it was "extracted **verbatim** from model.rs … the impls moved **as a group** so their
cross-block method calls stay co-located" — a mechanical split, not a semantic boundary. A cluster of
files sit pinned just under the cap. The line-budget gate is being satisfied by mechanical splits + a cap
relaxation rather than by reducing responsibility, so navigation cost stays high.

## Current state

- `file-size-budget.toml:31-40` — cap relaxed 1500→2000 for "three hot-path files whose decomposition
  reached a safe floor above 1500L"; ledger now empty.
- `crates/jackin-console/src/tui/screens/editor/model/state_impl.rs:1-4` — header admits it's a verbatim,
  budget-driven split (1798 lines).
- Files pinned just under cap: `components/save_preview.rs:1467`, `screens/workspaces/view.rs:1462`,
  `screens/workspaces/update.rs:1437`, `screens/settings/update.rs:1385`, `screens/settings/model.rs:1361`,
  `input/global_mounts.rs:1337`.

## Scope

**In scope:** `crates/jackin-console/src/tui/screens/editor/model/state_impl.rs` and the near-cap screen
files above, `file-size-budget.toml`. **Out of scope:** behavior changes (this is behavior-preserving);
the generics reshape (that's plan 022 and its next-numbered follow-up, if one is written — do that first).

## Steps

### Step 1: Split `state_impl.rs` along feature seams (not line count)

Break the editor `state_impl.rs` by **responsibility** — per-modal or per-tab impl groups (e.g. save-flow
impls, auth-form impls, mount-isolation impls) into separate self-named module files under
`screens/editor/model/`, following the no-`mod.rs` rule (`crates/AGENTS.md`). Tests stay in the sibling
`tests.rs`. Each new file should be a coherent unit, not a line-count slice.

**Verify after the split**: `cargo check -p jackin-console --all-targets` → exit 0;
`cargo nextest run -p jackin-console` → all pass (behavior preserved).

### Step 2: Decompose the near-cap screen files similarly

For each near-cap file (workspaces/settings view/update/model), extract cohesive sub-responsibilities into
sibling modules. Prioritize the ones whose size is closest to the cap. One file per commit, verifying tests
each time.

### Step 3: Re-tighten the cap toward 1500

As files drop below 1500, lower the `file-size-budget.toml` production cap back toward 1500 (or a
documented intermediate). Do **not** lower it below what the now-decomposed files actually hit — the goal is
the cap reflects real structure, not a relaxation papering over debt.

**Verify**: `cargo xtask lint files` (or the repo's file-size gate command — find it:
`grep -rn "file.size\|file_size_budget" crates/jackin-xtask/src`) → passes at the new cap.

## Done criteria

- [x] `state_impl.rs` is gone or reduced to a genuine semantic unit; no file exists solely to satisfy the budget
- [x] Near-cap screen files audited; remaining console production files are below the tightened cap without mechanical splits
- [x] `file-size-budget.toml` cap lowered toward 1500 with the ledger still empty
- [x] `cargo nextest run -p jackin-console` green (behavior preserved)
- [x] `cargo clippy -p jackin-console --all-targets -- -D warnings` exits 0
- [x] `PROJECT_STRUCTURE.md`/codebase-map updated for new module files (structural-change docs gate)
- [x] `plans/README.md` row updated

## Implementation result

- Split `crates/jackin-console/src/tui/screens/editor/model/state_impl.rs` into semantic child modules:
  `state_impl/pending.rs` (console trait adapters and pending worker polling),
  `state_impl/navigation.rs` (focus/navigation/modal lifecycle helpers), and
  `state_impl/workspace.rs` (workspace mutation, dirty/change accounting, mounts, roles, secrets, and auth
  state mutation). The root `state_impl.rs` is now a 9-line module shell.
- Audited the listed near-cap console files after the split: `components/save_preview.rs` (1464),
  `screens/workspaces/view.rs` (1462), `screens/workspaces/update.rs` (1437),
  `screens/settings/update.rs` (1385), `screens/settings/model.rs` (1361), and
  `input/global_mounts.rs` (1337). They are all under 1500 and were left intact to avoid new mechanical
  line-count splits.
- Tightened the repo-wide production cap from 2000 to 1850. A 1500 repo-wide cap is still blocked by
  non-console files (`jackin-runtime/src/runtime/image.rs`, `jackin-term/src/grid.rs`,
  `jackin-capsule/src/session.rs`), so 1850 is the strictest ledger-empty intermediate.

## STOP conditions

- Plan 022's generics decision isn't made yet — the decomposition will churn twice; do 022 first.
- A split can't be made behavior-preserving because cross-block method calls are genuinely entangled —
  report; that entanglement may be the real finding (the module is doing too many things and needs a design
  change, not a move).

## Maintenance notes

- The file-size cap is a *symptom* gauge; the real target is responsibility per module. A reviewer should
  reject future mechanical line-count splits and ask for semantic ones.
- Coordinate with plan 015 (crate split): decomposing modules first makes a later crate carve much cheaper.
