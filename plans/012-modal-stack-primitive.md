# Plan 012: One ModalStack primitive — settings tabs converge on a single modal enum, stash slots die

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-console/src/tui/ crates/jackin-tui/src/components/modal_lifecycle.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED–HIGH (largest plan in the set; heavy test surface)
- **Depends on**: plans/002-tui-component-contract.md (recommended)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: IN PROGRESS — current branch drift is the integrated #721 baseline; `ModalStack` now exists in `jackin-tui`, editor/settings lifecycle methods route through it, settings env plus editor picker context now lives on modal variants, and auth-form return terminology now names the modal parent stack. Remaining work is settings enum convergence.

## Why this matters

Sub-dialog stacking — "Esc walks back one step" — was implemented four ways: the editor's `ConsoleModal` enum with `modal_parents` stacks, THREE separate per-tab settings enums (`GlobalMountModal`, `SettingsEnvModal`, `SettingsAuthModal`) each redeclaring overlapping variants and each with its own render dispatch, the capsule's `Vec<Dialog>`, and legacy per-flow "stash" fields (`pending_picker_value`, `pending_env_key`, `pending_auth_form_return`, …) that the design docs flagged as "legacy … does not compose past two levels and is easy to forget." The current branch has moved picker/auth return context onto modal variants or modal parent-stack frames; the remaining structural duplication is the three settings modal enums. This plan introduces one `ModalStack<M>` primitive, converges the three settings enums into one, and eliminates the stash slots in the flows that use them.

## Current state

- Editor modal container: `crates/jackin-console/src/tui/model/modal.rs:20` — `pub enum ConsoleModal<...>` with **22 type parameters** (TextInputTarget…SecretsScopeTag; verified) and ~20 variants; `#[allow(clippy::large_enum_variant)]`. Stacking via `modal_parents` / `open_sub_modal` / `clear_modal_chain` — this machinery is referenced across ~19 files (`rg -l 'modal_parents|open_sub_modal|clear_modal_chain' crates/jackin-console/src` — run it; the list includes `state/manager.rs`, `input/editor/modal.rs`, `screens/editor/model/state_impl.rs`, `screens/settings/model/{env_impls,auth_impls}.rs`, `auth_config.rs`, `file_browser.rs`).
- Settings modal containers: `crates/jackin-console/src/tui/screens/settings/view.rs` — three per-tab enums, each with its own generic render fn:
  - `render_global_mount_modal` (`:567`) over `GlobalMountModal<TextInputState, FileBrowserState, MountDstChoiceState, ScopePickerState, RolePickerState<R>, ConfirmState, …>`
  - `render_settings_env_modal` (`:609`) over `SettingsEnvModal<TextInputState, SourcePickerState, O, RolePickerState<R>, …>`
  - `render_settings_auth_modal` (`:648`) over `SettingsAuthModal<TextInputState, SourcePickerState, O, FileBrowserState, …>`
  All three wrap the same shared widgets the editor's `ConsoleModal` wraps.
- Stash-slot legacy before this branch's follow-up slices: e.g. `crates/jackin-console/src/tui/input/editor/modal.rs:290-296` at plan time:
  ```rust
  if let Some(stashed) = editor.pending_picker_value.take() {
      set_pending_env_value_typed(editor, scope, &key, stashed);
      editor.clear_modal_chain();
      return;
  }
  editor.open_sub_modal(Modal::SourcePicker { state: ..., env_key: Some((scope.clone(), key)) });
  ```
  `rg -c 'pending_' crates/jackin-console/src/tui/input/editor.rs` → 13 hits; also `input/global_mounts.rs` (14), `input/auth.rs` (5), others.
- The capsule `Vec<Dialog>` stack is fine as a mechanism (single enum + vec) — it is NOT migrated here; the primitive should merely be compatible with its semantics.
- Shared crate has `modal_lifecycle.rs` (1.7K, small) — read it first; if it already models open/close lifecycle, `ModalStack` belongs beside (or inside) it.

Docs canon (`dialogs.mdx` §"Sub-dialogs push onto a stack — Esc walks back one step"): names `ModalStack`, modal-variant context, and modal parent-stack return as the current mechanisms, and warns the bare `Option<Modal>` slot "regresses this rule by construction". `components.mdx` wizard rules: multi-step flow = chain of reusable components on a stack.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Console tests | `cargo nextest run -p jackin-console` | pass (large suite — the Esc-back tests are the safety net) |
| Full | `cargo nextest run` | pass |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/modal_lifecycle.rs` (or a sibling `modal_stack.rs`) — new `ModalStack<M>` with `open`, `open_sub` (push), `pop` (Esc-back one), `clear_chain` (terminal commit), `current`, `current_mut`, `parents`
- `crates/jackin-console/src/tui/screens/settings/` — one `SettingsModal` enum replacing the three per-tab enums; one render dispatch fn; per-tab `.modal` fields replaced by one stack per settings screen (or one per tab if focus isolation requires — decide by reading how tab switching interacts with open modals in `screens/settings/model.rs`)
- `crates/jackin-console/src/tui/model/modal.rs` + `state/manager.rs` etc. — editor's `modal_parents` machinery re-expressed on `ModalStack<ConsoleModal<…>>` (mechanical: the semantics already match push/pop/clear)
- Stash-slot elimination in `input/editor/modal.rs`, `input/editor.rs`, `input/global_mounts*.rs`, `input/auth.rs` — each `pending_*` becomes data carried in the modal variant that needs it (the `SourcePicker { env_key: Some(...) }` pattern above is the model: context rides IN the variant, not beside the stack)
- `docs/content/docs/reference/tui/dialogs.mdx` §Sub-dialogs — rewrite to name `ModalStack` as the one mechanism

**Out of scope**:
- Capsule `Vec<Dialog>` (compatible already; migrating it is optional follow-up).
- `crates/jackin/src/console/tui.rs` root facade — unless it holds the bare `Option<Modal>` slot the docs warn about; if it does (grep `Option<.*Modal`), migrating that slot IS in scope (it is the documented regression).
- Modal *contents* (which widgets render) — container only.
- Modal sizing (plan 013).

## Git workflow

Branch (operator confirm): `refactor/tui-modal-stack`. `git commit -s` + push per commit — this plan especially: commit per step, keep the tree green between steps. Update `dialogs.mdx` in the same PR.

## Steps

### Step 1: Build `ModalStack<M>` with the editor's semantics

Read `modal_lifecycle.rs` and the editor's `open_sub_modal`/`clear_modal_chain`/`modal_parents` implementations (`rg -n 'fn open_sub_modal|fn clear_modal_chain|modal_parents' crates/jackin-console/src` — the defining impls are in the editor/settings model files, e.g. `screens/editor/model/state_impl.rs`). Implement `ModalStack<M>` in `jackin-tui` with EXACTLY those semantics (push parent when opening sub; pop restores parent; clear-chain drops all). Unit-test the primitive in isolation.

**Verify**: `cargo nextest run -p jackin-tui` → pass with new stack tests.

### Step 2: Editor machinery onto the primitive

Replace the editor's hand-rolled parent-stack fields/methods with `ModalStack<ConsoleModal<…>>`, keeping `open_sub_modal`/`clear_modal_chain` as thin delegating methods so the ~19 call-sites don't churn. Behavior-neutral.

**Verify**: `cargo nextest run -p jackin-console` → pass, zero test-expectation changes (if an Esc-back test changes expectation, semantics drifted — STOP).

### Step 3: One settings modal enum

Define `SettingsModal` (one enum covering the union of the three per-tab enums' variants — Text input, SourcePicker, OpPicker, RolePicker, ScopePicker, FileBrowser, MountDstChoice, Confirm, PreviewSave/ConfirmSave, …), one `render_settings_modal` dispatch, and a `ModalStack<SettingsModal>`. Migrate tab by tab (Mounts → Env → Auth), each tab a separate commit with tests green. The giant-type-parameter pattern of `ConsoleModal` is the existing precedent — follow it (or, if the settings enum can name concrete types directly because settings isn't generic over targets the way the editor is, prefer concrete types; read how the three current enums are instantiated at `view.rs:567/609/648` — they already name mostly concrete states).

**Verify** (after each tab): `cargo nextest run -p jackin-console` → pass; after all three: `rg 'GlobalMountModal|SettingsEnvModal|SettingsAuthModal' crates/` → 0.

### Step 4: Kill the stash slots

For each `pending_*` field (enumerate: `rg -n 'pending_[a-z_]+\s*:' crates/jackin-console/src/tui` for declarations): move the carried context into the modal variant(s) of the flow that uses it, following the `SourcePicker { env_key }` in-variant pattern. Delete the field. One commit per field/flow.

**Verify**: `rg -n 'pending_picker_value|pending_env_key|pending_auth_form_return' crates/` → 0 (other `pending_*` fields may be non-modal state — only remove the modal-flow stashes; list any you deliberately keep in the PR body); `cargo nextest run -p jackin-console` → pass.

### Step 5: Docs

Rewrite `dialogs.mdx` §"Sub-dialogs push onto a stack": one mechanism (`ModalStack`), stash form removed, capsule `Vec<Dialog>` noted as the same semantics pending optional migration.

**Verify**: `cd docs && bun run build` → exit 0.

## Test plan

- New: `ModalStack` unit tests (push/pop/clear/current, sub-of-sub Esc-back walks one level, clear-chain from depth 3).
- Existing Esc-back and modal-flow tests in `jackin-console` are the regression net — they must pass UNCHANGED through Steps 2–3.
- New per Step 4: for each de-stashed flow, a test that the multi-step flow (e.g. env add: key input → source picker → value/op-picker → commit) completes and Esc at each depth returns exactly one level — model on existing flow tests in `input/global_mounts/tests.rs` / `input/auth/tests.rs`.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'GlobalMountModal|SettingsEnvModal|SettingsAuthModal' crates/` → 0
- [ ] `rg 'pending_picker_value|pending_env_key|pending_auth_form_return' crates/` → 0
- [ ] One `ModalStack` type in `jackin-tui`, used by editor + settings
- [ ] `dialogs.mdx` sub-dialog section names one mechanism
- [ ] `plans/README.md` updated

## STOP conditions

- Step 2 changes any Esc-back test expectation.
- The three settings enums turn out to carry per-tab state that cannot merge without cross-tab type parameters exploding beyond `ConsoleModal`'s 22 (i.e. the union enum is WORSE) — report with the variant table; the fallback design (three enums, one shared stack type) needs operator sign-off.
- A `pending_*` field is load-bearing across screens (not just across modal steps) — leave it, list it, move on.
- Step count/test churn exceeds ~2× the estimate — checkpoint with the operator rather than pushing through.

## Maintenance notes

- All future wizard flows (the docs' Claude-token flow among them) build on `ModalStack` — reviewers reject new stash fields or per-tab modal enums.
- Plan 013 (sizing registry) keys off modal kinds; the unified `SettingsModal` makes its registry table smaller — land 012 first.
- Deferred: capsule `Vec<Dialog>` migration onto `ModalStack` (same semantics; value is uniformity, not behavior).
