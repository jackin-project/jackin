# Plan 045: Prepare op-picker for a safe leaf-crate extraction

> **Executor instructions**: Pre-refactor for Plan 015. Keep this scoped to dependency untangling; do not
> create the new crate in this plan. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat HEAD -- crates/jackin-console/src/tui/op_picker* crates/jackin-console/src/tui/components/op_picker*`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 015
- **Category**: perf / tech-debt
- **Planned at**: commit `166c51b52`, 2026-07-04
- **Completed at**: current PR branch, 2026-07-04
- **Result**: DONE — pure op-picker model/planning now lives under `tui::op_picker::model`; render-only component code is a facade/adapter.

## Why this matters

Plan 015 measured real compile cost in the mega-crates, but the recommended `tui/op_picker` carve is not
a leaf today. The picker state/input/load modules depend on `crate::tui::components::op_picker`, and that
component depends on console-local list helpers and render modules. Moving `op_picker` directly would
either introduce a dependency cycle or move too much surface in the first extraction.

## Steps

1. Move generic picker model/planning helpers out of `tui/components/op_picker` dependencies on
   `crate::tui::components::list_helpers` by passing the small filtering/selection helpers in or by
   relocating those helpers to a lower shared crate.
2. Split render-only code from model/planning code so `OpPickerState` can depend on model/planning types
   without depending on the full console render component.
3. Keep `jackin-console::tui::op_picker` as the public facade and avoid call-site churn.
4. Verify that `cargo check -p jackin-console --all-targets` and
   `cargo test -p jackin-console --features test-support op_picker` pass.

## Done Criteria

- [x] `tui/op_picker/{state,input,load}` no longer imports from render-only op-picker code.
- [x] Any shared list/filter helper moved to a lower crate or injected through small pure functions.
- [x] No dependency cycle would be required for a future `jackin-console-oppicker` crate.
- [x] `cargo clippy -p jackin-console --features test-support -- -D warnings` exits 0.
- [x] `plans/README.md` row updated.

## STOP Conditions

- Untangling requires broad changes to workspace/settings/editor screens. In that case, defer until
  Plans 022 and 023 reduce console state coupling.
