# Plan 046: Collapse `EditorState`'s concrete `WorkspaceConfig` parameter

> **Executor instructions**: Follow-up from Plan 022. Implement this before Plan 023. Keep the change
> monotonic: one concrete parameter only, no broad console decomposition in the same commit. Update
> `plans/README.md` and record the final decision as an ADR.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: 022
- **Category**: tech-debt
- **Completed at**: PR #713
- **Planned at**: PR #713 after Plan 022 investigation

## Why this matters

Plan 022 measured the generic cost in `jackin-console`: 94 trait definitions, 19 `Console*` bridge traits,
28 single-impl traits, and 124 `EditorState<...>` spell-outs across 26 files. Production binds
`EditorState` through one concrete `crate::tui::state::EditorState<'a>` alias, and the first generic
parameter is always `jackin_config::WorkspaceConfig`. Keeping that parameter generic adds noise without
preserving a real production seam.

The throwaway spike removed only the `WorkspaceConfig` axis. It touched 9 files and removed roughly 60
lines before compile errors exposed adjacent generic aliases that need deliberate sequencing:

- `ConsoleManagerMessage` and `WorkspaceSaveEffect` appeared to need changes in the spike because the
  throwaway rewrite removed standalone `WorkspaceConfig` generic lines too broadly. The final implementation
  did not need to touch those aliases.
- `tui/state/update.rs` compiled unchanged once only direct `EditorState<...>` spell-outs were rewritten.

That is a real win, but not a drive-by cleanup.

## Scope

**In scope:** concretize the `WorkspaceConfig` axis in `EditorState`, its view aliases, and tests. **Out of
scope:** collapsing `Modal`, `SaveFlow`, `EnvValue`, `AuthFormTarget`, pending-subscription parameters, or
deleting the broader `Console*` bridge trait layer.

## Steps

### Step 1: Concretize `EditorState`'s first parameter

In `crates/jackin-console/src/tui/screens/editor/model.rs`, remove `WorkspaceConfig` from
`EditorState<...>`'s generic parameter list and keep the `original`/`pending` fields as concrete
`jackin_config::WorkspaceConfig`.

Expected direct files:

- `crates/jackin-console/src/tui/screens/editor/model.rs`
- `crates/jackin-console/src/tui/screens/editor/model/state_impl.rs`
- `crates/jackin-console/src/tui/screens/editor/model/tests.rs`
- `crates/jackin-console/src/tui/screens/editor/view.rs`
- `crates/jackin-console/src/tui/components/save_preview.rs`
- `crates/jackin-console/src/tui/components/save_preview/tests.rs`
- `crates/jackin-console/src/tui/state.rs`

### Step 2: Avoid adjacent alias churn

Do not remove generic arguments by broad text substitution. The final implementation should not need to
change `ConsoleManagerMessage`, `WorkspaceSaveEffect`, or `tui/state/update.rs`; if it does, stop and
re-check the rewrite scope.

### Step 3: Preserve test-fixture seams

The remaining ten `EditorState` parameters still buy cheap isolated tests: model/view tests use `()` or
small test modal types for modal, cache, save-flow, auth-target, and pending-operation slots. Leave those
alone unless a separate measured plan proves they are pure ceremony.

### Step 4: Record the decision

Add the next ADR under `docs/content/docs/reference/adrs/` explaining the narrowed decision: `EditorState`
uses a concrete workspace config but keeps the other parameters until separately measured. Update
`docs/content/docs/reference/adrs/meta.json` and the ADR index.

## Verification

- `cargo fmt --check`
- `cargo check -p jackin-console`
- `cargo check -p jackin-console --all-targets --features test-support`
- `cargo nextest run -p jackin-console --features test-support`
- `cargo clippy -p jackin-console --features test-support -- -D warnings`

## Done criteria

- [x] `EditorState` no longer has a `WorkspaceConfig` type parameter
- [x] Direct `EditorState<...>` spell-outs shrink by one argument where applicable
- [x] Adjacent message/effect aliases compile unchanged after the precise rewrite
- [x] ADR recorded for the narrowed generics decision
- [x] `plans/README.md` row updated

## STOP conditions

- More than the listed adjacent aliases need signature churn; stop and split the additional axis into a new
  plan instead of broadening this one.
- Test fixtures lose the ability to use lightweight modal/pending placeholder types; stop and preserve those
  seams.
