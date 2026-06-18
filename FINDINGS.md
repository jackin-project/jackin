# Console Folder Extraction Findings

Date: 2026-06-17
Branch inspected: `fix/docker-startup-tui-error`
Primary folder inspected: `crates/jackin/src/console`
Target crate under review: `crates/jackin-console`

## Executive Summary

`crates/jackin/src/console` is not a small entrypoint shim today. It is the largest remaining part of the host console implementation: 80 Rust files and 34,666 lines, versus 143 Rust files and 50,056 lines in `crates/jackin-console/src`.

The current repository documentation explicitly calls this split an unfinished extraction. `docs/content/docs/reference/getting-oriented/codebase-map.mdx` says the crate split is "Phase 1, not finished" and that future work should move reusable, root-independent console domain/service/effect pieces into `jackin-console` or lower-tier crates when the dependency direction stays acyclic.

So the surprise is valid: a lot of what reads as jackin' console product code still lives in the binary crate. However, not all of it can move directly into `jackin-console` as-is. Much of it depends on root-only services from the binary crate: `crate::instance`, `crate::runtime`, `crate::paths`, `crate::docker`, and `crate::app`. Production console code now uses lower-crate owners directly for config, workspace-resolution, operator-env, agent, and role-selector data where those types are already lower-crate owned.

The clean end state is realistic, but it needs a staged extraction:

1. Move root-independent model/update/view helpers from `crates/jackin/src/console/tui` into `crates/jackin-console`.
2. Move product types currently trapped in the binary crate into lower crates where appropriate.
3. Keep only a thin binary-owned adapter for command dispatch, Docker/runtime execution, terminal ownership, and post-console launch/attach handling.

## Current Ownership Model

The workspace currently uses this layering:

- `jackin-console` is Tier 4: generic console UI plus effects-as-data. It depends on `jackin-core`, `jackin-config`, `jackin-tui`, and `jackin-diagnostics`.
- `jackin` is Tier 5: the binary crate. It depends on all lower crates and owns CLI dispatch, runtime wiring, Docker command runners, host paths, and the final launch/attach actions.

That means `jackin-console` cannot import anything from `crates/jackin/src/*`. Moving code from root `console/` into `jackin-console` requires one of these changes:

- make the code generic over root-owned types,
- move the root-owned types into lower crates,
- or keep that code in the binary adapter.

## Folder Inventory: `crates/jackin/src/console`

Approximate local inventory:

| Area | Files | Lines | Current role |
|---|---:|---:|---|
| `crates/jackin/src/console` total | 80 | 34,666 | Remaining root console implementation |
| `domain.rs` | 1 | 157 | Role-source logging, provider derivation, and root instance snapshot alias |
| `services.rs` + `services/` | 9 | 850 | Side-effect adapters around config, Docker, runtime, op, token setup |
| `effects.rs` | 1 | 1,226 | Root effect executor and background polling |
| `terminal.rs` | 1 | 50 | Host terminal ownership adapter |
| `tui/` | 65 | 31,920 | Remaining TUI state, input, update, rendering adapters, run loop, tests |

Largest root files:

- `crates/jackin/src/console/effects.rs`: root effect execution and async/background work.
- `crates/jackin/src/console/tui/state.rs`: concrete manager state, modals, pending workers, root type aliases.
- `crates/jackin/src/console/tui/message.rs`: root manager message alias and reducer.
- `crates/jackin/src/console/tui/run.rs`: terminal event loop, startup handling, prompt handoff, effect draining.
- `crates/jackin/src/console/tui/input/*`: keyboard/mouse handling for list, editor, settings, auth, save, global mounts.
- `crates/jackin/src/console/tui/components/*` and `view/*`: root display adapters over `jackin-console` and `jackin-tui` primitives.

## Current `jackin-console` Contents

`crates/jackin-console` already contains substantial console logic:

- generic app/stage/modal vocabulary in `tui/app.rs`,
- generic message/effect/update carriers in `tui/message.rs`, `tui/effect.rs`, and `tui/update.rs`,
- terminal lifecycle helpers in `tui/terminal.rs`,
- generic run-loop helper policies in `tui/run.rs`,
- screen modules under `tui/screens/{workspaces,editor,settings}`,
- reusable components under `tui/components/*`,
- op-picker state/input/render planning,
- mount display, mount diff, mount info, GitHub mount resolution, workspace helpers,
- service helpers for browser opening, file browser, mount info, config save, workspace helpers.

This confirms `jackin-console` is already the intended home for reusable console logic. The current split is not "all console in root"; it is a partial extraction with many root adapters still remaining.

Progress since this findings pass: save-preview rows, auth/environment diffing, workspace/settings preview snapshot construction, save-preview line builders, collapse row construction, and their tests have moved into `crates/jackin-console/src/tui/components/save_preview.rs`. The root save-preview module has been removed. Production console code now also imports `AppConfig`, `RoleSource`, `GlobalMountRow`, workspace-resolution helpers, `EnvValue`, `OpRef`, `OpCache`, token-setup types, and role-picker state from `jackin-config`, `jackin-core`, `jackin-env`, and `jackin-console` instead of the root shims. Pure workspace-choice and launch-dispatch resolution now lives in `crates/jackin-console/src/services/launch.rs`; root `domain.rs` no longer owns those rules. The generic instance-refresh snapshot carrier, instance-refresh throttle/generation policy, and forced-refresh generation policy now live in `jackin-console/src/tui/subscriptions.rs`; root supplies the concrete instance/session/snapshot types and worker receiver. Agent-picker prompt gating now lives in `jackin-console/src/tui/message.rs`; root opens the concrete picker state. Top-level input dispatch precedence, non-modal manager-stage route classification, create-prelude completion status, create-prelude top-level key routing, create-prelude file-browser outcome routing, create-prelude workdir-cancel routing, create-prelude mount-destination choice routing, create-prelude destination text-input routing, create-prelude workdir-picker routing, and create-prelude workspace-name text-input routing, create-prelude first-mount fact construction, create-prelude pending-mount workdir-picker transition, create-prelude mount-destination rewind now live in `jackin-console/src/tui/app.rs`; root dispatches concrete handlers and payloads. More root-console tests now use owner crates for agent, selector, config/workspace, mount isolation, and op data types instead of root shim paths. Console effect aliases and isolation service calls now use the owner `jackin-runtime` isolation types directly. Diagnostics screen mapping, plain-main-list quit policy, quit-confirm outcome policy, global clickability policy, modal mouse-consumption policy, debug-chip activation, pointer hand policy, and debug-chip chrome hover state now live in `jackin-console/src/tui/run.rs`; root maps concrete stages into lower-crate facts. Workspace-list hover target state and hover row extraction now live with the workspace screen model in `jackin-console`. List-modal GitHub-picker, role-picker, dismiss-only popup outcome routing, inline role/agent/session picker outcome routing, scope-picker/source-picker outcome routing, editor/settings browse-mode op-picker outcome routing, settings auth source/text/op-picker outcome routing, settings auth source-folder picker outcome routing, create-mode token op-picker outcome routing, editor/settings text-input outcome routing, editor/settings bool-confirm outcome routing, settings error-popup dismissal routing, editor workdir/error-popup outcome routing, editor/settings role-picker outcome routing, editor/settings file-browser modal outcome routing, editor/settings mount-destination choice routing, and editor/settings save-discard/confirm-save modal outcome routing now lives in `jackin-console/src/tui/update.rs`; root performs concrete effect requests, modal dismissal, and launch outcomes. Config sidebar selection input assembly, selected-sidebar target routing, global-mount row source routing, and inline picker role precedence now live in `jackin-console/src/tui/sidebar_layout.rs`; root supplies selected-row facts, selected inline picker rows, instance counts, and config existence checks. Footer row-kind facts, snapshot-routing, scroll-axis precedence, workspace-screen footer ownership, editor footer routing, and settings footer modal-precedence policies now live in `jackin-console/src/tui/components/footer_hints.rs`; root supplies concrete modal footer rows, instance lookup closures, and scroll-axis candidates. Workspace-list row-width routing, list-name content-width planning, and workspace footer list-column scroll geometry now live in `jackin-console/src/tui/list_geometry.rs`; root supplies concrete instance/workspace lookup callbacks. Workspace-list display-row vector assembly, display-row routing, and preview-pane routing now lives in `jackin-console/src/tui/screens/workspaces/view.rs`; root supplies concrete instance/workspace row lookup callbacks and render adapters. Workspace-list initial selection, saved-workspace reselection after save, row lookup, selected-row fallback, selected-index clamping, last-row index policy, Enter row policy, edit/delete/settings row planning, selected-row predicates, collapse selection routing, current-dir collapse selection indexing, collapse-result selection indexing, preview-pane selected-index clamping and focused action planning, visual-selected indexing, selected-instance scope routing, selected-instance direct-vs-scope routing, selected-instance action container lookup, selected-instance action/error planning, selected-instance purge-confirm open/error planning, instance-action empty-state message selection, top-level key routing, new-session row routing, horizontal tree-vs-scroll routing, list horizontal/vertical scroll-target planning, list mouse seam/click planning, pointer clickability routing, and workspace delete / instance purge key payload planning now live in `jackin-console/src/tui/screens/workspaces/update.rs`; root supplies concrete workspace and instance lookup, status mapping, state mutation, labels, root action mapping, and effect execution. Settings auth, settings shell/tab key routing, settings General/Environment/Trust-tab key routing, Settings tab-hover hit-testing, Settings Trust clickability routing, settings environment selected-key matching, settings environment selected-key op-ref predicate, settings environment selected op-ref/delete-key lookup, settings environment selected header expand/collapse routing, settings environment delete-target routing, settings environment selected-row deletion, settings environment selected-row mask toggling and maskability policy, settings environment selected add/picker target routing, settings environment selected Enter planning, settings environment role-picker choices/open planning, global-mount role-picker choices/open/commit planning, global-mount non-modal key routing, GitHub-open planning, and sensitive-save branch routing, global-mount confirm commit planning, global-mount selected-index clamping, global-mount text commit planning, normalization, and edit-row mutation, global-mount add draft destination mutation planning, global-mount add finalization planning and draft lifecycle, global-mount scope-picker choice planning, settings-env text commit planning, settings-env source-picker commit planning, settings-env op-picker target planning, settings-env scope-picker commit planning, settings-env role-picker commit planning, and add-row selection detection now lives in `jackin-console/src/tui/screens/settings/update.rs`; root supplies counts after add/remove or auth persistence. Settings-env pending key, error lifecycle, role expansion, picker scratch, pending picker value access, value mutation, selected-row deletion, pending picker target, and modal pending-key/target cleanup plus settings subpanel save refs, settings auth save refs, settings auth modal slot lifecycle, settings auth modal-open lookup lifecycle, settings auth modal-stack lifecycle, settings auth selected-kind, selected-kind query, scroll offset, and selection-movement lifecycle, settings auth pending op-commit lifecycle, settings auth row/env commit and clear lifecycle, settings trust error lifecycle and global-mount modal-dismiss add-draft cleanup, add-draft start state, error/exit flag lifecycle, add-row close mutation, remove-row selection mutation, and post-settings event error/exit aggregation now live with the shared settings state in `jackin-console/src/tui/screens/settings/model.rs`. Shared auth clear-layer helpers, modal-level Auth-form generate eligibility, Auth-form target/form generate traits, editor-level Auth-form generate query, Auth-token generate modal transition, Auth source-picker open transition, Auth source-folder browser transition, Auth op-picker open transition, Auth op-ref commit transition, Auth plain-source text-input transition, Auth plain-text/folder commit transitions, and Auth side-modal restore transition now live in `jackin-console/src/tui/auth_config.rs`, `jackin-console/src/tui/app.rs`, and `jackin-console/src/tui/screens/editor/model.rs`. Editor selected-mount removal, selected-mount isolation cycling, secret lookup, secret text-editability, focused unmask target derivation, focused secret Enter/delete/add target planning, focused secret op-ref detection, focused Secrets/Auth role-header expansion planning, role-header expansion key routing, focused Auth-kind lookup, focused Auth Enter planning, focused Mounts/Roles add-row selection, focused mount GitHub-open planning, horizontal-scroll key planning, workspace mount content-width planning, role override eligibility, allowed-role toggling, default-role toggling, Auth-tab role override eligibility, Auth-tab form prefill/source-folder derivation, Auth-tab form persistence/reset mutation, Auth-tab role expansion, Auth-tab clear-row mutation, and state-aware editor selection bounds, field-selection key planning, tab-navigation key planning, immediate editor action key planning, Roles-tab action key planning, Mounts-tab action key planning, Secrets-tab action key planning, Auth-tab action key planning, Enter-key planning, Escape-key planning, and Save-key planning now live with the shared editor state in `jackin-console/src/tui/screens/editor/model.rs`. Editor auth-generate scope classification, General-tab field modal routing, and tab-hover hit-testing now lives in `jackin-console/src/tui/screens/editor/update.rs`; root only opens the concrete modal state. Editor and settings frame rendering, footer-height stabilization, tab-strip rendering, tab-body dispatch, global-mount selected edit-text modal planning, settings-env key text, plain-value text, and value-edit text planning now live in `jackin-console/src/tui/screens/editor/view.rs` and `jackin-console/src/tui/screens/settings/view.rs`; root only supplies modal footer facts and opens concrete modal state. Primitive editor selection-bounds routing, secrets selection bounds, and add-row selection detection now live in `jackin-console/src/tui/screens/editor/update.rs`; root supplies only the concrete editor state and config. Reserved-footer height selection, footer minimum-height policy, and modal content-area derivation now live in `jackin-console/src/tui/view.rs`; root supplies editor/settings/workspace footer heights.

## What Still Lives In Root Console

### Domain Rules

`crates/jackin/src/console/domain.rs` owns:

- role source candidate derivation with root debug logging,
- role input resolution,
- committed agent launch provider derivation,
- provider derivation for launch,
- instance refresh snapshot shape.

Move potential: medium.

Blockers:

- uses `jackin_core::Agent` directly in production console code,
- uses `jackin_console::services` helpers for role/source and launch selection rules,
- uses `jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace, RoleSource, resolve_load_workspace}` directly in production console code,
- uses `jackin_core::RoleSelector` directly in production console code,
- uses `crate::instance` and `crate::runtime::snapshot`.

Recommendation:

- Keep provider derivation in root until the operator-env query can be injected or moved without creating a `jackin-console` -> `jackin-env` dependency cycle.
- `InstanceRefreshSnapshot` likely belongs in `jackin-console` only if instance index/session/snapshot types also move lower.

### Services

`crates/jackin/src/console/services/` owns root side effects:

- `agents.rs`: resolves supported agents through runtime.
- `config.rs`: persists workspaces/settings/role sources through root config editor.
- `instances.rs`: reads/rebuilds instance index, runs `docker ps`, overlays running instances, fetches daemon snapshots.
- `op.rs`: probes and validates `op`.
- `op_picker.rs`: starts op metadata loads and invalidates root op cache.
- `role_load.rs`: registers role repos through runtime.
- `token_setup.rs`: mints Claude OAuth token values.
- `workspace_save.rs`: starts drift detection and isolation cleanup workers.

Move potential: mixed.

Likely should stay in the binary adapter for now:

- Docker-backed instance reconciliation,
- runtime role registration,
- workspace drift detection,
- isolation cleanup,
- token minting,
- anything that needs `JackinPaths`, `CommandRunner`, `ShellRunner`, or runtime modules.

Likely movable with small interfaces:

- config-save request/response shapes,
- op-picker load request/result adapters once op data types fully live below root,
- some workspace save planning helpers that do not execute IO.

### Effects

`crates/jackin/src/console/effects.rs` executes typed `ManagerEffect` values and polls background work.

Move potential: low as a whole, high in pieces.

This file is root integration by design. It owns calls into root services and runtime paths. It should probably shrink over time, but not move wholesale. The reusable parts are effect vocabulary and pure planning, and much of that already lives in `jackin-console/src/tui/effect.rs`, `update.rs`, and `subscriptions.rs`.

Recommendation:

- Keep the executor in root.
- Continue moving effect request shapes and pure follow-up planning into `jackin-console`.
- Make `effects.rs` look like a thin interpreter: `ManagerEffect -> service call -> ManagerMessage`.

### TUI State, Message, Input, View

`crates/jackin/src/console/tui` is the main remaining duplication risk. It contains:

- root aliases binding generic `jackin-console` app/modal/stage types to root types,
- concrete `ManagerState`,
- root `ManagerMessage` alias and reducer,
- input handlers,
- mouse handlers,
- render adapters,
- root modal layout/render dispatch,
- root run loop.

Move potential:

- High for pure geometry/focus/row/update/view helpers that only need fact structs.
- Medium for state/update/input if more root-owned types become generic parameters.
- Low for terminal event loop and launch/attach handoff until effect execution is inverted behind a trait.

Current blockers:

- direct use of config/workspace data types from lower crates; these no longer block extraction by themselves,
- direct use of `RoleSelector`, `Agent`, provider types,
- direct use of `InstanceIndexEntry`, `InstanceStatus`, `SessionRecord`, `InstanceSnapshot`,
- direct use of lower-crate `EnvValue`, `OpRef`, `OpCache`, and op runner types,
- direct use of `JackinPaths`, `CommandRunner`, `runtime::RepoError`, drift types,
- integration tests and benchmarks import `jackin::console::tui::{ManagerState, render, handle_key}`.

## Console-Related Code Outside Both Console Locations

The search found console-related code outside `crates/jackin/src/console` and `crates/jackin-console`:

### Binary Crate Command/App Glue

- `crates/jackin/src/cli.rs`, `crates/jackin/src/cli/dispatch.rs`, `crates/jackin/src/cli/role.rs`: parse `jackin console`, bare `jackin` fallback, TTY checks.
- `crates/jackin/src/app.rs`: dispatches parsed commands to the console handler.
- `crates/jackin/src/app/load_cmd.rs`: enters terminal session, connects Docker, runs console, handles console outcomes.
- `crates/jackin/src/app/restore.rs`: handles console instance actions like reconnect, new session, shell, inspect, stop, purge.

These should remain outside `jackin-console`. They are CLI/runtime integration.

### Root Type Adapters

- `crates/jackin/src/agent.rs`: binds root `Agent` to console agent-choice state.
- `crates/jackin/src/selector.rs`: binds `RoleSelector` to console role-picker state.
- `crates/jackin/src/workspace.rs`: contains orphan-rule impl notes and workspace types used by console.
- `crates/jackin/src/app/context.rs`: role eligibility and preferred-agent helpers used by console domain.

These are extraction candidates only if their core data types move below the binary crate.

### Runtime/Protocol Support

- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`: `resolve_supported_agents_for_console`.
- `crates/jackin-runtime/src/runtime/attach.rs`: provider/session behavior used by console outcomes.
- `crates/jackin-runtime/src/runtime/snapshot.rs`: daemon snapshot types used by console preview/instance refresh.
- `crates/jackin-protocol/src/lib.rs`: provider selection fields that mention console-originated launches.
- `crates/jackin-capsule/src/protocol/attach.rs`: attach protocol variants for console-initiated provider/session actions.

These are shared runtime/protocol concerns, not UI ownership problems.

### Shared Visual Infrastructure

- `crates/jackin-tui/src/*`: shared components, geometry, modal lifecycle, status footer, text input, select list.
- `crates/jackin-tui-lookbook/src/stories.rs`: lookbook stories including console-flavored visual examples.
- `crates/jackin-launch/src/tui/*`: launch UI reuses visual language and container-info model.
- `crates/jackin-capsule/src/tui/*`: capsule UI shares design primitives and protocol concepts, but is a separate in-container TUI.

These should not move into `jackin-console`; they are shared across multiple TUI surfaces.

### Tests And Benchmarks

- `crates/jackin/tests/manager_flow.rs` and `crates/jackin/tests/manager_flow/**` import root console state/input directly.
- `crates/jackin/tests/frame_time.rs` imports root render path.
- `crates/jackin/benches/console_frame.rs` imports root render path.

These tests will need migration if the public console surface moves. Their current imports are a signal that root console internals are exposed as the practical test API.

## Can Everything Move Into `jackin-console`?

Not immediately, and not literally everything.

What should remain in `crates/jackin`:

- CLI argument parsing and dispatch,
- TTY capability fallback,
- terminal session entry tied to host diagnostics globals,
- Docker connection setup,
- runtime launch/attach/eject/restore handling after console returns,
- the concrete service interpreter that performs Docker/runtime/config filesystem work.

What should move or be extracted:

- root-independent state/update/view/input helpers,
- screen-specific planning currently in root input handlers,
- display row construction that can consume fact structs instead of root config structs,
- modal behavior and footer hint routing still implemented as root adapters,
- pure domain helpers once their types are lower-crate types,
- reusable service request/response types and non-IO planning.

The desired end state is therefore not "zero files in `crates/jackin/src/console`". A healthier target is "root `console/` is only an adapter shell": maybe `run.rs`, `effects.rs`, `services.rs`, `terminal.rs`, and a small type binding module. All product state, update, view, input planning, and business rules should live below the binary crate.

## Main Dependency Blockers

### Root-Owned Product Types

The largest blocker is type ownership. `jackin-console` cannot depend on the binary crate, so code using these types cannot move as-is:

- `crate::agent::Agent` shim paths only in root-console tests; production console code now uses `jackin_core::Agent`,
- `crate::selector::RoleSelector` shim paths only in root-console tests; production console code now uses `jackin_core::RoleSelector`,
- `crate::workspace` shim paths for workspace schema/resolution types only in tests; production console code now uses `jackin_config` for those types and helpers,
- `crate::config` shim paths for config model types only in tests; production console code now uses `jackin_config`,
- `crate::operator_env` shim paths for operator-env data types only in tests; production console code now uses `jackin_core` and `jackin_env`,
- `crate::selector::RolePickerState` and `crate::workspace::token_setup` shim paths only in tests/outside console; production console code now uses `jackin-console` and `jackin-env`,
- `crate::instance::{InstanceIndexEntry, InstanceStatus, SessionRecord}`,
- `crate::runtime::snapshot::InstanceSnapshot`,
- `crate::runtime::drift::DriftDetection`,
- `crate::paths::JackinPaths`,
- `crate::docker::CommandRunner`.

Some of these already have lower-crate homes conceptually. For example, `jackin-core` already owns shared selector/op vocabulary, and `jackin-config` owns config. A cleanup pass should decide which root types are accidental binary-owned product types.

### Effect Execution Is Still Root-Owned

`jackin-console` already has effects-as-data, but root still owns:

- effect draining,
- background worker creation,
- Docker/runtime/service execution,
- mapping results back into root `ManagerMessage`.

That is reasonable, but the boundary can be clearer. More pure planning can move into `jackin-console`; root should execute, not decide UI transitions.

### Tests Encode Root Internals As Public API

The integration tests and frame benchmark import root `ManagerState`, `handle_key`, `render`, and specific root state variants. Moving internals will require either:

- exporting equivalent test-friendly APIs from `jackin-console`,
- updating tests to use `jackin-console` model/update APIs,
- or keeping compatibility aliases in root during migration.

## Recommended Refactor Plan

### Phase 1: Document And Enforce The Boundary

- Keep `crates/jackin/src/console.rs`, but rewrite its module comment to say root console is an adapter layer, not the canonical home of console logic.
- Add a short boundary note to `crates/jackin-console/src/lib.rs` describing what belongs there.
- Update `codebase-map.mdx` after any structural move.

### Phase 2: Move Pure Root TUI Helpers First

Good candidates:

- modal outside-click/dismissal policies from root `run.rs`,
- debug location label derivation that can consume generic modal/stage facts,
- footer hint routing adapters that can become fact-based helpers,
- view row/line construction that currently reads root structs but can be converted to explicit fact structs,
- input planning that already delegates to `jackin-console::tui::screens::*::update`.

Goal: shrink root `components/`, `layout/`, `view/`, and pieces of `input/` without touching runtime behavior.

### Phase 3: Lower Root Product Types

Evaluate these moves:

- Production console code now uses `jackin_core::Agent` directly; remaining root-shim references are test-only cleanup.
- Production console code now uses `jackin_core::RoleSelector` directly; remaining root-shim references are test-only cleanup.
- `LoadWorkspaceInput` and `ResolvedWorkspace` are already lower-crate owned by `jackin-config`; production console code no longer depends on root shims for them.
- Move instance preview/index public shapes needed by console into `jackin-runtime` or a lower model crate.
- Move `operator_env` data types fully below root where possible; `jackin-env` already has pressure here.

Goal: make `domain.rs` and state aliases movable without generic explosion.

### Phase 4: Move State/Message/Update Into `jackin-console`

After type lowering, move:

- concrete `ManagerState`,
- concrete `ManagerStage` bindings,
- concrete `Modal` bindings,
- `ManagerMessage` alias/reducer,
- more input handlers that only produce effects.

Root should retain only the effect interpreter and terminal/application shell.

### Phase 5: Collapse Root Console To Adapter Shell

Desired root shape:

- `console.rs`: public binary-facing entrypoint exports.
- `console/run.rs`: calls `jackin_console::run_console` with concrete services.
- `console/effects.rs`: interprets effects by calling root/runtime services.
- `console/services.rs`: thin wrappers over config/runtime/Docker/op/token APIs.
- `console/terminal.rs`: host diagnostics terminal adapter.

Everything else should be in `jackin-console`, `jackin-tui`, `jackin-runtime`, `jackin-config`, `jackin-env`, or `jackin-core`.

## Risk Areas

- Moving too much at once will break tests across `manager_flow`, frame-time benchmarks, and TUI snapshots.
- Genericizing every root type inside `jackin-console` may make the code harder to understand than the current split.
- Moving runtime or Docker side effects into `jackin-console` would violate current tiering; `jackin-console` should stay UI/model/effects-as-data, not become a runtime crate.
- The docs already describe the split as intentional transitional architecture. Any refactor should update docs and avoid presenting current root code as accidental duplicate without acknowledging the extraction ledger.

## Suggested Decision

Adopt this target:

> All console product state, update planning, input planning, view composition, reusable components, and pure business rules live outside the binary crate. The binary crate keeps only terminal/application/runtime/service integration.

This gives the maintainability benefit the user wants while preserving the important dependency boundary: the reusable console crate should not know how to connect Docker, mutate host config files, launch containers, or attach to running instances.

## Recommended Final Structure

The strongest practical structure is not "everything under `jackin-console` and nothing under root." The strongest structure is "one product console crate plus one small binary adapter."

Recommended ownership:

```text
crates/jackin-console/
  src/
    domain/              pure console product decisions
    model/               console state, stage, modal, row, selection models
    update/              message reducers and transition planning
    input/               key/mouse planning that returns messages/effects
    view/                frame composition and display adapters
    components/          console-local reusable widgets and popup state builders
    services/            non-runtime helper services and request/response shapes
    effects.rs           effect vocabulary only, no Docker/runtime execution
    terminal.rs          generic terminal lifecycle helpers

crates/jackin/src/console/
  mod.rs                 binary-facing console entrypoint and re-exports
  run.rs                 wires jackin_console loop to concrete app/runtime services
  effects.rs             interprets jackin_console effects using root services
  services.rs            root IO adapters: config, Docker, runtime, op, token, paths
  terminal.rs            host diagnostics terminal adapter
```

This creates a single owner for console behavior while keeping side effects in the binary crate. The root `console/` folder remains, but it becomes boring glue instead of a second console implementation.

## Move / Keep Matrix

Move to `jackin-console`:

- console state shapes that are not inherently root-owned,
- modal/stage/list/editor/settings models,
- `ManagerMessage` and reducer logic once root-only payloads are lowered or genericized,
- keyboard and mouse planning that only mutates console state or emits effects,
- footer hints, modal routing, row construction, and view composition,
- save-preview display facts and validation wording,
- startup error dialog policy and other modal lifecycle decisions,
- role/workspace/provider selection decisions once their payload types live in lower crates,
- tests for pure state/input/update/view behavior.

Keep in `crates/jackin`:

- CLI parsing and `jackin console` command dispatch,
- TTY fallback and terminal capability checks,
- Docker daemon connection setup,
- `JackinPaths` ownership,
- concrete config file writes through `ConfigEditor`,
- runtime launch/attach/eject/restore calls,
- Docker-backed instance reconciliation,
- role repo registration through runtime,
- token minting and op CLI execution,
- the effect interpreter that turns `jackin-console` effects into real IO.

Move to lower crates before moving console code:

- `Agent` is already lower-crate owned by `jackin-core`; production console code no longer depends on the root shim.
- `RoleSelector` is already lower-crate owned by `jackin-core`; production console code no longer depends on the root shim.
- `LoadWorkspaceInput`, `ResolvedWorkspace`, `AppConfig`, `RoleSource`, `GlobalMountRow`, and workspace-resolution helpers are already lower-crate owned by `jackin-config`; production console code no longer depends on the root shims for them.
- instance display/index/snapshot types needed by the console should live in `jackin-runtime` or a lower model crate, not in binary root.
- operator environment data shapes (`EnvValue`, `OpRef`, cache metadata) already live in `jackin-core`/`jackin-env`; production console code no longer depends on the root shim for them.

## Recommended Path To Maximum Consolidation

1. First, define `jackin-console` as the canonical owner in docs and module comments.
   Do this before moving code so every later PR has a clear rule: if it is console behavior and does not execute root IO, it belongs in `jackin-console`.

2. Next, move pure TUI behavior that already depends mostly on `jackin-console`.
   Start with modal lifecycle policies, footer hint routing, display-row construction, and input planning helpers. These moves are low risk because they do not change root product types or runtime behavior.

3. Then lower shared product types out of `crates/jackin`.
   This is the main unlocking step. Without it, `jackin-console` either cannot own the concrete console model or must become over-generic. Prefer moving real product vocabulary to lower crates over genericizing everything.

4. After type lowering, move the concrete manager model and reducer.
   This is the point where `jackin-console` becomes the single real console implementation. Root should still supply config/runtime facts and execute effects.

5. Finally, move tests with the code.
   The current root `manager_flow` tests should become `jackin-console` tests where they test model/update/input/view behavior. Keep only end-to-end command and runtime integration tests in `crates/jackin/tests`.

## Concrete End-State Rule

Use this rule for future PR review:

> A new console feature may touch `crates/jackin/src/console` only when it needs CLI dispatch, host terminal ownership, Docker/runtime/config IO, or effect interpretation. All state, input, update, rendering, dialog, and product decision logic must go to `crates/jackin-console` or a lower shared crate.

This rule gets as close as possible to "everything related to jackin' console is stored in one place" while respecting Rust crate layering and keeping the binary crate responsible for actual host-side execution.
