# Plan 013: One modal-sizing registry — promote modal_rects into jackin-tui, route capsule + launch through it

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-console/src/tui/components/modal_rects.rs crates/jackin-capsule/src/tui/components/dialog.rs crates/jackin-launch-tui/src/tui/ crates/jackin-tui/src/geometry.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (sizing regressions are visually load-bearing)
- **Depends on**: plans/012-modal-stack-primitive.md (recommended — unified modal kinds shrink the registry)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: BLOCKED — drift check found existing launch TUI failure-dialog and run-loop changes before plan work.

## Why this matters

"How big is this modal and where does it sit above the reserved footer" is computed by three independent registries: the console's `modal_rects.rs` (a real mode→rect registry), the capsule's per-variant `box_rect` arithmetic, and launch's per-dialog `centered_rect` calls. The docs' Modal Sizing Rules (stable preferred size, never draw over the status bar, symmetric padding) must therefore be re-verified in three places on every change, and the shared crate itself concedes the migration is pending: "Console modal rects intentionally keep their fixed 160-column reference sizing until that modal layer is migrated" (`text_input.rs:487`). One registry in `jackin-tui`, three consumers.

## Current state

- Console registry (the best-shaped of the three — promote it): `crates/jackin-console/src/tui/components/modal_rects.rs` — `ModalRectMode` enum (`:120`), `ModalRectSpec`, `modal_rect_for_mode(outer, mode)` (`:190`), `modal_rect(outer, spec)` (`:195`), plus named wrappers `text_input_rect` (`:217`), `source_picker_rect` (`:223`), `scope_picker_rect` (`:229`), `op_picker_rect` (`:235`), `role_picker_rect_for_count` (`:241`), `confirm_rect(outer, &ConfirmState)` (`:248`), `mount_choice_rect` (`:258`), `auth_form_rect_for_height` (`:272`).
- Capsule: `crates/jackin-capsule/src/tui/components/dialog.rs:1272` — `box_rect(&self, term_rows, term_cols) -> (u16,u16,u16,u16)` with per-variant width arithmetic (e.g. `CONTAINER_INFO_WIDTH.min(cols-4).max(PALETTE_WIDTH)`; a comment noting the exit data-loss confirm uses "the shared Details width percentage (70%)"), including status-bar offset handling.
- Launch: `centered_rect` calls per dialog — `container_info_dialog.rs:94`, `run.rs:667,712`, plus `failure_popup_rect_for_rows` (moves under plan 008).
- Shared geometry primitive already exists: `crates/jackin-tui/src/geometry.rs:52` `pub fn centered_rect(width, height, area)` (re-exported from `lib.rs:45`); `text_input.rs:489` `text_input_prompt_rect` — a shared prompt-geometry fn whose doc carries the pending-migration note.

Docs canon (`dialogs.mdx`): §"Modal Sizing Rules" — stable preferred dialog size (resize does not rescale), status bar always reserved (modals never draw over it), symmetric vertical padding.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui -p jackin-console -p jackin-capsule -p jackin-launch-tui` then full | pass |

## Scope

**In scope**:
- New `crates/jackin-tui/src/components/modal_rects.rs` (moved + generalized from the console; console file becomes a thin re-export or is deleted with imports updated)
- `crates/jackin-capsule/src/tui/components/dialog.rs` `box_rect` — re-express each variant's size via the shared registry (`ModalRectSpec` values), keeping the capsule's terminal-degradation guards (the "pathologically small" clamps in its doc comment) as capsule-side post-clamps if the registry can't express them
- `crates/jackin-launch-tui/src/tui/components/container_info_dialog.rs`, `run.rs:667,712` — route through registry specs
- `docs/content/docs/reference/tui/dialogs.mdx` §Modal Sizing Rules — name the registry

**Out of scope**:
- The 160-column reference-sizing *policy* (keep values byte-identical; changing preferred sizes is a design decision, not this plan)
- `failure_popup` sizing if plan 008 landed (it sizes via the shared error popup)
- `bottom_chrome`/footer reservation logic itself (the registry consumes the reserved area, it doesn't own it)

## Git workflow

Branch (operator confirm): `refactor/tui-modal-rect-registry`. `git commit -s` + push. dialogs.mdx same PR.

## Steps

### Step 1: Move the registry

Move `modal_rects.rs` to `crates/jackin-tui/src/components/modal_rects.rs` verbatim minus console-only imports (`confirm_rect` already takes shared `ConfirmState`; `role_picker_rect_for_count`/`auth_form_rect_for_height` take counts/heights — all portable). Leave a `pub use jackin_tui::components::modal_rects::*;` shim at the old console path OR update all console imports (`rg -l 'modal_rects' crates/jackin-console/src`) — prefer updating imports (repo has zero-`mod.rs`, no-shims hygiene; PRERELEASE.md allows breaking moves).

**Verify**: `cargo nextest run -p jackin-console -p jackin-tui` → pass, zero expectation changes.

### Step 2: Capsule `box_rect` onto specs

For each `Dialog` variant in `box_rect` (`dialog.rs:1272+`), express its width/height as a `ModalRectSpec` (add spec variants to the registry where the capsule needs one the console lacks — e.g. fixed-width `CONTAINER_INFO_WIDTH`, palette width). The capsule keeps: its `(row,col,height,width)` tuple shape (convert from `Rect` at the edge), status-bar offset, and small-terminal recoverability clamps. Byte-identical output required — write a conversion test FIRST (see Test plan), then refactor.

**Verify**: the new equivalence tests pass; `cargo nextest run -p jackin-capsule` → zero snapshot changes.

### Step 3: Launch dialogs onto specs

`container_info_dialog.rs:94` and `run.rs:667,712`: replace ad-hoc width/height math with the corresponding registry spec (add specs as needed). Byte-identical.

**Verify**: `cargo nextest run -p jackin-launch-tui` → zero expectation changes.

### Step 4: Docs

`dialogs.mdx` §Modal Sizing Rules: add "sizes come from the `modal_rects` registry in `jackin-tui`; a dialog computing its own rect is a violation." Remove/adjust the stale pending-migration note in `text_input.rs:487`'s doc comment.

**Verify**: `cd docs && bun run build` → 0; `rg 'until that modal layer is migrated' crates/` → 0.

## Test plan

- **Equivalence-first**: before refactoring each surface, capture current outputs — a table-driven test enumerating (terminal size × dialog kind) → expected rect using TODAY'S code paths, committed first; the refactor must keep it green. Sizes to cover: 80×24, 120×40, 160×50, pathological 20×6.
- Registry unit tests: every `ModalRectSpec` respects the reserved footer rows and symmetric padding invariants.

## Done criteria

- [ ] fmt / clippy / full `cargo nextest run` exit 0
- [ ] `modal_rects` lives in `jackin-tui`; `rg 'modal_rect' crates/jackin-console/src/tui/components/` → only imports
- [ ] Capsule `box_rect` contains no per-variant width arithmetic (specs only + edge conversion + clamps)
- [ ] `rg 'centered_rect' crates/jackin-launch-tui/src` → only via registry (or zero)
- [ ] Equivalence tests green (zero rect changes at all covered sizes)
- [ ] dialogs.mdx updated; `plans/README.md` updated
- [ ] Codebase Map (`docs/.../codebase-map.mdx`) updated for the moved module (repo rule: module moves update the map same PR)

## STOP conditions

- Any equivalence test disagrees between old and new paths at any covered size — report the (size, dialog) pair; do not "fix" by adjusting the expectation.
- Capsule's `(row,col,height,width)` + status-bar offset semantics can't round-trip through `Rect` without off-by-one — report with the exact case.
- The registry needs > ~6 new spec variants for the capsule — signals the spec model is wrong for it; report instead of forcing.

## Maintenance notes

- New dialogs must take a registry spec; reviewers reject inline `centered_rect` math in dialog code.
- The 160-column reference-sizing policy is now changeable in one file — if the operator ever revisits it, that is a one-PR design change.
- Deferred: `text_input_prompt_rect` folding into the registry (it is shared already; unify when its callers churn).
