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
- **Planned at**: PR #713 after Plan 022 investigation

## Why this matters

Plan 022 measured the generic cost in `jackin-console`: 94 trait definitions, 19 `Console*` bridge traits,
28 single-impl traits, and 124 `EditorState<...>` spell-outs across 26 files. Production binds
`EditorState` through one concrete `crate::tui::state::EditorState<'a>` alias, and the first generic
parameter is always `jackin_config::WorkspaceConfig`. Keeping that parameter generic adds noise without
preserving a real production seam.

The throwaway spike removed only the `WorkspaceConfig` axis. It touched 9 files and removed roughly 60
lines before compile errors exposed adjacent generic aliases that need deliberate sequencing:

- `ConsoleManagerMessage` still expected a workspace-config parameter at its call sites.
- `WorkspaceSaveEffect` still required its fourth `WorkspaceConfig` generic.
- `tui/state/update.rs` needed matching alias arity changes.

That is a real win, but not a drive-by cleanup.

## Scope

**In scope:** concretize the `WorkspaceConfig` axis in `EditorState`, its view aliases, tests, and adjacent
message/effect aliases that only forward the same concrete type. **Out of scope:** collapsing `Modal`,
`SaveFlow`, `EnvValue`, `AuthFormTarget`, pending-subscription parameters, or deleting the broader
`Console*` bridge trait layer.

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

### Step 2: Update adjacent aliases deliberately

Do not remove generic arguments by broad text substitution. Update the aliases that still carry
`WorkspaceConfig` only as pass-through ceremony:

- `crates/jackin-console/src/tui/effect.rs`
- `crates/jackin-console/src/tui/message.rs`
- `crates/jackin-console/src/tui/state/update.rs`

Keep each alias compiling before moving to the next one.

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
- `cargo check -p jackin-console --all-targets`
- `cargo nextest run -p jackin-console`
- `cargo clippy -p jackin-console -- -D warnings`

## Done criteria

- [ ] `EditorState` no longer has a `WorkspaceConfig` type parameter
- [ ] Direct `EditorState<...>` spell-outs shrink by one argument where applicable
- [ ] Adjacent message/effect aliases compile with concrete workspace config ownership
- [ ] ADR recorded for the narrowed generics decision
- [ ] `plans/README.md` row updated

## STOP conditions

- More than the listed adjacent aliases need signature churn; stop and split the additional axis into a new
  plan instead of broadening this one.
- Test fixtures lose the ability to use lightweight modal/pending placeholder types; stop and preserve those
  seams.
