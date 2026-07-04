# Plan 054: Extract the remaining op-picker state/input boundary

> **Executor instructions**: Follow-up to Plans 015 and 045. Stay in the active PR/branch. Do not start this
> until Plan 015's `jackin-console-oppicker` crate is green on CI.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: 015, 045
- **Category**: perf / tech-debt
- **Planned at**: 2026-07-04, after the first op-picker model crate carve
- **Completed at**: current PR branch, 2026-07-04

## Why this matters

Plan 015 extracted the pure 1Password picker model/planning API into `crates/jackin-console-oppicker`,
but the rest of the op-picker surface still lives in `jackin-console`. That was intentional: the state,
input, and load adapters still depend on console-local list helpers, render traits, and animation tick
vocabulary. Moving them in the first carve would have widened the blast radius beyond Plan 015's
"one leaf behind a facade" rule.

## Steps

1. Read `crates/jackin-console/src/tui/op_picker/{state,input,load}.rs`,
   `crates/jackin-console/src/tui/components/op_picker.rs`, and the current
   `crates/jackin-console-oppicker/src/lib.rs` API.
2. Replace console-local helper dependencies with lower-tier abstractions only when the abstraction is
   already useful outside this move. Do not move unrelated list/rendering modules just to satisfy the
   extraction.
3. Move exactly one additional boundary into `jackin-console-oppicker`, preferring state/input planning
   before service execution. Keep `jackin-console` facade modules so call sites churn minimally.
4. Re-run the same Plan 015 timing/check suite and record whether the larger split reduces warm
   `jackin-console` rebuild time enough to justify another carve.

## Done criteria

- [x] One additional op-picker boundary is owned by `jackin-console-oppicker`.
- [x] No dependency cycle from `jackin-console-oppicker` back into `jackin-console`.
- [x] `jackin-console` keeps thin facade modules for migrated APIs.
- [x] `cargo check --workspace --all-targets --all-features --locked` passes.
- [x] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passes.
- [x] Focused op-picker tests pass.
- [x] `PROJECT_STRUCTURE.md`, codebase map, and `plans/README.md` are updated.

## Result

Moved the remaining op-picker state/input boundary into
`crates/jackin-console-oppicker`: concrete picker state, constructors,
key-handling methods, pending-load request creation, load-result polling, and
animation tick state now live in the leaf crate. `jackin-console` keeps facade
modules for `model`, `state`, and `input`; its `load` module owns only external
`op` runner execution and the console-local animation trait binding.

The leaf crate has no dependency on `jackin-console`; it depends only on lower
tiers (`jackin-core`, `jackin-tui`, `jackin-diagnostics`, `crossterm`, and
`tui-widget-list`).

Warm timing after the move:

- Touching `crates/jackin-console/src/tui/op_picker/state.rs` rebuilt
  `jackin-console` in 1.41s cargo / 1.45s wall.
- Touching `crates/jackin-console-oppicker/src/state.rs` rebuilt
  `jackin-console-oppicker` plus `jackin-console` in 1.39s cargo / 1.44s wall.

Plan 015's post-first-carve console facade timing was 6.64s cargo / 6.70s wall,
so this wider state/input carve materially improves the warm edit-check loop.

## STOP conditions

- The move requires pulling broad console rendering/list infrastructure into the leaf crate.
- The abstraction needed to break the dependency would be used only by this migration and adds more API
  surface than it removes.
- Timings show no meaningful incremental rebuild improvement after the first Plan 015 carve.
