# Plan 015: Split the mega-crates to parallelize rustc and shrink incremental rebuilds (spike + first carve)

> **Executor instructions**: Large structural change. This plan is a **spike-then-carve**: measure, then
> do ONE crate split behind a façade, verify, and write follow-up plans for the rest. Do **not** attempt
> all three mega-crates in one pass. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-console crates/jackin-capsule crates/jackin-runtime Cargo.toml`

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED
- **Depends on**: none (but coordinate with 022/023 which also reshape jackin-console)
- **Category**: perf / tech-debt
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`jackin-console` (86.5K lines), `jackin-capsule` (49.5K), and `jackin-runtime` (36.8K) are each a single
compilation unit. Rust's front-end (macro expansion, type/borrow check) is per-crate and largely serial,
and downstream crates block on it. Any edit inside `jackin-console` re-typechecks all 86K lines and
re-links; the giant `tests.rs` files (launch `tests.rs` 8.6K lines, daemon `tests.rs` 7.6K) recompile
wholesale on the CI clippy `--all-targets` gate. This is the structural reason edit→check feedback is
slow. Splitting along existing seams lets Cargo compile pieces in parallel and stops unrelated edits from
re-checking the whole surface.

## Current state

- Crate sizes (incl. tests): `jackin-console` 86,498; `jackin-capsule` 49,546; `jackin-runtime` 36,833.
- `jackin-console` seams visible in the tree: `tui/screens/{workspaces,settings,editor}`, `tui/op_picker`,
  `tui/layout`, `input/`, `components/`.
- `jackin-capsule` seams: `daemon`, `tui`, `session`, `exec`.
- `Cargo.toml:132-134` — `[profile.dev]` only sets `debug = 1`; no structural mitigation possible while
  crates stay monolithic.
- NOTE: a CI-pipeline speedup backlog already exists as a roadmap doc — this plan is the **crate-boundary**
  lever (rustc parallelism), a different thing; verify against `docs/content/docs/reference/roadmap/`
  before acting so you don't duplicate a tracked item.

## Steps

### Step 1 (GATE): measure the current rebuild cost

Run `cargo build --timings` after a one-line edit inside each mega-crate and record the self+downstream
recompile time in this plan's row note. This is the baseline the split must beat. If the timings show the
mega-crates are **not** the bottleneck (e.g. a proc-macro or a different crate dominates), STOP and
re-scope.

### Step 2: Carve ONE leaf out of `jackin-console` behind a façade

Pick the most self-contained seam (recommend `tui/op_picker` — it already has a clear boundary and its own
tests). Move it to a new crate `crates/jackin-console-oppicker` (or the repo's naming preference), have
`jackin-console` depend on it and re-export through a thin façade so call sites don't churn. Honor
`crates/AGENTS.md`: self-named module files (no `mod.rs`), tests in sibling `tests.rs`, `[lints] workspace = true`,
no per-crate lint/edition copies.

**Verify**: `cargo check --workspace --all-targets` → exit 0;
`cargo nextest run -p jackin-console -p jackin-console-oppicker` → all pass;
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0 (the CI gate).

### Step 3: Re-measure and write follow-ups

Re-run `cargo build --timings` for a one-line edit in `jackin-console`; record the delta. Write follow-up
plans (`015a`, `015b`, …) for the remaining carves (console screens; capsule daemon/tui/session), each
scoped to one seam, only if Step 2's measured win justifies continuing.

## Done criteria

- [ ] Baseline + post-split `cargo build --timings` numbers recorded in the row note
- [ ] One leaf crate extracted; workspace builds, all tests pass, CI clippy gate passes
- [ ] New crate follows `crates/AGENTS.md` layout (no `mod.rs`; tests in `tests.rs`; `[lints] workspace = true`)
- [ ] `cargo shear` passes (no unused/misplaced deps introduced) — `cargo shear` if available via mise
- [ ] Follow-up carve plans written (or a note that the measured win didn't justify them)
- [ ] `plans/README.md` row updated
- [ ] `PROJECT_STRUCTURE.md` + codebase-map doc updated for the new crate (repo's structural-change docs gate)

## STOP conditions

- Step 1 shows the mega-crates aren't the compile bottleneck → re-scope, don't split for no gain.
- The chosen seam has pervasive `pub(crate)` coupling that makes extraction touch hundreds of call sites —
  pick a smaller seam or report that a pre-refactor (022/023) is needed first.
- Extraction forces a dependency cycle (console-leaf needs something only in console) — report the cycle.

## Maintenance notes

- Coordinate with plans 022 (console generics) and 023 (console decomposition): if the generics are
  collapsed first, the crate split gets much easier. Sequencing 022/023 before further console carves may
  be wise — note the decision.
- Each new crate must inherit workspace lints; a reviewer should confirm no `edition`/`rust-version`/lint
  tables were copied per-crate.
