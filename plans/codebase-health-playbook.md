# Codebase health — execution playbook

This is the **executor-grade** companion to [Codebase health: structure & reviewability](/roadmap/codebase-health-enforcement/). That page holds the strategy and decisions (D1–D7); **this page holds the mechanical steps**. It is written so a simple executor can run one slice at a time with no improvisation. Each slice is one shippable, behavior-preserving PR.

The strategy page is the source of truth for *why*; if a step here conflicts with a decision there, the decision wins — stop and flag it.

## Executor contract

Read once, obey on every slice.

1. **One slice = one PR.** Do exactly the slice you were given. Do not start the next slice, do not bundle.
2. **Structure only — never behavior.** Move code, relocate types, rename, edit Cargo/CI config. Never change logic, control flow, signatures, or anything an operator can observe. If a step seems to require editing a test to pass, the move changed behavior — **stop**, do not edit the test, report it.
3. **Respect preconditions.** If a slice lists preconditions, confirm those slices are merged first (check the strategy page's checkboxes). If not, stop.
4. **Run the Verify block in order. Stop on the first failure** and run the slice's Rollback. Never force past a red gate.
5. **Do not improvise.** If a step says `TODO(investigate)` or the code does not match what the step describes, **stop and report** — do not guess. Real code may have moved since this was written; when in doubt, stop.
6. **Conventions are non-negotiable:** no `mod.rs`; tests in a sibling `tests.rs` (`#[cfg(test)] mod tests;`), never inline, never child modules in `tests.rs`; every crate `[lints] workspace = true`; no wildcard imports.
7. **Commit:** Conventional Commit subject, sign off (`-s`), push immediately. Title `refactor(<scope>): <slice title>` unless the slice says otherwise.

### Standard verify commands

| Command | Expected |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | no warnings |
| `cargo nextest run -p <crate>` (or `--workspace` for cross-crate moves) | all pass |
| `cargo run -p jackin-xtask --locked -- lint` | file-size + test-layout OK; arch informational |
| behavioral specs `runtime-launch`, `op-picker` | pass **unmodified** |

If a file dropped under its cap, refresh the ratchet: `cargo run -p jackin-xtask --locked -- lint files --print-budget` (over `file-size-budget.toml`) / `lint tests --print-allowlist` (over `test-layout-allowlist.toml`), pruning the now-fixed entry.

### Per-slice template (the shape every block below follows)

- **Goal** · **Preconditions** · **Pattern** · **Touches**
- **Steps** — numbered mechanical actions
- **Verify** — exact commands + expected
- **Done when** · **Rollback** · **Open questions**

### Crate-carve recipe (referenced by the C/E carve slices)

1. Create `crates/<new>/` with a manifest (`[lints] workspace = true`, minimal deps) + a `//!` Architecture-Invariant header.
2. `git mv` the subsystem modules across verbatim (byte-identical bodies).
3. Visibility-only: the crate's public surface becomes `pub`; the rest stays `pub(crate)`. No signature/logic edits.
4. Repoint importers to the new crate root (Parallel Change).
5. Add the crate to root `Cargo.toml` `members`; add the old crate's dep on it only if it still calls inward.
6. Add the `cargo-deny` ban entries for the new allowed edges.
7. Update `PROJECT_STRUCTURE.md` + Codebase Map.
8. Run the standard Verify; for a hot-path carve, run the E0 benchmark vs baseline and attach the numbers.

## Slice index & order

Critical path: `A1–A4 → A5 → {B, C, D, W5} → F → {E0 → E1, E2} → {G0 → G1 → G2 → G3}`. Cold/dedup/W5 slices are independent once A5 is in.

> **Detailing status:** 21/23 slices detailed from the real code by the playbook workflow. Re-detail pending for: G2, W5-usage. Slices marked ⚠️ in the index carry open questions resolved in **Blockers & open decisions** below.
- **A1** — build_log trait port (P2) · _ready_
- **A2** — Relocate launch `progress` value types to jackin-core · _ready_
- **A3** — Move presentation helpers out of jackin-core (P7) · ⚠️ _carries open questions — see Blockers_
- **A4** — Move terminal-ownership state out of jackin-diagnostics (P7) · _ready_
- **A5** — cargo-deny dependency-direction bans + per-crate invariant headers · ⚠️ _carries open questions — see Blockers_
- **B1** — Rename jackin-launch → jackin-launch-tui (D4) · _ready_
- **B2** — Delete the 19 binary shim modules (P1, D6) · _ready_
- **C1** — Carve jackin-host out of jackin-runtime (cold) · _ready_
- **C2** — Carve jackin-usage out of jackin-capsule (cold) · ⚠️ _carries open questions — see Blockers_
- **D1** — Absorb runtime/image.rs into jackin-image (P5) + split it · ⚠️ _carries open questions — see Blockers_
- **D2** — Dedup env-resolution homes (P5) · _ready_
- **D3** — Dedup op_cache (P5) · _ready_
- **D4** — Dedup resource naming (P5) · _ready_
- **E0** — Launch/attach benchmark + enable lto=thin (prerequisite) · _ready_
- **E1** — Carve jackin-isolation out of jackin-runtime (GATED) · ⚠️ _carries open questions — see Blockers_
- **E2** — Carve jackin-instance out of jackin-runtime (GATED) · ⚠️ _carries open questions — see Blockers_
- **F1** — app_config_* fan-out → app_config/ coordinator + siblings (P8) · _ready_
- **G0** — Build shared Elm runtime contract in jackin-tui (D5) · _ready_
- **G1** — Migrate jackin-launch-tui onto the shared runtime · ⚠️ _carries open questions — see Blockers_
- **G2** — Migrate the capsule TUI onto the shared runtime · ⚠️ _detailing failed — re-detail before executing_
- **G3** — Migrate jackin-console onto the shared runtime (+ unify-settings) · ⚠️ _carries open questions — see Blockers_
- **W5-usage** — Decompose jackin-capsule usage.rs into provider modules · ⚠️ _detailing failed — re-detail before executing_
- **W5-console** — Decompose console tui/app.rs + editor/settings model.rs · ⚠️ _carries open questions — see Blockers_

## Slice playbook

## Blockers & open decisions

The detailing agents surfaced these by inspecting the real code. **Each is a "do-not-guess" item: resolve it (operator decision or a preparatory slice) before executing the named slice.** Many are circular-dependency or layer-inversion sub-problems that the one-line plan did not anticipate.

- **A3 — Move presentation helpers out of jackin-core (P7)**
  - How to migrate `crates/jackin-diagnostics/src/run.rs:35` (`use jackin_core::{JackinPaths, ansi_text::strip_bytes, prune_output}`) after both modules leave `jackin-core`. Adding `jackin-tui` to `jackin-diagnostics` is architecturally blocked because `jackin-config`, `jackin-manifest`, `jackin-docker`, `jackin-env`, `jackin-image`, `jackin-term` (all L0/L1/L2) depend on `jackin-diagnostics` and would transitively pull in `ratatui`. The executor must choose between: (a) inlining small private copies of `strip_bytes`/`PlainPerformer` plus `prune_output::section`/`start`/`PendingRow` into `run.rs` with `anstyle-parse` + `owo-colors` added to `jackin-diagnostics/Cargo.toml`; or (b) refactoring `prune_all_runs` (lines 693–708 of `run.rs`) to push the `prune_output::section`/`start` calls up to the CLI/entry-point layer before running this slice.
  - How to migrate `crates/jackin-protocol/src/attach.rs:1211` (`jackin_core::url_text::is_host_open_url`) after `url_text` leaves `jackin-core`. `jackin-protocol` is L0 domain and cannot depend on `jackin-tui` (L3 presentation). Options: (a) inline a three-line private `fn host_open_url_scheme_allowed(url: &str) -> bool` in `attach.rs`; (b) keep a minimal `url_text` module in `jackin-core` with only `is_host_open_url` (leaving `has_url_scheme` and `redact_url_for_log` in `jackin-tui`). Option (b) partially satisfies the 'Done when' condition but does not fully clean `jackin-core`.
  - Whether `crates/jackin-tui/src/ansi_text/tests.rs` currently passes `clippy::wildcard_imports` with its `use super::*;` on line 1, or is allowlisted. This determines whether the new `url_text/tests.rs` must use explicit `use super::X;` imports or can follow the same wildcard pattern.
- **A4 — Move terminal-ownership state out of jackin-diagnostics (P7)**
  - Should the new `jackin-diagnostics → jackin-tui` dependency edge (L2 infrastructure → L3 presentation) be added to FORBIDDEN_EDGES in `crates/jackin-xtask/src/arch.rs` as part of A5, or is it acceptable as a transitional edge? The roadmap's A5 section does not list this edge, and the executor must not guess — flag for operator decision before A5 lands.
  - The HOUSE RULES require an Architecture-Invariant `//!` header for new crates, but `ownership.rs` is a new module in an existing crate. If the reviewer wants an invariant comment on the module, the text would be: `//! Architecture Invariant: depends only on `crossterm` and `std`; no `jackin-*` crate deps.`
- **A5 — cargo-deny dependency-direction bans + per-crate invariant headers**
  - jackin-launch wrappers list: After A1/A2, who other than the `jackin` binary (L4) directly depends on `jackin-launch`? The `wrappers = ["jackin"]` in Step 3 assumes only the `jackin` binary holds the injection dep; if `jackin-capsule` or another L4 crate also needs it, add it to the list. Verify by inspecting A1/A2 PR diffs to `crates/jackin/Cargo.toml` and `crates/jackin-capsule/Cargo.toml`.
  - State of `crates/jackin-xtask/src/arch/tests.rs` and `FORBIDDEN_EDGES` after A2/A4: Steps 5–6 assume FORBIDDEN_EDGES is already `&[]` when A5 runs. If A2 and A4 each remove their entries as part of their own PRs, the test `synthetic_graph_flags_only_listed_forbidden_edges` (which currently asserts 3 problems) will have been updated by those PRs. If instead those PRs leave FORBIDDEN_EDGES untouched and leave it to A5, then A5 must also remove the remaining entries from FORBIDDEN_EDGES in Step 5 AND update the test in Step 6 with assertions reflecting exactly which entries remain. Check actual state of arch.rs/tests.rs after A1–A4 land before executing Step 5/6.
  - jackin-runtime allowed-deps in Step 11: After A2 removes the `jackin-runtime → jackin-launch` dep from `Cargo.toml`, `jackin-launch` must not appear in the `jackin-runtime` Architecture-Invariant header's allowed-deps list. The header written in Step 11 intentionally omits `jackin-launch`, but verify the actual `jackin-runtime/Cargo.toml` after A2 to confirm no other presentation crate was added in its place.
- **B2 — Delete the 19 binary shim modules (P1, D6)**
  - jackin-core/src/env_model.rs has pre-existing inline tests violating crates/AGENTS.md hard rule; Step 3 fixes this as a side-effect — confirm maintainer wants this in the same PR or a separate cleanup.
  - docker_client/tests.rs size_of<FakeDockerClient> assertion: delete or port to jackin-runtime/src/runtime/test_support/tests.rs?
  - The 19 shims can ship as clustered PRs (e.g., all image shims together, all config shims together, runtime shim alone). Confirm preferred grouping to avoid one mega-diff.
- **C1 — Carve jackin-host out of jackin-runtime (cold)**
  - Step 4 (editing jackin-runtime/src/runtime/test_support.rs) requires removing the FakeRunner struct definition and impl blocks spanning approximately lines 37-148 without removing install_all_test_stubs, TEST_DOCKERFILE_FROM, seed_valid_role_repo, or first_temp_role_repo; verify exact line ranges with wc -l or a read before editing.
  - The jackin-host Cargo.toml tokio dependency lists features = ['rt', 'macros']; if any moved file uses tokio::net, tokio::io, tokio::fs, or other feature-gated APIs, add those features to the jackin-host tokio dep to avoid a compile error.
  - The codebase-map.mdx file has a RepoFile component pointing to crates/jackin-runtime/src/runtime/caffeinate.rs (around line 132); this must be updated to crates/jackin-host/src/caffeinate.rs in step 19 or bun run check:repo-links will fail CI.
  - Verify that no other file in the workspace imports from super::caffeinate, super::host_clipboard, or super::host_desktop within jackin-runtime beyond the three files edited in steps 15-17; run: grep -rn 'super::caffeinate\|super::host_clipboard\|super::host_desktop' crates/jackin-runtime/src/ to confirm zero remaining references.
- **C2 — Carve jackin-usage out of jackin-capsule (cold)**
  - BLOCKER 1 — logging macro strategy: `clog!` and `cdebug!` in `crates/jackin-capsule/src/logging.rs` (lines 141–164) use `$crate::logging::write_line` and `$crate::telemetry::bridge_log`. Files to be moved (usage.rs 14 sites, telemetry.rs 2 sites, telemetry_store.rs 1 site, token_monitor.rs 1 site, token_monitor/opencode.rs 4 sites) all call `crate::clog!`/`crate::cdebug!`. After the move to jackin-usage, `crate::` resolves to `jackin_usage` — but no such macro exists there; importing from jackin-capsule creates a circular dep. Three options are described in the block (A: new leaf crate; B: extend jackin-diagnostics; C: move logging.rs to jackin-usage and re-export from capsule). The operator MUST pick one before execution. The steps above assume Option C; if a different option is chosen, steps 4, 13, and parts of step 3 change.
  - BLOCKER 2 — telemetry_store/tests.rs imports TUI type: `crates/jackin-capsule/src/telemetry_store/tests.rs` line 1 is `use crate::tui::components::dialog::Dialog;` and line 440 uses `Dialog::new_usage(view).usage_state()`. Moving telemetry_store to jackin-usage makes this a circular dep (jackin-usage → jackin-capsule for Dialog → jackin-usage for telemetry_store). Two options: (A) remove the Dialog assertion from tests (step 12 in the block); (B) keep telemetry_store in jackin-capsule (forces keeping usage in capsule too due to mutual calls — see usage.rs:394 calling crate::telemetry_store::store_usage_snapshots and telemetry_store.rs:354–490 calling crate::usage::*). The operator MUST decide and confirm before execution.
  - Confirm which Cargo.toml deps can be dropped from jackin-capsule after the move: reqwest, turso, fs2, url, serde, serde_json, base64, chrono are all used by the moved files. Before removing them from jackin-capsule/Cargo.toml, run `cargo shear` to verify no other capsule module still uses each one. Do NOT remove without verification — e.g. clipboard.rs uses base64, runtime_setup.rs uses reqwest.
  - Confirm exact line numbers in daemon.rs (steps 16), client.rs (step 17), daemon/multiplexer_utils.rs (step 18), and daemon/tests.rs (step 19) before editing — investigation was done on the current HEAD and line numbers shift with any concurrent changes.
  - Confirm whether the dhat-heap feature in jackin-usage Cargo.toml (step 2) is actually needed. alloc_telemetry.rs stays in jackin-capsule and is the only user of the dhat feature. If no moved file uses dhat, omit the feature from jackin-usage's manifest.
- **D1 — Absorb runtime/image.rs into jackin-image (P5) + split it**
  - A2 exact outcome (hard blocker before starting D1): after A2 lands, what are the exact Rust module paths for LaunchProgress and LaunchStage in jackin-core? The playbook writes `use jackin_core::{LaunchProgress, LaunchStage};` in image_build.rs but the actual path depends on how A2 is implemented. Executor must verify before writing that import.
  - Dev-dep cycle viability: the playbook adds jackin-runtime as a [dev-dependency] of jackin-image (features=["test-support"]) so that FakeDockerClient, FakeRunner, TEST_DOCKERFILE_FROM are accessible from jackin-image tests. Run `cargo check --workspace` immediately after that Cargo.toml edit; if Cargo rejects the cycle, the fallback is to move those test utilities into jackin-image itself or a shared jackin-test-fixtures crate.
  - Exact test-function partitioning in image/tests.rs (2111 lines): before writing the split test files, manually inspect every #[test]/#[tokio::test] function and classify it by the primary symbol it calls (image_recipe / image_decision / image_build / prewarm-coordinator). Do NOT guess — misplacing a test for a staying function into a moved file breaks compilation.
  - Exact Cargo.toml path for `futures-util` version constraint: the playbook writes `futures-util = "0.3"` in jackin-image/Cargo.toml; verify the exact version string matches what jackin-runtime's Cargo.toml uses so the workspace deduplicates the dep.
  - LABEL_IMAGE_KEY (const in runtime/naming.rs, value "jackin.image") is intentionally NOT moved to jackin-image — it is a container label consumed by cleanup.rs and stays in runtime/naming.rs. Confirm this is correct before deleting the image-naming symbols from naming.rs.
- **E1 — Carve jackin-isolation out of jackin-runtime (GATED)**
  - E0 precondition not met: Cargo.toml [profile.release] has no lto key, and no benchmark crate is in the workspace. E1 must not execute until E0 lands (adds lto = 'thin' to release profile and records a launch/attach baseline).
  - finalize.rs circular dependency: it uses crate::runtime::attach::JACKIN_STATUS_CMD (pub const) and crate::runtime::attach::parse_session_count (pub(crate)). The pub(crate) function cannot be accessed from outside jackin-runtime at all. Moving finalize.rs to jackin-isolation creates a circular dependency. Decision needed: does a preparatory slice move these to jackin-core first, or does finalize.rs stay in jackin-runtime for E1?
  - git_inspect.rs inverted layer dependency: worktree_inspect returns jackin_launch::WorktreeInspect (presentation-layer struct in jackin-launch/src/lib.rs:34). Moving git_inspect.rs to jackin-isolation (L1 application) creates an L1->L3 inverted dep. Decision needed: move WorktreeInspect/FileDiff to jackin-core first, or leave git_inspect.rs in jackin-runtime for E1?
  - FakeRunner dev-dependency: cleanup/tests.rs and finalize/tests.rs (if finalize moves) use crate::runtime::test_support::FakeRunner which is pub(crate)-gated in jackin-runtime. In jackin-isolation dev-deps, jackin-runtime with features=[test-support] should expose jackin_runtime::runtime::test_support::FakeRunner. Confirm this pattern works before executing.
  - materialize/tests.rs full import audit: only the first 30 lines of the 1337-line file were inspected. Run grep -n 'crate::isolation\|crate::runtime\|crate::instance' crates/jackin-runtime/src/isolation/materialize/tests.rs before step 7 to find all import rewrites.
  - Benchmark invocation: the exact command added by E0 (for the perf gate in step 13) is unknown. Read the E0 PR for the command; do not guess.
- **E2 — Carve jackin-instance out of jackin-runtime (GATED)**
  - E1 ordering: if jackin-isolation has already been carved out (E1 done before E2), its source files reference instance types via jackin_runtime::instance::* which continues to resolve through the re-export chain after E2. The executor must run `cargo nextest run -p jackin-isolation` (if the crate exists) to confirm this. If jackin-isolation imports directly from jackin_instance (not possible at E1 time but possible if E1 slice was written to add jackin-instance dep early), the dep graph must be verified.
  - rand version pinning: jackin-runtime uses rand = '0.10'. After adding rand = '0.10' to jackin-instance/Cargo.toml, verify Cargo.lock resolves to the same version (no duplicate) by running cargo build and inspecting Cargo.lock for duplicate rand entries.
  - cargo shear post-carve: after removing rand, chrono, serde_yaml_ng from jackin-runtime/[dependencies] and moving tempfile to [dev-dependencies], run cargo shear against the workspace to confirm no other jackin-runtime dep became orphaned as a side-effect.
  - dind_certs_volume test in runtime/naming/tests.rs (lines 93-99) tests the re-exported function; confirm it still compiles and passes after step 2 introduces the re-export rather than an inline definition.
  - TODO(investigate): confirm the exact column numbers / offsets for the three crate::instance::naming edits in manifest.rs step 13 — the line numbers quoted (75, 76, 77, 177) are from the file as read at investigation time and must be verified against the file state after Phase A step 3 has been applied, which may shift line numbers if the replacement strings differ in length.
- **G1 — Migrate jackin-launch-tui onto the shared runtime**
  - G0 contract shape (blocking Part B): what exact traits, types, and (if any) shared run-loop function does G0 add to crates/jackin-tui/src/runtime.rs or new files? The executor must read the G0 output before writing any code in Part B.
  - Does G0 provide a shared run-loop that replaces the 41 KB crates/jackin-launch-tui/src/tui/run.rs (RichRenderer + RichDriver), or is G0 purely a trait contract with no shared loop (in which case run.rs is unchanged in G1)?
  - Does G0 define a Component/View contract that any of the launch-local components under crates/jackin-launch-tui/src/tui/components/ must implement in G1, or is that deferred to G2/G3?
  - Has B1 already updated test-layout-allowlist.toml from crates/jackin-launch/src/tui/view.rs to crates/jackin-launch-tui/src/tui/view.rs? If not, G1 must include that path string replacement.
- **G3 — Migrate jackin-console onto the shared runtime (+ unify-settings)**
  - What are the exact trait names and signatures added to `crates/jackin-tui/src/runtime.rs` by G0? G3 is blocked until G0 ships and these are known.
  - Does the unified modal type stay as `ConsoleModal` in `crates/jackin-console/src/tui/app.rs` (line 762), or does a new `ConfigModal` type get introduced in a shared location within `jackin-console`? The unify-settings doc says 'leaning jackin-console, beside the shared widgets' but this decision is not locked.
  - Do the per-panel modal type params on `GlobalMountsState<Row, Modal>`, `SettingsEnvState<EnvValue, Modal>`, `SettingsAuthState<EnvValue, Modal, PendingOpCommit>` get removed entirely (replaced by one `modal: Option<ConsoleModal>` + `modal_parents: Vec<ConsoleModal>` on `SettingsState` itself, matching EditorState), or do the per-panel slots stay but change type to `ConsoleModal`?
  - How is the trust asymmetry reconciled: promote the editor to a `Trust` tab matching `SettingsTab::Trust` in `settings/model.rs` line 24, or keep the editor's `ConfirmTarget::TrustRoleSource` confirm flow and add the same pattern to settings?
  - What are the exact cluster boundaries for splitting `app.rs` (5095 L) after the modal collapse — which symbols move to sibling `app/` files and which stay in `app.rs` as the coordinator?
  - Which portions of `crates/jackin/src/console/tui/run.rs` (827 L) are absorbed by the G0 shared runtime event-loop helper vs which remain as console-specific glue after the G0 wiring?
  - The auth mouse-row-click asymmetry (`EditorState.auth_expanded` BTreeSet + row-click in the editor vs `SettingsAuthState.selected_kind` detail-rows with no row-click in settings) — is this an intentional per-surface affordance to preserve, or a behavior gap to reconcile in G3?
- **W5-console — Decompose console tui/app.rs + editor/settings model.rs**
  - Phase B (editor/settings model splits) cannot be executed until G3 (unify-settings) merges. The post-G3 unified file structure is unknown; the executor must re-derive cluster boundaries from the actual merged content before writing Phase B file-creation steps. Do not split the current duplicated pair.
  - modal.rs imports: the full set of `use crate::tui::auth_config::*` and `use crate::tui::update::*` items needed in `app/modal.rs` must be derived from compiler errors after the move — the original `app.rs` referenced many auth_config and update module items inline (e.g. `crate::tui::auth_config::ModalAuthFormGenerate`) without top-level use statements. The playbook lists only the imports visible in the original header; additional ones must be added to make modal.rs compile.
  - Confirm that `crate::tui::debug::ConsoleStageDebug` (referenced in manager_stage.rs line 672 region) has not been relocated by any prior slice. If it has moved, update the `use` in `app/manager_stage.rs`.
  - The `#[allow(clippy::large_enum_variant)]` attributes on `ConsoleManagerStage` (original line 254) and `ConsoleModal` (original line 760) must be preserved verbatim when those definitions move to their respective sibling files — clippy::large_enum_variant is not suppressed workspace-wide.

---

### A1 — build_log trait port (P2)

- **Goal:** Define `trait BuildLogSink` in `jackin-core`, wire it through `RunOptions` so `jackin-docker`'s `read_process_pipe` calls `sink.push_line` instead of the `jackin-diagnostics` global, add `DiagnosticsBuildLogSink` in `jackin-launch` as the concrete adapter, and inject it at the two `tee_to_build_log: true` sites in `jackin-runtime/src/runtime/image.rs` — keeping the file within its ratchet budget via compensating import merges.
- **Preconditions:** none (A0 is already shipped)
- **Pattern:** Branch by Abstraction (expand + contract in one PR): add the port trait, route every tee call through it, remove the direct `jackin_diagnostics::build_log::push_line` call from `shell_runner.rs`.
- **Touches:**
  - **created:** `crates/jackin-core/src/build_log_sink.rs`
  - **created:** `crates/jackin-launch/src/build_log.rs`
  - **modified:** `crates/jackin-core/src/lib.rs`
  - **modified:** `crates/jackin-core/src/runner.rs`
  - **modified:** `crates/jackin-launch/src/lib.rs`
  - **modified:** `crates/jackin-docker/src/shell_runner.rs`
  - **modified:** `crates/jackin-runtime/src/runtime/image.rs`

---

**Current-state snapshot** (confirmed by code inspection):

| Symbol | Location | Notes |
|---|---|---|
| `build_log::push_line` global | `jackin-diagnostics/src/build_log.rs:45` | called by shell_runner |
| `build_log::begin/end` | `jackin-diagnostics/src/build_log.rs:24,35` | called by image.rs lines 1853, 1868, 2184, 2205 |
| `build_log::snapshot/is_active` | `jackin-diagnostics/src/build_log.rs:62,40` | called by jackin-launch/src/tui/run.rs lines 120, 121 |
| `read_process_pipe` | `jackin-docker/src/shell_runner.rs:103` | private fn; calls `jackin_diagnostics::build_log::push_line` at lines 130, 141 |
| `let tee = opts.tee_to_build_log;` | `jackin-docker/src/shell_runner.rs:295` | feeds `read_process_pipe` tee param |
| `tee_to_build_log: true` (first) | `jackin-runtime/src/runtime/image.rs:1859` | inside `build_role_base_image`, runner param `runner: &mut impl CommandRunner` |
| `tee_to_build_log: true` (second) | `jackin-runtime/src/runtime/image.rs:2191` | inside `build_agent_image`, runner param same |
| file-size ratchet entry | `file-size-budget.toml:55-56` | `image.rs` budgeted at **2812 lines** (must not grow) |
| `jackin-launch/src/build_log.rs` | **does not exist** | the roadmap description of a "re-export shim" refers to the planned state; no such file exists currently |

---

**Steps** (each step is one mechanical action):

**1. Create `crates/jackin-core/src/build_log_sink.rs`** with this exact content:

```rust
//! Build-log line sink port (D2 in codebase-health-enforcement).
//!
//! Defined in the domain layer so infrastructure adapters (`jackin-docker`)
//! can call `push_line` without depending on the presentation layer.
//! `jackin-launch` provides the concrete adapter; `jackin-runtime` injects it.

/// Receives docker-build output lines for live display.
///
/// Architecture invariant: all callers of this trait must belong to
/// `jackin-docker` or lower layers only. The implementation lives in
/// `jackin-launch`.
pub trait BuildLogSink: Send + Sync + std::fmt::Debug {
    fn push_line(&self, line: &str);
}
```

**2. Edit `crates/jackin-core/src/lib.rs`**: after line 30 (`pub mod runner;`) insert:

```rust
pub mod build_log_sink;
```

After line 53 (`pub use runner::{CommandRunner, RunOptions};`) insert:

```rust
pub use build_log_sink::BuildLogSink;
```

**3. Edit `crates/jackin-core/src/runner.rs`**:

After line 9 (`use std::path::Path;`) insert two import lines:

```rust
use std::sync::Arc;
use crate::build_log_sink::BuildLogSink;
```

After line 29 (`pub tee_to_build_log: bool,`) insert the new field with its doc comment:

```rust
    /// The sink that receives tee'd build output when `tee_to_build_log` is
    /// true. Injected by the runtime entry point (`jackin-runtime`) before
    /// docker-build invocations; `None` suppresses teeing.
    pub build_log_sink: Option<Arc<dyn BuildLogSink>>,
```

After line 42 (`tee_to_build_log: false,`) inside `Default::default()` insert:

```rust
            build_log_sink: None,
```

**4. Create `crates/jackin-launch/src/build_log.rs`** with this exact content:

```rust
//! `DiagnosticsBuildLogSink`: adapter from the `BuildLogSink` port to the
//! `jackin-diagnostics` process-global build-log buffer.
//!
//! `jackin-runtime` constructs this and injects it into `RunOptions` before
//! any docker-build invocation, so `jackin-docker`'s `ShellRunner` never
//! imports from `jackin-launch` or `jackin-diagnostics` directly for teeing.

use jackin_core::BuildLogSink;

/// Wraps the process-global `jackin-diagnostics::build_log` buffer.
///
/// A zero-sized type; every `push_line` call forwards directly to the global.
#[derive(Debug)]
pub struct DiagnosticsBuildLogSink;

impl BuildLogSink for DiagnosticsBuildLogSink {
    fn push_line(&self, line: &str) {
        jackin_diagnostics::build_log::push_line(line);
    }
}
```

**5. Edit `crates/jackin-launch/src/lib.rs`**: after line 9 (`pub mod progress;`) insert:

```rust
pub mod build_log;
```

**6. Edit `crates/jackin-docker/src/shell_runner.rs`**:

**6a.** After line 10 (`use std::path::Path;`) insert:

```rust
use jackin_core::BuildLogSink;
```

**6b.** Replace the `read_process_pipe` function signature lines 103–108 (the four parameter lines `pipe`, `stream`, `tee_build_log`, `mut output` and the return type line) — change parameter `tee_build_log: bool,` to `sink: Option<&dyn BuildLogSink>,`:

Old (lines 103–108):
```rust
async fn read_process_pipe<R, W>(
    pipe: &mut R,
    stream: bool,
    tee_build_log: bool,
    mut output: W,
) -> std::io::Result<Vec<u8>>
```

New:
```rust
async fn read_process_pipe<R, W>(
    pipe: &mut R,
    stream: bool,
    sink: Option<&dyn BuildLogSink>,
    mut output: W,
) -> std::io::Result<Vec<u8>>
```

**6c.** Replace the `if tee_build_log {` block at lines 126–136 (the outer loop body checking `tee_build_log`):

Old:
```rust
        if tee_build_log {
            for &byte in &buf[..n] {
                if byte == b'\n' {
                    let line = String::from_utf8_lossy(&line_remainder);
                    jackin_diagnostics::build_log::push_line(line.trim_end_matches('\r'));
                    line_remainder.clear();
                } else {
                    line_remainder.push(byte);
                }
            }
        }
```

New:
```rust
        if let Some(s) = sink {
            for &byte in &buf[..n] {
                if byte == b'\n' {
                    let line = String::from_utf8_lossy(&line_remainder);
                    s.push_line(line.trim_end_matches('\r'));
                    line_remainder.clear();
                } else {
                    line_remainder.push(byte);
                }
            }
        }
```

**6d.** Replace the post-loop flush at lines 139–142:

Old:
```rust
    if tee_build_log && !line_remainder.is_empty() {
        let line = String::from_utf8_lossy(&line_remainder);
        jackin_diagnostics::build_log::push_line(line.trim_end_matches('\r'));
    }
```

New:
```rust
    if !line_remainder.is_empty() {
        let line = String::from_utf8_lossy(&line_remainder);
        sink.map(|s| s.push_line(line.trim_end_matches('\r')));
    }
```

**6e.** In `run_captured` (line 295), replace:

Old:
```rust
        let tee = opts.tee_to_build_log;
```

New:
```rust
        let (sink_out, sink_err) = (opts.build_log_sink.clone(), opts.build_log_sink.clone());
```

**6f.** Update the first `read_process_pipe` call (line 300):

Old:
```rust
            read_process_pipe(&mut stdout_pipe, stream, tee, std::io::stdout()).await
```

New:
```rust
            read_process_pipe(&mut stdout_pipe, stream, sink_out.as_deref(), std::io::stdout()).await
```

**6g.** Update the second `read_process_pipe` call (line 306):

Old:
```rust
            read_process_pipe(&mut stderr_pipe, stream, tee, std::io::stderr()).await
```

New:
```rust
            read_process_pipe(&mut stderr_pipe, stream, sink_err.as_deref(), std::io::stderr()).await
```

**7. Edit `crates/jackin-runtime/src/runtime/image.rs`** — all five edits below must be applied together; they are designed to net **zero** line-count change so the file stays at its ratcheted 2812 lines:

**7a.** Replace line 14 (`use std::collections::{BTreeMap, HashMap};`) and line 32 (`use std::path::PathBuf;`) with one merged line replacing line 14, and **delete** line 32:

Old line 14:
```rust
use std::collections::{BTreeMap, HashMap};
```
New line 14:
```rust
use std::{collections::{BTreeMap, HashMap}, path::PathBuf, sync::Arc};
```
Then delete the standalone line: `use std::path::PathBuf;`
Net: **−1 line**

**7b.** Replace lines 20–23 (four lines) with two lines:

Old (lines 20–23):
```rust
#[cfg(not(test))]
use jackin_docker::ShellRunner;
#[cfg(not(test))]
use jackin_docker::docker_client::BollardDockerClient;
```

New:
```rust
#[cfg(not(test))]
use jackin_docker::{ShellRunner, docker_client::BollardDockerClient};
```
Net: **−2 lines**

**7c.** After the line `use jackin_manifest::repo::CachedRepo;` (currently line 31; becomes line 28 after 7a and 7b) insert:

```rust
use jackin_launch::build_log::DiagnosticsBuildLogSink;
```
Net: **+1 line**

**7d.** In the first `RunOptions` construction with `tee_to_build_log: true` (inside the role-base build, identifiable by `runner.run("docker", &args, None, &build_options)`), add the `build_log_sink` field after `tee_to_build_log: true,`:

Old:
```rust
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &args, None, &build_options);
```

New:
```rust
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &args, None, &build_options);
```
Net: **+1 line**

**7e.** In the second `RunOptions` construction with `tee_to_build_log: true` (inside `build_agent_image`, identifiable by `runner.run("docker", &build_args, None, &build_options)`), add the `build_log_sink` field after `tee_to_build_log: true,`:

Old:
```rust
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &build_args, None, &build_options);
```

New:
```rust
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &build_args, None, &build_options);
```
Net: **+1 line**

**image.rs total: −1 − 2 + 1 + 1 + 1 = 0 net lines → stays at 2812. Ratchet satisfied.**

**No Cargo.toml changes required.** All dependency edges already exist: `jackin-runtime → jackin-launch`, `jackin-launch → jackin-core`, `jackin-launch → jackin-diagnostics`, `jackin-docker → jackin-core`.

---

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → exits 0 (no formatting diffs)
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → 0 warnings (new trait uses `Send + Sync + Debug` so `Arc<dyn BuildLogSink>` satisfies all derive bounds on `RunOptions`)
- `cargo nextest run -p jackin-core` → all pass
- `cargo nextest run -p jackin-docker` → all pass (shell_runner tests use `RunOptions::default()` which sets `build_log_sink: None`; `read_process_pipe` never enters the `if let Some(s) = sink` branch so behavior is identical)
- `cargo nextest run -p jackin-launch` → all pass
- `cargo nextest run -p jackin-runtime` → all pass (`launch/tests.rs:3120` asserts `build_opts.tee_to_build_log` only; the added `build_log_sink` field is not asserted)
- `cargo nextest run --workspace --all-features` → all pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + arch all OK; `image.rs` stays at 2812 lines
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → verify `image.rs` entry still shows `lines = 2812` (no ratchet violation)
- behavioral specs `runtime-launch` and `op-picker` pass **unmodified**

---

**Done when:**
- `crates/jackin-core/src/build_log_sink.rs` exists and exports `pub trait BuildLogSink`
- `RunOptions::build_log_sink` field exists, defaults to `None`
- `crates/jackin-launch/src/build_log.rs` exists with `pub struct DiagnosticsBuildLogSink` implementing `BuildLogSink`
- `read_process_pipe` in `jackin-docker/src/shell_runner.rs` no longer calls `jackin_diagnostics::build_log::push_line` directly; calls `sink.push_line` when `sink` is `Some`
- Both `RunOptions { tee_to_build_log: true, ... }` constructions in `image.rs` inject `Some(Arc::new(DiagnosticsBuildLogSink))`
- `cargo xtask lint` exits 0; `image.rs` budget entry is unchanged at 2812 lines
- All tests pass unmodified

**Rollback:** `git restore crates/jackin-core/src/lib.rs crates/jackin-core/src/runner.rs crates/jackin-docker/src/shell_runner.rs crates/jackin-launch/src/lib.rs crates/jackin-runtime/src/runtime/image.rs && git rm --cached crates/jackin-core/src/build_log_sink.rs crates/jackin-launch/src/build_log.rs && rm crates/jackin-core/src/build_log_sink.rs crates/jackin-launch/src/build_log.rs`

**Open questions:** none — all symbols, line ranges, and dependency edges were confirmed from the live code.

---

### A2 — Relocate launch `progress` value types to jackin-core

- **Goal:** Move all non-TUI launch progress value types and port traits from `jackin-launch` into `jackin-core` so that `jackin-runtime` no longer imports value-type definitions from `jackin-launch`.
- **Preconditions:** A0 (PromptResult already in jackin-core; that re-export pattern is the precedent)
- **Pattern:** Parallel Change — create the types in `jackin-core`, add re-exports in `jackin-launch` so all existing paths keep compiling, then update the direct `jackin_launch::TypeName` qualified paths in `jackin-runtime` production code to use `jackin_core` directly.
- **Touches:**
  - **Create:** `crates/jackin-core/src/launch_progress.rs`
  - **Modify:** `crates/jackin-core/src/lib.rs`
  - **Modify:** `crates/jackin-launch/src/tui/app.rs`
  - **Modify:** `crates/jackin-launch/src/lib.rs`
  - **Modify:** `crates/jackin-runtime/src/runtime/progress.rs`
  - **Modify:** `crates/jackin-runtime/src/runtime/launch/restore.rs`
  - **Modify:** `crates/jackin-runtime/src/runtime/launch/progress_helpers.rs`
  - **Modify:** `crates/jackin-runtime/src/isolation/git_inspect.rs`
  - **Modify:** `crates/jackin-runtime/src/isolation/finalize.rs`

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Create** `crates/jackin-core/src/launch_progress.rs` with the following exact content (copy verbatim — preserve every doc comment and attribute from the originals):

```rust
//! Non-UI launch cockpit value types: stages, identity, failure, restore
//! dialog data, and port traits. Shared by the orchestration layer
//! (`jackin-runtime`) and the presentation layer (`jackin-launch`) with no
//! dependency on `ratatui` or `jackin-tui`.

use std::path::{Path, PathBuf};

// --- Stage types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LaunchStage {
    Identity,
    Role,
    Credentials,
    Construct,
    AgentBinaries,
    DerivedImage,
    Workspace,
    Network,
    Sidecar,
    Capsule,
    Hardline,
}

impl LaunchStage {
    pub const ALL: [Self; 11] = [
        Self::Identity,
        Self::Role,
        Self::Credentials,
        Self::Construct,
        Self::AgentBinaries,
        Self::DerivedImage,
        Self::Workspace,
        Self::Network,
        Self::Sidecar,
        Self::Capsule,
        Self::Hardline,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Role => "role",
            Self::Credentials => "credentials",
            Self::Construct => "construct",
            Self::AgentBinaries => "agent binaries",
            Self::DerivedImage => "derived image",
            Self::Workspace => "workspace",
            Self::Network => "network",
            Self::Sidecar => "sidecar",
            Self::Capsule => "capsule",
            Self::Hardline => "hardline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Queued,
    Running,
    Done,
    Skipped,
    Failed,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct StageView {
    pub stage: LaunchStage,
    pub status: StageStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy)]
pub struct StageLabelTransition {
    pub from: usize,
    pub to: usize,
    pub start_frame: usize,
}

// --- Launch identity and failure ---

#[derive(Debug, Clone)]
pub struct LaunchIdentity {
    pub role: String,
    pub agent: String,
    pub target_kind: LaunchTargetKind,
    pub target_label: String,
    /// Mounts whose host source differs from the container destination,
    /// pre-formatted for display. Same-path mounts are omitted upstream.
    pub mounts: Vec<String>,
    pub image: Option<String>,
    pub container: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub detail: Option<String>,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
    pub diagnostics_path: Option<PathBuf>,
    pub command_output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTargetKind {
    Workspace,
    Directory,
}

impl LaunchTargetKind {
    #[must_use]
    pub const fn launch_preposition(self) -> &'static str {
        match self {
            Self::Workspace => "into workspace",
            Self::Directory => "in directory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCopyTarget {
    RunId,
    DiagnosticsPath,
    CommandOutputPath,
}

// --- Prompt context ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContextLine {
    Emphasis(String),
    Muted(String),
    Path(String),
    Plain(String),
    Blank,
}

// --- Restore dialog types ---

/// One changed file entry for the D24 Inspect surface.
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Porcelain status character (`M`, `A`, `D`, `?`, …).
    pub status: char,
    /// Path relative to the worktree root.
    pub path: String,
    /// File content at HEAD — `None` for added/untracked files.
    pub before: Option<String>,
    /// File content in the working tree — `None` for deleted files.
    pub after: Option<String>,
}

/// Pre-computed inspection data for one worktree in the D24 surface.
#[derive(Debug, Clone)]
pub struct WorktreeInspect {
    /// Display label shown in the repos pane (workspace name or mount path).
    pub label: String,
    /// Changed files with their diff content.
    pub files: Vec<FileDiff>,
}

/// One candidate row in the D23 launch dialog.
#[derive(Debug, Clone)]
pub struct LaunchCandidate {
    /// Formatted label shown in the picker list.
    pub label: String,
    /// `true` if the candidate has dirty/unpushed state.
    /// Dirty candidates require a `ConfirmDialog` before deletion (D21).
    pub is_dirty: bool,
    /// Pre-fetched inspect data (one entry per isolated worktree in this
    /// instance). Empty for clean/crashed instances with no worktree state.
    pub inspect: Vec<WorktreeInspect>,
}

/// Outcome of the D23 launch dialog.
#[derive(Debug, Clone)]
pub enum LaunchDialogResult {
    /// Operator chose to start a new instance.
    StartFresh,
    /// Operator chose to restore the candidate at this index.
    Restore(usize),
    /// Operator confirmed deletion of the candidate at this index.
    Delete(usize),
}

// --- Cancellation marker ---

/// Marker error: the operator deliberately aborted the launch (Ctrl+C,
/// Ctrl+Q, or a Cancel modal). This is an intent, not a failure — the binary
/// entry point treats it as a clean exit and never renders it as `error:`.
///
/// Carried as a concrete error inside an `anyhow::Error` so any layer can
/// detect it via [`LaunchCancelled::is_cancel`] regardless of `.context(..)`
/// wrapping. `Display` keeps the historical "launch cancelled by operator"
/// wording for debug/log surfaces.
#[derive(Debug)]
pub struct LaunchCancelled;

impl std::fmt::Display for LaunchCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("launch cancelled by operator")
    }
}

impl std::error::Error for LaunchCancelled {}

impl LaunchCancelled {
    /// Build the cancellation as an `anyhow::Error` for return up the stack.
    pub fn err() -> anyhow::Error {
        anyhow::Error::new(Self)
    }

    /// `true` if `error` — or anything in its source chain — is a
    /// `LaunchCancelled`. `anyhow`'s downcast walks the chain, so the check
    /// survives intermediate `.context(..)` layers.
    pub fn is_cancel(error: &anyhow::Error) -> bool {
        error.downcast_ref::<Self>().is_some()
    }
}

// --- Port traits ---

pub trait LaunchDiagnostics: Send + Sync {
    fn run_id(&self) -> &str;
    fn path(&self) -> &Path;
    fn command_output_path(&self, name: &str) -> PathBuf;
    fn compact(&self, kind: &str, message: &str);
    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>);
}

pub trait LaunchHostTerminal: Send + Sync {
    fn set_rich_surface_active(&self, active: bool);
    fn host_screen_owned(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
    fn emit_compact_line(&self, kind: &str, line: &str);
    fn emit_debug_line(&self, category: &str, line: &str);
    fn set_pointer_shape(&self, pointer: bool);
    fn copy_to_clipboard(&self, payload: &str) -> bool;
    fn reveal_file(&self, path: &Path) -> bool;
    fn open_file(&self, path: &Path) -> bool;
}
```

2. **Edit** `crates/jackin-core/src/lib.rs` — add the new module and re-exports. After the last existing `pub mod` line (`pub mod worktree_dirty;`, line 33) and before the first `pub use` line (`pub use agent::...`, line 35), insert:

```rust
pub mod launch_progress;
```

Then add to the `pub use` block at the bottom of `lib.rs` (after line 54, `pub use selector::{RoleSelector, Selector, SelectorError};`):

```rust
pub use launch_progress::{
    FailureCopyTarget, FileDiff, LaunchCandidate, LaunchCancelled, LaunchDialogResult,
    LaunchDiagnostics, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchStage,
    LaunchTargetKind, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    WorktreeInspect,
};
```

3. **Edit** `crates/jackin-launch/src/tui/app.rs` — remove moved type definitions and add re-exports.

   a. Remove lines 3–5 (the `use std::path::PathBuf;` import — no longer needed after `LaunchFailure` moves):
   ```
   use std::path::PathBuf;
   ```

   b. Remove lines 9–15 (`PromptContextLine` enum definition):
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum PromptContextLine {
       Emphasis(String),
       Muted(String),
       Path(String),
       Plain(String),
       Blank,
   }
   ```

   c. Remove lines 17–31 (`LaunchStage` enum definition):
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
   pub enum LaunchStage {
       Identity,
       Role,
       Credentials,
       Construct,
       AgentBinaries,
       DerivedImage,
       Workspace,
       Network,
       Sidecar,
       Capsule,
       Hardline,
   }
   ```

   d. Remove lines 32–63 (`impl LaunchStage { ... }` block including `ALL` const and `label` method).

   e. Remove lines 65–73 (`StageStatus` enum definition):
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum StageStatus {
       Queued,
       Running,
       Done,
       Skipped,
       Failed,
       Blocked,
   }
   ```

   f. Remove lines 75–80 (`StageView` struct definition):
   ```rust
   #[derive(Debug, Clone)]
   pub struct StageView {
       pub stage: LaunchStage,
       pub status: StageStatus,
       pub detail: String,
   }
   ```

   g. Remove lines 140–145 (`FailureCopyTarget` enum definition):
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum FailureCopyTarget {
       RunId,
       DiagnosticsPath,
       CommandOutputPath,
   }
   ```

   h. Remove lines 147–152 (`StageLabelTransition` struct definition):
   ```rust
   #[derive(Debug, Clone, Copy)]
   pub struct StageLabelTransition {
       pub from: usize,
       pub to: usize,
       pub start_frame: usize,
   }
   ```

   i. Remove lines 161–172 (`LaunchIdentity` struct definition).

   j. Remove lines 174–184 (`LaunchFailure` struct definition).

   k. Remove lines 185–199 (`LaunchTargetKind` enum + `impl LaunchTargetKind` block).

   l. After line 6 (`use ratatui::text::Line;`) and before the `#[derive(...)] pub struct LaunchView` block, add the re-exports so they remain available at the `crate::tui::app::TypeName` path:
   ```rust
   pub use jackin_core::launch_progress::{
       FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind,
       PromptContextLine, StageLabelTransition, StageStatus, StageView,
   };
   ```

   The file now starts with:
   ```rust
   //! Launch cockpit model types shared with runtime orchestration.

   use jackin_tui::components::StatusFooterHover;
   use ratatui::text::Line;

   pub use jackin_core::launch_progress::{
       FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind,
       PromptContextLine, StageLabelTransition, StageStatus, StageView,
   };
   // Re-exported from `jackin_core` (Workstream 1 — architecture/boundaries: ...
   pub use jackin_core::PromptResult;

   #[derive(...)]
   pub struct LaunchView { ... }
   ```

4. **Edit** `crates/jackin-launch/src/lib.rs` — remove moved type definitions and add re-exports.

   a. Remove lines 7–8 (`use std::path::{Path, PathBuf};`). Keep `use std::path::{Path, PathBuf};` — actually, the `impl LaunchDiagnostics for jackin_diagnostics::RunDiagnostics` block (lines 106–126) still returns `&Path` and `PathBuf`, so keep `use std::path::{Path, PathBuf};`.

   b. Remove lines 19–63 (the entire block: `FileDiff` struct, `WorktreeInspect` struct, `LaunchCandidate` struct, `LaunchDialogResult` enum). These are lines 19 (`/// One changed file entry`) through 63 (closing `}` of `LaunchDialogResult`).

   c. Remove lines 65–96 (the entire `LaunchCancelled` block: the `///` doc comment through the closing `}` of `impl LaunchCancelled`).

   d. Remove lines 98–104 (`pub trait LaunchDiagnostics: Send + Sync { ... }` definition — 6 lines up to the closing `}`). Keep the `impl LaunchDiagnostics for jackin_diagnostics::RunDiagnostics` block below it (lines 106–126), which now gets `LaunchDiagnostics` from the re-export.

   e. Remove lines 128–138 (`pub trait LaunchHostTerminal: Send + Sync { ... }` definition). Keep the `mod test_support` block which implements it; `test_support` already uses `super::LaunchHostTerminal` so it picks up the re-export.

   f. After the `pub use tui::update::{...}` line (line 17), add:
   ```rust
   pub use jackin_core::launch_progress::{
       FailureCopyTarget, FileDiff, LaunchCandidate, LaunchCancelled, LaunchDialogResult,
       LaunchDiagnostics, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchStage,
       LaunchTargetKind, PromptContextLine, StageLabelTransition, StageStatus, StageView,
       WorktreeInspect,
   };
   ```

   The existing lines 12–17 stay unchanged:
   ```rust
   pub use tui::app::{
       FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, LaunchView,
       PromptContextLine, PromptResult, StageLabelTransition, StageStatus, StageView,
   };
   pub use tui::message::LaunchMessage;
   pub use tui::update::{active_stage_index, initial_view, update_launch_view, update_stage};
   ```
   These re-export via `tui::app` which in turn re-exports from `jackin-core`, so they continue to work. No duplication issue: Rust allows multiple `pub use` aliases for the same item.

5. **Edit** `crates/jackin-runtime/src/runtime/progress.rs` — update production imports to reference `jackin-core` directly.

   Replace lines 11–44 (from `pub use jackin_launch::LaunchCancelled;` through the closing `;` of the big `pub use jackin_launch::{...}` block) with:

   ```rust
   pub use jackin_core::launch_progress::{
       FailureCopyTarget, LaunchCancelled, LaunchCandidate, LaunchDialogResult, LaunchFailure,
       LaunchIdentity, LaunchStage, LaunchTargetKind, PromptContextLine, StageLabelTransition,
       StageStatus, StageView, WorktreeInspect,
   };
   use jackin_core::launch_progress::LaunchHostTerminal;
   pub use jackin_launch::{
       LaunchMessage, LaunchView, active_stage_index, initial_view, update_launch_view,
       update_stage,
   };
   pub use jackin_launch::progress::LaunchProgress;
   ```

   Also, in the function signatures on lines 183, 198–199 change qualified `jackin_launch::` prefix to unqualified (these types are now in scope from the `pub use jackin_core::launch_progress::{...}` above):

   Line 183: `worktrees_per_record: &[Vec<jackin_launch::WorktreeInspect>]`
   → `worktrees_per_record: &[Vec<WorktreeInspect>]`

   Line 198: `candidates: &[jackin_launch::LaunchCandidate]`
   → `candidates: &[LaunchCandidate]`

   Line 199: `) -> anyhow::Result<jackin_launch::LaunchDialogResult> {`
   → `) -> anyhow::Result<LaunchDialogResult> {`

   All `#[cfg(test)]` import lines (lines 14–56) are left **unchanged**.

6. **Edit** `crates/jackin-runtime/src/runtime/launch/restore.rs` — add `use` imports and replace all qualified `jackin_launch::` paths.

   After the existing `use jackin_docker::docker_client::DockerApi;` import line (line 6), add:
   ```rust
   use jackin_core::launch_progress::{LaunchCandidate, LaunchDialogResult};
   ```

   Then replace every occurrence of `jackin_launch::LaunchCandidate` with `LaunchCandidate` (3 occurrences: lines 17, 31, 53) and every occurrence of `jackin_launch::LaunchDialogResult::` with `LaunchDialogResult::` (4 occurrences: lines 81, 85, 88, 91).

7. **Edit** `crates/jackin-runtime/src/runtime/launch/progress_helpers.rs` — update the one `LaunchCancelled` qualified path.

   Line 35: `return Err(jackin_launch::LaunchCancelled::err());`
   → `return Err(jackin_core::launch_progress::LaunchCancelled::err());`

8. **Edit** `crates/jackin-runtime/src/isolation/git_inspect.rs` — add `use` imports and replace qualified `jackin_launch::` paths.

   After the existing `use jackin_core::worktree_dirty::{ChangedFile, parse_porcelain};` line (line 16), add:
   ```rust
   use jackin_core::launch_progress::{FileDiff, WorktreeInspect};
   ```

   Then replace:
   - Line 70: `pub fn worktree_inspect(worktree_path: &str) -> jackin_launch::WorktreeInspect {`
     → `pub fn worktree_inspect(worktree_path: &str) -> WorktreeInspect {`
   - Line 73: `.map(|f| jackin_launch::FileDiff {`
     → `.map(|f| FileDiff {`
   - Line 80: `jackin_launch::WorktreeInspect {`
     → `WorktreeInspect {`

9. **Edit** `crates/jackin-runtime/src/isolation/finalize.rs` — add `use` import and replace the one qualified `jackin_launch::WorktreeInspect` path.

   After the existing `use jackin_core::CommandRunner;` import line (line 32), add:
   ```rust
   use jackin_core::launch_progress::WorktreeInspect;
   ```

   Then replace line 207: `let worktrees_per_record: Vec<Vec<jackin_launch::WorktreeInspect>> = records`
   → `let worktrees_per_record: Vec<Vec<WorktreeInspect>> = records`

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exits 0, no formatting diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-core` → all pass
- `cargo nextest run -p jackin-launch` → all pass
- `cargo nextest run -p jackin-runtime` → all pass (behavioral specs `runtime-launch` in `crates/jackin-runtime/src/runtime/launch/tests.rs` pass unmodified)
- `cargo nextest run -p jackin-console` → all pass (op-picker specs in `crates/jackin-console/src/tui/app.rs` pass unmodified)
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK; arch gate may still report `jackin-runtime → jackin-tui` informational (non-strict, expected)

**Done when:** `crates/jackin-core/src/launch_progress.rs` exists; `LaunchStage`, `StageStatus`, `StageView`, `StageLabelTransition`, `LaunchIdentity`, `LaunchFailure`, `LaunchTargetKind`, `FailureCopyTarget`, `PromptContextLine`, `FileDiff`, `WorktreeInspect`, `LaunchCandidate`, `LaunchDialogResult`, `LaunchCancelled`, `LaunchDiagnostics`, and `LaunchHostTerminal` are no longer defined in `jackin-launch`; `jackin-launch` re-exports all of them from `jackin-core::launch_progress`; `jackin-runtime`'s production (non-test) code imports these types from `jackin-core::launch_progress` directly; all clippy/nextest gates pass.

**Rollback:** `git restore crates/jackin-core/src/launch_progress.rs crates/jackin-core/src/lib.rs crates/jackin-launch/src/tui/app.rs crates/jackin-launch/src/lib.rs crates/jackin-runtime/src/runtime/progress.rs crates/jackin-runtime/src/runtime/launch/restore.rs crates/jackin-runtime/src/runtime/launch/progress_helpers.rs crates/jackin-runtime/src/isolation/git_inspect.rs crates/jackin-runtime/src/isolation/finalize.rs` then `git rm crates/jackin-core/src/launch_progress.rs`

**Open questions:** none

---

### A3 — Move presentation helpers out of jackin-core (P7)

- **Goal:** Remove all rendering/presentation code from `jackin-core` by relocating `ansi_text` (ANSI stripping), `prune_output` (formatted terminal rows), and `url_text` (URL safety helpers) into `jackin-tui`, so `jackin-core` is a purely IO-free vocabulary crate.
- **Preconditions:** none (A3 is independent of A1/A2 in Phase A; does not require A1 or A2 to have landed first)
- **Pattern:** Parallel Change (expand → migrate → contract): `jackin-tui` already has stub modules (`prune_output.rs` = wildcard re-export; `ansi_text.rs` = re-export + extension). Replace each stub with the real implementation, update call sites, then delete the source modules from `jackin-core`.
- **Touches:**
  - **Modified:** `crates/jackin-core/src/lib.rs`, `crates/jackin-core/Cargo.toml`
  - **Deleted:** `crates/jackin-core/src/ansi_text.rs`, `crates/jackin-core/src/ansi_text/tests.rs`, `crates/jackin-core/src/prune_output.rs`, `crates/jackin-core/src/prune_output/tests.rs`, `crates/jackin-core/src/url_text.rs`
  - **Modified:** `crates/jackin-tui/src/ansi_text.rs`, `crates/jackin-tui/src/prune_output.rs`, `crates/jackin-tui/src/lib.rs`
  - **Created:** `crates/jackin-tui/src/prune_output/tests.rs`, `crates/jackin-tui/src/url_text.rs`, `crates/jackin-tui/src/url_text/tests.rs`
  - **Modified (call-site rewrites):** `crates/jackin-capsule/src/daemon/input_dispatch.rs`, `crates/jackin-capsule/src/daemon/mouse_input.rs`, `crates/jackin-capsule/src/tui/run.rs`, `crates/jackin-runtime/src/runtime/host_attach.rs`, `crates/jackin-runtime/src/runtime/host_desktop.rs`
  - **Modified:** `test-layout-allowlist.toml`
  - **TODO(investigate):** `crates/jackin-diagnostics/src/run.rs` (see open questions), `crates/jackin-protocol/src/attach.rs` (see open questions)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **In `crates/jackin-tui/src/ansi_text.rs`**: Remove line 11 (`pub use jackin_core::ansi_text::strip_bytes;`). After the existing `use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};` import block, insert the `strip_bytes` function and `PlainPerformer` struct copied verbatim from `crates/jackin-core/src/ansi_text.rs` (lines 5–31: `#[must_use] pub fn strip_bytes(bytes: &[u8]) -> Vec<u8> { ... }` and `struct PlainPerformer { ... }` plus `impl Perform for PlainPerformer { ... }`). Do not change the existing `styled_spans`, `StyledPerformer`, `ansi_color`, or `parse_extended_color` code. The `#[cfg(test)] mod tests;` at the end of the file remains unchanged. The `Perform` trait import was already imported in the `anstyle-parse` use line — no import duplication.

2. **In `crates/jackin-tui/src/prune_output.rs`**: Replace the entire current content (`//! Compatibility re-export …\n\npub use jackin_core::prune_output::*;`) with the full content of `crates/jackin-core/src/prune_output.rs` (lines 1–141), keeping the doc comment updated to remove the phrase "shared by runtime and diagnostics" if desired (or leave unchanged — structure-only). Add `#[cfg(test)] mod tests;` as the final line (replacing the existing `#[cfg(test)]\nmod tests;` that is already there at lines 140–141 in `jackin-core`'s version — copy it exactly).

3. **Create `crates/jackin-tui/src/prune_output/tests.rs`** with the exact content of `crates/jackin-core/src/prune_output/tests.rs` (39 lines). The `use super::*;` import on line 1 stays as-is. The `PendingRow { finalized: false }` struct literal on line 37 accesses the private `finalized` field; this is valid because `tests.rs` is declared as `mod tests` inside `prune_output.rs` (making it a child module with access to private fields).

4. **Create `crates/jackin-tui/src/url_text.rs`** with the following content — copy the non-test portion of `crates/jackin-core/src/url_text.rs` (lines 1–51, i.e., the module doc comment plus the three public functions `is_host_open_url`, `has_url_scheme`, `redact_url_for_log`) and append `#[cfg(test)] mod tests;` as the final line. `url_text.rs` uses no external crate imports (pure `std`-free string logic); no new `Cargo.toml` entry is needed for `jackin-tui`.

5. **Create `crates/jackin-tui/src/url_text/tests.rs`** with the following exact content:
   ```
   use super::redact_url_for_log;
   use super::is_host_open_url;
   use super::has_url_scheme;
   ```
   followed by the four test functions from the inline `mod tests { ... }` block at lines 53–96 of `crates/jackin-core/src/url_text.rs` (`redact_url_for_log_preserves_plain_url`, `redact_url_for_log_removes_query_and_fragment_payloads`, `host_open_url_policy_allows_http_https_mailto_only`, `has_url_scheme_detects_scheme_bearing_tokens`). Use explicit `use super::X` imports (three lines above), not `use super::*`, per the no-wildcard-imports rule.

6. **In `crates/jackin-tui/src/lib.rs`**: Add `pub mod url_text;` to the module declaration list (alphabetically after `pub mod theme;`, before any later entry, or after `pub mod terminal_modes;` — keep consistent alphabetical order with the existing list: `animation`, `ansi_text`, `components`, `geometry`, `host_colors`, `keymap`, `output`, `prune_output`, `runtime`, `scroll`, `terminal_modes`, `theme`). Insert `pub mod url_text;` after `pub mod theme;`.

7. **In `crates/jackin-core/src/lib.rs`**: Remove the three lines `pub mod ansi_text;` (line 12), `pub mod prune_output;` (line 29), `pub mod url_text;` (line 32). Adjust line numbering for surrounding modules accordingly. No other changes to the file.

8. **In `crates/jackin-core/Cargo.toml`**: Remove the `anstyle-parse = "1.0"` entry from `[dependencies]`. Remove the `owo-colors = { version = "4", features = ["supports-colors"] }` entry from `[dependencies]`. Verify no other file under `crates/jackin-core/src/` imports `anstyle_parse` or `owo_colors` before deleting — after step 7 removes the module declarations those imports will be gone.

9. **Delete `crates/jackin-core/src/ansi_text.rs`** (35 lines). The single test it contains (`strip_removes_sgr_sequences`) is already covered by `strips_ansi_sequences_from_bytes` in `crates/jackin-tui/src/ansi_text/tests.rs` — coverage is maintained.

10. **Delete `crates/jackin-core/src/ansi_text/tests.rs`** (10 lines). Directory `crates/jackin-core/src/ansi_text/` becomes empty after this deletion; remove it.

11. **Delete `crates/jackin-core/src/prune_output.rs`** (142 lines).

12. **Delete `crates/jackin-core/src/prune_output/tests.rs`** (39 lines). Remove now-empty directory `crates/jackin-core/src/prune_output/`.

13. **Delete `crates/jackin-core/src/url_text.rs`** (97 lines, including inline `mod tests { … }` block).

14. **In `test-layout-allowlist.toml`**: Remove the entry `"crates/jackin-core/src/url_text.rs"` (line 22 in the current file). The file no longer exists after step 13, so keeping the entry would cause the xtask lint to fail with a "listed file not found" error.

15. **In `crates/jackin-capsule/src/daemon/input_dispatch.rs` line 42**: Change `jackin_core::url_text::is_host_open_url` → `jackin_tui::url_text::is_host_open_url`.

16. **In `crates/jackin-capsule/src/daemon/mouse_input.rs`** (8 occurrences at lines 681, 685, 690, 725, 726, 744, 765, 774): Replace all occurrences of `jackin_core::url_text::` with `jackin_tui::url_text::` (three distinct symbols: `is_host_open_url`, `redact_url_for_log`, `has_url_scheme`).

17. **In `crates/jackin-capsule/src/tui/run.rs` line 130**: Change `jackin_core::url_text::redact_url_for_log` → `jackin_tui::url_text::redact_url_for_log`.

18. **In `crates/jackin-runtime/src/runtime/host_attach.rs` line 272**: Change `jackin_core::url_text::redact_url_for_log` → `jackin_tui::url_text::redact_url_for_log`.

19. **In `crates/jackin-runtime/src/runtime/host_desktop.rs`** (lines 9 and 79): Change `jackin_core::url_text::redact_url_for_log` → `jackin_tui::url_text::redact_url_for_log` (line 9) and `jackin_core::url_text::is_host_open_url` → `jackin_tui::url_text::is_host_open_url` (line 79).

20. **TODO(investigate) — `crates/jackin-diagnostics/src/run.rs:35`**: This file imports `use jackin_core::{JackinPaths, ansi_text::strip_bytes, prune_output};`. After steps 7–13, both `ansi_text::strip_bytes` and `prune_output` will be absent from `jackin-core`. Adding `jackin-tui` to `jackin-diagnostics/Cargo.toml` is NOT acceptable because eight crates at L0/L1/L2 (`jackin-config`, `jackin-manifest`, `jackin-docker`, `jackin-env`, `jackin-image`, `jackin-term`, `jackin-runtime`, `jackin-capsule`) transitively depend on `jackin-diagnostics`, which would pull `jackin-tui` (L3 presentation, with `ratatui`) into the domain layer. The executor MUST resolve this before running steps 7–14. Two viable options: (a) inline a private `strip_bytes` copy and inline the two `prune_output` calls (lines 695–696: `prune_output::section(…)` and `prune_output::start(…)`) directly in `run.rs` using raw `std::io::Write` and `owo_colors::OwoColorize` — requires adding `owo-colors` and `anstyle-parse` to `jackin-diagnostics/Cargo.toml`; or (b) extract `prune_all_runs`'s presentation calls to a wrapper function supplied by the `jackin` CLI binary (moving presentation code up the call stack). Option (b) is architecturally cleaner but changes the public API of `jackin-diagnostics`.

21. **TODO(investigate) — `crates/jackin-protocol/src/attach.rs:1211`**: This calls `jackin_core::url_text::is_host_open_url(url)` inside the `TAG_HOST_OPEN_URL` frame decoder. `jackin-protocol` is L0 (wire-format types) and MUST NOT depend on `jackin-tui` (L3 presentation). The executor MUST resolve this before running step 13. Two options: (a) inline a private three-line `fn host_open_url_scheme_allowed(url: &str) -> bool { let lower = url.to_ascii_lowercase(); lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") }` directly in `crates/jackin-protocol/src/attach.rs`; or (b) keep a minimal `url_text` module in `jackin-core` containing only `is_host_open_url` (removing `has_url_scheme` and `redact_url_for_log` which can move to `jackin-tui`). Option (b) leaves partial rendering code in `jackin-core` and does not satisfy the "Done when" condition.

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exits 0, no formatting differences
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings (verifies that `jackin-core` no longer references `anstyle_parse` or `owo_colors`, that all `jackin_core::url_text::` paths in `jackin-capsule` and `jackin-runtime` are resolved to `jackin_tui::url_text::`, and that `jackin-tui`'s `prune_output` and `ansi_text` compile with their moved implementations)
- `cargo nextest run --workspace` → all tests pass (specifically: `jackin-tui`'s `ansi_text` tests include the merged `strip_bytes` coverage; `jackin-tui`'s `prune_output` tests for `pending_rows_align_status_column` and `complete_propagates_errors_after_finalizing_row` pass; `jackin-tui`'s `url_text` tests for all four URL cases pass)
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout gate passes; in particular the `test-layout-allowlist.toml` no longer lists the now-deleted `crates/jackin-core/src/url_text.rs`
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → run only if any affected file crossed a budget boundary; refresh `file-size-budget.toml` if `jackin-tui/src/prune_output.rs` grew over 2000 lines (current `prune_output` is 142 lines; combined with the existing 3-line stub it will be ~142 lines, well under cap)

**Done when:**
- `crates/jackin-core/src/lib.rs` contains no `pub mod ansi_text;`, `pub mod prune_output;`, or `pub mod url_text;` declarations
- `crates/jackin-core/Cargo.toml` lists neither `anstyle-parse` nor `owo-colors` in `[dependencies]`
- `crates/jackin-tui/src/ansi_text.rs` owns `strip_bytes` and `PlainPerformer` (no `pub use jackin_core::ansi_text::strip_bytes;` line)
- `crates/jackin-tui/src/prune_output.rs` owns the full `PendingRow`, `section`, `start`, `ok`, `skip`, `failed`, `pending_parts` implementations (no wildcard re-export from `jackin-core`)
- `crates/jackin-tui/src/url_text.rs` exists with `is_host_open_url`, `has_url_scheme`, `redact_url_for_log`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- `cargo nextest run --workspace` exits 0

**Rollback:** `git restore crates/jackin-core/src/lib.rs crates/jackin-core/Cargo.toml crates/jackin-tui/src/ansi_text.rs crates/jackin-tui/src/prune_output.rs crates/jackin-tui/src/lib.rs test-layout-allowlist.toml crates/jackin-capsule/src/daemon/input_dispatch.rs crates/jackin-capsule/src/daemon/mouse_input.rs crates/jackin-capsule/src/tui/run.rs crates/jackin-runtime/src/runtime/host_attach.rs crates/jackin-runtime/src/runtime/host_desktop.rs` and `git checkout -- crates/jackin-core/src/ansi_text.rs crates/jackin-core/src/ansi_text/tests.rs crates/jackin-core/src/prune_output.rs crates/jackin-core/src/prune_output/tests.rs crates/jackin-core/src/url_text.rs` and delete the newly created files `crates/jackin-tui/src/prune_output/tests.rs`, `crates/jackin-tui/src/url_text.rs`, `crates/jackin-tui/src/url_text/tests.rs`.

**Open questions:**
- How to migrate `crates/jackin-diagnostics/src/run.rs:35` (`use jackin_core::{JackinPaths, ansi_text::strip_bytes, prune_output}`) after both modules leave `jackin-core`. Adding `jackin-tui` to `jackin-diagnostics` is architecturally blocked because `jackin-config`, `jackin-manifest`, `jackin-docker`, `jackin-env`, `jackin-image`, `jackin-term` (all L0/L1/L2) depend on `jackin-diagnostics`. The executor must choose between inlining small private copies with `anstyle-parse` + `owo-colors` added to `jackin-diagnostics/Cargo.toml`, or refactoring `prune_all_runs` to push the presentation calls up to the CLI layer before running this slice.
- How to migrate `crates/jackin-protocol/src/attach.rs:1211` (`jackin_core::url_text::is_host_open_url`) after `url_text` leaves `jackin-core`. `jackin-protocol` is L0 domain and cannot depend on `jackin-tui` (L3). The executor must decide whether to inline a three-line private scheme-allowlist check into `attach.rs`, or to keep a minimal `url_text::is_host_open_url` stub in `jackin-core` (leaving partial rendering code behind and failing the "Done when" condition).
- Confirm that `crates/jackin-tui/src/url_text/tests.rs` uses explicit imports (`use super::redact_url_for_log; use super::is_host_open_url; use super::has_url_scheme;`) rather than `use super::*;` to comply with `clippy::wildcard_imports`. Check whether the existing `crates/jackin-tui/src/ansi_text/tests.rs` (which uses `use super::*;`) is currently passing under the clippy lint or is allowlisted.

---

### A4 — Move terminal-ownership state out of jackin-diagnostics (P7)

- **Goal:** Move the `RICH_SURFACE_ACTIVE` / `HOST_SCREEN_OWNED` atomics and all their accessor functions from `crates/jackin-diagnostics/src/terminal.rs` into a new `crates/jackin-tui/src/ownership.rs` module, then replace the diagnostics file with a thin re-export shim so every existing `jackin_diagnostics::*` call site compiles unchanged.
- **Preconditions:** none
- **Pattern:** Parallel Change (Expand: new home in `jackin-tui`; existing `jackin-diagnostics::terminal` becomes a re-export shim; no call-site edits needed in this PR)
- **Touches:**
  - `crates/jackin-tui/src/ownership.rs` — **created** (new module, receives moved content)
  - `crates/jackin-tui/src/lib.rs` — **modified** (adds `pub mod ownership;`)
  - `crates/jackin-diagnostics/Cargo.toml` — **modified** (adds `jackin-tui` dependency)
  - `crates/jackin-diagnostics/src/terminal.rs` — **modified** (body replaced with re-exports)
  - `crates/jackin-diagnostics/src/lib.rs` — **modified** (updates `//!` header, no pub-API change)
  - `crates/jackin-tui/src/animation.rs` — **modified** (three doc-comment lines updated)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Create** `crates/jackin-tui/src/ownership.rs` with the following exact content (moved verbatim from `crates/jackin-diagnostics/src/terminal.rs`, minus the `shorten_home` re-export line):

```rust
//! Terminal-ownership flags, alt-screen assertion, and terminal-title helpers.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static RICH_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set while a full-screen rich TUI owns the alternate screen.
///
/// Ancillary stderr status output — spinners, "waiting" lines — checks this
/// and stays silent so it cannot stream over the cockpit. Driven by the
/// renderer's lifetime, never by callers.
pub fn set_rich_surface_active(active: bool) {
    RICH_SURFACE_ACTIVE.store(active, Ordering::Relaxed);
}

#[must_use]
pub fn rich_surface_active() -> bool {
    RICH_SURFACE_ACTIVE.load(Ordering::Relaxed)
}

static HOST_SCREEN_OWNED: AtomicBool = AtomicBool::new(false);

/// Set while a single host-side guard owns the screen for a whole launch flow.
///
/// The guard holds the alternate screen, raw mode, and mouse capture across
/// console → loading → capsule → exit. The individual surfaces (console
/// manager, launch cockpit, exit outro) check this and skip their own
/// enter/leave so the flow never drops back to the cooked terminal between
/// screens. Driven only by the owning guard's lifetime.
pub fn set_host_screen_owned(owned: bool) {
    HOST_SCREEN_OWNED.store(owned, Ordering::Relaxed);
}

#[must_use]
pub fn host_screen_owned() -> bool {
    HOST_SCREEN_OWNED.load(Ordering::Relaxed)
}

/// True when any host-side full-screen surface owns terminal modes that make
/// direct stdout/stderr streaming unsafe.
///
/// `rich_surface_active` tracks a currently drawing cockpit/dialog. The host
/// guard can outlive an individual renderer while still holding raw mode,
/// mouse capture, and the alternate screen across console → launch → capsule.
/// Plain command output is equally corrupting in that gap.
#[must_use]
pub fn rich_terminal_owned() -> bool {
    rich_surface_active() || host_screen_owned()
}

/// Re-enter the host alternate screen after an interactive child returns.
///
/// A baked capsule still drops `?1049l` on detach and returns the terminal to
/// the primary screen; re-asserting the moment the `docker exec` returns means
/// the post-attach work (outcome inspection, the exit outro) renders on the
/// alternate screen instead of flashing the operator's shell. No-op unless a
/// host guard owns the screen.
pub fn reassert_alt_screen() {
    use crossterm::ExecutableCommand as _;
    if !host_screen_owned() {
        return;
    }
    let mut out = io::stdout();
    drop(out.execute(crossterm::terminal::EnterAlternateScreen));
    drop(out.execute(crossterm::cursor::Hide));
}

pub fn set_terminal_title(title: &str) {
    let mut stderr = io::stderr().lock();
    drop(write!(stderr, "\x1b]0;jackin❯ \u{00b7} {title}\x07"));
    drop(stderr.flush());
}
```

2. **Edit** `crates/jackin-tui/src/lib.rs`: after the existing `pub mod terminal_modes;` line (line 21), add:

   old_string:
   ```
   pub mod terminal_modes;
   pub mod theme;
   ```
   new_string:
   ```
   pub mod ownership;
   pub mod terminal_modes;
   pub mod theme;
   ```

3. **Edit** `crates/jackin-diagnostics/Cargo.toml`: add `jackin-tui` as a dependency. Insert the following line immediately after the `jackin-core` dependency line:

   old_string:
   ```
   jackin-core = { version = "0.6.0-dev", path = "../jackin-core" }
   ```
   new_string:
   ```
   jackin-core = { version = "0.6.0-dev", path = "../jackin-core" }
   jackin-tui = { version = "0.6.0-dev", path = "../jackin-tui" }
   ```

4. **Replace** the entire body of `crates/jackin-diagnostics/src/terminal.rs` with the following re-export shim (full file replacement):

```rust
//! Re-exports terminal-ownership helpers from `jackin-tui::ownership`.
//!
//! The authoritative state lives in `jackin_tui::ownership`; this module
//! re-exports it so existing `jackin_diagnostics::*` call sites compile
//! unchanged while `jackin-diagnostics` keeps observability only.

pub use jackin_core::shorten_home;
pub use jackin_tui::ownership::{
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title,
};
```

5. **Edit** `crates/jackin-diagnostics/src/lib.rs`: update the `//!` header on lines 1–3:

   old_string:
   ```
   //! Host observability substrate: structured JSONL run diagnostics, debug-mode
   //! flag, terminal-ownership guards, and the `debug_log!` macro.
   ```
   new_string:
   ```
   //! Host observability substrate: structured JSONL run diagnostics, debug-mode
   //! flag, and the `debug_log!` macro. Terminal-ownership guards are re-exported
   //! from `jackin_tui::ownership`.
   ```

6. **Edit** `crates/jackin-tui/src/animation.rs`: update three doc-comment lines that reference `jackin_diagnostics::host_screen_owned()`. These are the only occurrences in the file (lines 285, 294, 301 approximately — search for the string `jackin_diagnostics::host_screen_owned()`):

   Replace every occurrence of:
   ```
   /// `host_screen_owned` should be `jackin_diagnostics::host_screen_owned()`.
   ```
   with:
   ```
   /// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
   ```
   (There are exactly 3 occurrences; use replace_all.)

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exits 0, no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exits 0, no warnings
- `cargo nextest run --workspace` → all tests pass (including `jackin-diagnostics` and `jackin-tui` test suites); no test is rewritten
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK; arch gate informational (the new `jackin-diagnostics → jackin-tui` edge is not in FORBIDDEN_EDGES and does not create a cycle)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
- `crates/jackin-tui/src/ownership.rs` exists and contains the `RICH_SURFACE_ACTIVE` and `HOST_SCREEN_OWNED` atomics plus all seven public functions (`set_rich_surface_active`, `rich_surface_active`, `set_host_screen_owned`, `host_screen_owned`, `rich_terminal_owned`, `reassert_alt_screen`, `set_terminal_title`).
- `crates/jackin-diagnostics/src/terminal.rs` is a ≤12-line re-export shim with no `AtomicBool` definitions.
- All existing callers (`jackin_diagnostics::host_screen_owned()` in `jackin-runtime/src/runtime/attach.rs`, `jackin_diagnostics::rich_terminal_owned()` in `jackin-docker/src/shell_runner.rs`, etc.) compile unchanged without any import edits.

**Rollback:** `git restore crates/jackin-tui/src/lib.rs crates/jackin-diagnostics/Cargo.toml crates/jackin-diagnostics/src/terminal.rs crates/jackin-diagnostics/src/lib.rs crates/jackin-tui/src/animation.rs && git rm crates/jackin-tui/src/ownership.rs`

**Open questions:**
- Should the new `jackin-diagnostics → jackin-tui` dependency edge (L2 infrastructure → L3 presentation) be added to `FORBIDDEN_EDGES` in `crates/jackin-xtask/src/arch.rs` as part of A5, or is it acceptable as a transitional edge? The roadmap's A5 section does not list this edge explicitly; the executor must NOT guess — flag for operator decision before A5 lands.
- The `//!` Architecture-Invariant header requirement applies to new **crates** per HOUSE RULES; `ownership.rs` is a new **module** in an existing crate (`jackin-tui`) and does not need one. If the reviewer disagrees and wants one, add a line such as `//! Architecture Invariant: depends only on `crossterm` and `std`; no `jackin-*` deps.` to `ownership.rs`.

---

### A5 — cargo-deny dependency-direction bans + per-crate invariant headers

- **Goal:** Lock the now-fixed W4 forbidden edges permanently via `cargo-deny` `wrappers` bans in `deny.toml`, add `//!` Architecture-Invariant headers to every crate whose layer boundary changed in A0–A4, and flip the arch gate to strict mode in CI so no re-inversion can land silently.
- **Preconditions:** A1, A2, A3, A4 — all five formerly forbidden edges must be absent from the live dep graph and their entries must already have been removed from `FORBIDDEN_EDGES` by their respective fix PRs, leaving `FORBIDDEN_EDGES = &[]`.
- **Pattern:** config/CI edit (deny.toml + arch.rs comment + ci.yml + crate lib.rs headers)
- **Touches:**
  - `deny.toml` (modified)
  - `crates/jackin-xtask/src/arch.rs` (modified — module doc + FORBIDDEN_EDGES comment)
  - `crates/jackin-xtask/src/arch/tests.rs` (modified — update test to match empty FORBIDDEN_EDGES)
  - `crates/jackin-xtask/src/main.rs` (modified — run_all_lints doc comment)
  - `.github/workflows/ci.yml` (modified — add `--strict` flag)
  - `crates/jackin-env/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-docker/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-runtime/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-config/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-manifest/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-diagnostics/src/lib.rs` (modified — add Architecture-Invariant header)
  - `crates/jackin-launch/src/lib.rs` (modified — add Architecture-Invariant header)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Verify precondition**: Run `cargo run -p jackin-xtask --locked -- lint arch --dump` and confirm that none of the five formerly forbidden edges appear in the output. If any edge still appears, STOP — the corresponding A1–A4 slice has not landed and this A5 cannot proceed.

2. **Verify FORBIDDEN_EDGES is empty**: Read `crates/jackin-xtask/src/arch.rs` lines 34–45. Confirm `FORBIDDEN_EDGES` is now `&[]` (all three entries removed by A2 and A4). If it still contains entries, STOP — A2/A4 have not completed.

3. **Update `deny.toml` — add `wrappers`-based bans for presentation crates**: In `/Users/donbeave/Projects/jackin-project/jackin/deny.toml`, in the `[bans]` section, find the existing `deny = [` block (currently lines 113–116) and replace it with the following (exact replacement):

   Old:
   ```toml
   deny = [
       { crate = "openssl", reason = "Prefer rustls/aws-lc-rs-backed TLS in this workspace." },
       { crate = "yaml-rust", reason = "Use serde_yaml_ng for YAML parsing." },
   ]
   ```

   New:
   ```toml
   deny = [
       { crate = "openssl", reason = "Prefer rustls/aws-lc-rs-backed TLS in this workspace." },
       { crate = "yaml-rust", reason = "Use serde_yaml_ng for YAML parsing." },
       # W4 dependency-direction bans (codebase-health-enforcement A5).
       # These mirror the formerly-informational FORBIDDEN_EDGES in arch.rs, now promoted to
       # hard cargo-deny bans so that fixed inversions cannot regress. The `wrappers` field
       # lists every crate that is *currently* allowed to directly depend on the banned crate;
       # any new direct consumer not in this list causes a CI failure. Update the wrappers
       # list when a new presentation- or infra-layer consumer is intentionally added.
       #
       # jackin-tui (L3 presentation): only presentation and entry/glue crates may depend on it.
       # The formerly-forbidden edge jackin-runtime → jackin-tui (fixed in A2) is not in wrappers.
       { crate = "jackin-tui", wrappers = ["jackin", "jackin-launch", "jackin-console", "jackin-capsule", "jackin-tui-lookbook"], reason = "Presentation crate; only L3/L4 crates may depend on it directly (W4)." },
       # jackin-launch (L3 presentation): only entry/glue crates may depend on it.
       # The formerly-forbidden edges jackin-env/jackin-docker/jackin-runtime → jackin-launch
       # (fixed in A0/A1/A2) are not in wrappers.
       # TODO(investigate): confirm the exact wrappers list after A1/A2 land.
       # After A1/A2, jackin-runtime injects the jackin-launch UI via a port trait owned by
       # jackin-core; the jackin binary (L4) is expected to hold the only direct dep.
       # Add any other L4 entry crate that wires the launch cockpit here.
       { crate = "jackin-launch", wrappers = ["jackin"], reason = "Presentation crate; only L4 entry crates may depend on it directly (W4)." },
       # jackin-diagnostics (L2 infra): domain crates (L0) must not depend on it.
       # The formerly-forbidden edges jackin-config → jackin-diagnostics and
       # jackin-manifest → jackin-diagnostics (fixed in A4) are not in wrappers.
       { crate = "jackin-diagnostics", wrappers = ["jackin", "jackin-capsule", "jackin-console", "jackin-docker", "jackin-env", "jackin-image", "jackin-launch", "jackin-runtime", "jackin-term"], reason = "Infrastructure crate; L0 domain crates (jackin-core, jackin-config, jackin-manifest, jackin-protocol) must not depend on it (W4)." },
   ]
   ```

4. **Update `crates/jackin-xtask/src/arch.rs` — replace module doc comment**: Replace the existing module-level `//!` block (lines 1–22) with the following:

   Old (exact text to match):
   ```rust
   //! Workspace dependency-direction check (Workstream 4 of
   //! `codebase-health-enforcement`).
   //!
   //! Walks `cargo metadata`'s resolved dep graph and asserts that no
   //! workspace crate depends on a layer it shouldn't. The forbidden edges
   //! are the P2 inverted-dependency rows in the architecture map:
   //!
   //! | From → To | Why forbidden |
   //! | --- | --- |
   //! | jackin-env → jackin-launch | launch is a TUI; env is infra. |
   //! | jackin-docker → jackin-launch | docker is infra; launch is a TUI. |
   //! | jackin-runtime → jackin-tui | runtime is infra; tui is presentation. |
   //! | jackin-config → jackin-diagnostics | config is domain; diagnostics carries presentation concerns. |
   //! | jackin-manifest → jackin-diagnostics | same as config. |
   //!
   //! Inverted edges trip the gate even if the original motivation has
   //! been removed — the bans are the lasting change, the exceptions are
   //! tracked in the roadmap item.
   //!
   //! ```sh
   //! cargo xtask lint arch
   //! ```
   ```

   New:
   ```rust
   //! Workspace dependency-direction check (Workstream 4 of
   //! `codebase-health-enforcement`).
   //!
   //! Walks `cargo metadata`'s resolved dep graph and asserts that no
   //! workspace crate depends on a layer it shouldn't.
   //!
   //! **A5 status:** all five formerly-forbidden P2 edges have been fixed
   //! (A0–A4) and promoted to hard `cargo-deny` `wrappers` bans in `deny.toml`.
   //! `FORBIDDEN_EDGES` is now empty; the gate runs in `--strict` mode in CI
   //! and exits 0 trivially (nothing to check). Add future inversions here as
   //! informational entries first; remove them and add a `deny.toml` ban once
   //! each inversion is fixed.
   //!
   //! ```sh
   //! cargo xtask lint arch
   //! ```
   ```

5. **Update `crates/jackin-xtask/src/arch.rs` — replace FORBIDDEN_EDGES comment**: Replace the comment block above and including the `FORBIDDEN_EDGES` constant (lines 32–45) with:

   Old (exact text):
   ```rust
   /// Forbid edges (from, to). `from` is not allowed to depend on `to`.
   /// Stored as `(from, to)` so symmetric blocks are easy to read.
   const FORBIDDEN_EDGES: &[(&str, &str)] = &[
       // Domain infra lifting logs into the diagnostics sink. Will move to a
       // port-trait indirection once the debug telemetry refactor lands — the
       // log calls themselves stay, only the layer edge flips.
       ("jackin-config", "jackin-diagnostics"),
       ("jackin-manifest", "jackin-diagnostics"),
       // Presentation/infra leak — runtime owns the bootstrap pipeline and
       // currently reaches upward into the TUI for the build log and launch
       // view. W1 follow-up adds a port trait that runtime owns and the TUI
       // subscribes to.
       ("jackin-runtime", "jackin-tui"),
   ];
   ```

   New:
   ```rust
   /// Forbid edges (from, to). `from` is not allowed to depend on `to`.
   /// Stored as `(from, to)` so symmetric blocks are easy to read.
   ///
   /// As of A5 this list is empty: all P2 inversions have been fixed and their
   /// bans are now encoded as `cargo-deny` `wrappers` entries in `deny.toml`.
   /// Add future inversions here as informational entries during cleanup; once
   /// fixed, remove the entry and add the corresponding `deny.toml` ban.
   const FORBIDDEN_EDGES: &[(&str, &str)] = &[];
   ```

6. **Update `crates/jackin-xtask/src/arch/tests.rs` — replace the first test** to match the empty `FORBIDDEN_EDGES`. The current test `synthetic_graph_flags_only_listed_forbidden_edges` asserts three problems, which will now always be zero. Replace the entire content of the file with:

   ```rust
   use super::*;
   use std::collections::{BTreeMap, BTreeSet};

   /// The forbidden-edge list is empty after A5 (all P2 inversions fixed and
   /// promoted to `cargo-deny` bans). Verify the gate passes on any dep graph.
   #[test]
   fn empty_forbidden_edges_always_passes() {
       let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
       // Simulate the formerly-forbidden edges being present — gate must still
       // pass because FORBIDDEN_EDGES is empty.
       deps.insert(
           "jackin-runtime".into(),
           BTreeSet::from(["jackin-tui".into()]),
       );
       deps.insert(
           "jackin-config".into(),
           BTreeSet::from(["jackin-diagnostics".into()]),
       );
       let mut problems = Vec::new();
       for (from, to) in FORBIDDEN_EDGES {
           if let Some(actual) = deps.get(*from)
               && actual.contains(*to)
           {
               problems.push(format!("{from} → {to}"));
           }
       }
       assert!(problems.is_empty(), "expected no forbidden edges, got: {problems:?}");
   }

   #[test]
   fn synthetic_graph_passes_when_clean() {
       let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
       deps.insert(
           "jackin-runtime".into(),
           BTreeSet::from(["jackin-core".into(), "jackin-config".into()]),
       );
       deps.insert(
           "jackin-config".into(),
           BTreeSet::from(["jackin-core".into()]),
       );
       deps.insert(
           "jackin-manifest".into(),
           BTreeSet::from(["jackin-core".into()]),
       );
       let mut problems = Vec::new();
       for (from, to) in FORBIDDEN_EDGES {
           if let Some(actual) = deps.get(*from)
               && actual.contains(*to)
           {
               problems.push(format!("{from} → {to}"));
           }
       }
       assert!(problems.is_empty());
   }
   ```

7. **Update `crates/jackin-xtask/src/main.rs` — update `run_all_lints` doc comment**: Replace the existing comment above `run_all_lints` (lines 107–110):

   Old (exact text):
   ```rust
   /// Run every codebase-health lint gate in sequence — the `cargo xtask lint`
   /// (no subcommand) entry point used by CI. The file-size ratchet and the
   /// test-file-layout rule always hard-fail on violations; the dependency-
   /// direction gate fails only in `strict` mode (informational otherwise, while
   /// the P2 inversions are still being cleaned up).
   fn run_all_lints(strict: bool) -> anyhow::Result<()> {
   ```

   New:
   ```rust
   /// Run every codebase-health lint gate in sequence — the `cargo xtask lint`
   /// (no subcommand) entry point used by CI. The file-size ratchet and the
   /// test-file-layout rule always hard-fail on violations; the dependency-
   /// direction gate also hard-fails (`--strict` is passed by CI since A5,
   /// as all P2 inversions are fixed and `cargo-deny` bans enforce the edges).
   fn run_all_lints(strict: bool) -> anyhow::Result<()> {
   ```

8. **Update `.github/workflows/ci.yml` — flip to `--strict` and update comment**: At line 434–441, replace:

   Old (exact text):
   ```yaml
      # Gate: all codebase-health lint checks via the umbrella command —
      #   • file-size ratchet (file-size-budget.toml: production ≤2000L, tests ≤10000L)
      #   • test-file-layout ratchet (test-layout-allowlist.toml: one sibling tests.rs)
      #   • dependency-direction (informational, exits 0, until the P2 inversions
      #     are cleaned up; `lint --strict` is the eventual hard mode).
      # Grandfathered entries in each ratchet file may only ever shrink. See
      # `roadmap/codebase-health-enforcement`.
      - run: cargo run -p jackin-xtask --locked -- lint
   ```

   New:
   ```yaml
      # Gate: all codebase-health lint checks via the umbrella command —
      #   • file-size ratchet (file-size-budget.toml: production ≤2000L, tests ≤10000L)
      #   • test-file-layout ratchet (test-layout-allowlist.toml: one sibling tests.rs)
      #   • dependency-direction (strict mode since A5: all P2 inversions fixed,
      #     cargo-deny wrappers bans in deny.toml enforce the lasting edge constraints).
      # Grandfathered entries in each ratchet file may only ever shrink. See
      # `roadmap/codebase-health-enforcement`.
      - run: cargo run -p jackin-xtask --locked -- lint --strict
   ```

9. **Update `crates/jackin-env/src/lib.rs` — add Architecture-Invariant header**: Replace the existing `//!` block at the top of the file (the 5-line block ending with `**Dependency tier:**`):

   Old (exact text):
   ```rust
   //! jackin-env: operator-env resolution and 1Password CLI integration.
   //!
   //! **Phase 3 (current):** Full `operator_env` stack extracted here.
   //!
   //! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env`
   ```

   New:
   ```rust
   //! jackin-env: operator-env resolution and 1Password CLI integration.
   //!
   //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
   //! `jackin-env` is an **application** crate (L1). Allowed workspace dependencies:
   //! `jackin-core` (L0 domain) · `jackin-config` (L0 domain) · `jackin-protocol` (L0 domain) ·
   //! `jackin-diagnostics` (L2 infra).
   //! Must **not** depend on any presentation crate: `jackin-tui`, `jackin-launch`, `jackin-console`.
   //!
   //! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env`
   ```

10. **Update `crates/jackin-docker/src/lib.rs` — add Architecture-Invariant header**: Replace the existing single-line `//!` comment:

    Old (exact text):
    ```rust
    //! Concrete Docker daemon and subprocess runner for jackin❯.
    ```

    New:
    ```rust
    //! Concrete Docker daemon and subprocess runner for jackin❯.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-docker` is an **infrastructure** crate (L2). Allowed workspace dependencies:
    //! `jackin-core` (L0 domain) · `jackin-diagnostics` (L2 infra, same layer) ·
    //! `jackin-build-meta` (build metadata).
    //! Must **not** depend on any presentation crate: `jackin-tui`, `jackin-launch`, `jackin-console`.
    ```

11. **Update `crates/jackin-runtime/src/lib.rs` — add Architecture-Invariant header**: Replace the existing `//!` block (the 7-line block ending with `**Dependency tier:**`):

    Old (exact text):
    ```rust
    //! jackin-runtime: container bootstrap pipeline.
    //!
    //! Holds the concrete `DockerApi` / `CommandRunner` implementations,
    //! image build, `DinD` sidecar management, mount materialization, and
    //! instance lifecycle.
    //!
    //! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env` → `jackin-runtime`
    ```

    New:
    ```rust
    //! jackin-runtime: container bootstrap pipeline.
    //!
    //! Holds the concrete `DockerApi` / `CommandRunner` implementations,
    //! image build, `DinD` sidecar management, mount materialization, and
    //! instance lifecycle.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-runtime` is an **application** crate (L1). Allowed workspace dependencies:
    //! `jackin-core` (L0) · `jackin-config` (L0) · `jackin-env` (L1) · `jackin-manifest` (L0) ·
    //! `jackin-docker` (L2) · `jackin-image` (L1) · `jackin-diagnostics` (L2) ·
    //! `jackin-protocol` (L0) · `jackin-build-meta` (build metadata).
    //! Must **not** depend on any presentation crate: `jackin-tui`, `jackin-launch`, `jackin-console`.
    //!
    //! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env` → `jackin-runtime`
    ```

12. **Update `crates/jackin-config/src/lib.rs` — add Architecture-Invariant header**: Replace the existing `//!` block (7-line block from `jackin-config:` through `workspace resolution.`):

    Old (exact text):
    ```rust
    //! jackin-config: configuration schema and workspace resolution.
    //!
    //! Merges the `config/` and `workspace/` modules into one crate to dissolve
    //! the config↔workspace mutual cycle that prevented crate extraction. Depends
    //! on `jackin-core` for the shared vocabulary types (`Agent`, `AuthForwardMode`,
    //! `MountIsolation`) and provides everything above: `AppConfig`, `WorkspaceConfig`,
    //! migrations, the config editor, and workspace resolution.
    ```

    New:
    ```rust
    //! jackin-config: configuration schema and workspace resolution.
    //!
    //! Merges the `config/` and `workspace/` modules into one crate to dissolve
    //! the config↔workspace mutual cycle that prevented crate extraction. Depends
    //! on `jackin-core` for the shared vocabulary types (`Agent`, `AuthForwardMode`,
    //! `MountIsolation`) and provides everything above: `AppConfig`, `WorkspaceConfig`,
    //! migrations, the config editor, and workspace resolution.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-config` is a **domain** crate (L0). Allowed workspace dependencies:
    //! `jackin-core` (L0 domain) only.
    //! Must **not** depend on `jackin-diagnostics` (L2 infra) or any presentation crate.
    ```

13. **Update `crates/jackin-manifest/src/lib.rs` — add Architecture-Invariant header**: Replace the existing single-line `//!` comment:

    Old (exact text):
    ```rust
    //! Role manifest loading, validation, and migration.
    ```

    New:
    ```rust
    //! Role manifest loading, validation, and migration.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-manifest` is a **domain** crate (L0). Allowed workspace dependencies:
    //! `jackin-core` (L0 domain) · `jackin-config` (L0 domain).
    //! Must **not** depend on `jackin-diagnostics` (L2 infra) or any presentation crate.
    ```

14. **Update `crates/jackin-diagnostics/src/lib.rs` — add Architecture-Invariant header**: Replace the existing single-line `//!` comment:

    Old (exact text):
    ```rust
    //! Host observability substrate: structured JSONL run diagnostics, debug-mode
    //! flag, terminal-ownership guards, and the `debug_log!` macro.
    ```

    New:
    ```rust
    //! Host observability substrate: structured JSONL run diagnostics, debug-mode
    //! flag, terminal-ownership guards, and the `debug_log!` macro.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-diagnostics` is an **infrastructure** crate (L2). Allowed workspace dependencies:
    //! `jackin-core` (L0 domain).
    //! Must **not** be depended on by domain (L0) crates: `jackin-core`, `jackin-config`,
    //! `jackin-manifest`, `jackin-protocol`. Those crates use a port-trait abstraction defined
    //! in `jackin-core`; `jackin-diagnostics` provides the concrete adapter.
    ```

15. **Update `crates/jackin-launch/src/lib.rs` — add Architecture-Invariant header**: The current `//!` block starts with `//! Launch progress surface model and UI ownership.`. Replace the first 4 lines of that block (before the `use` statement):

    Old (exact text):
    ```rust
    //! Launch progress surface model and UI ownership.
    //!
    //! This crate owns the launch cockpit boundary. Non-visual launch
    //! orchestration lives in `progress`, build-log capture lives in `build_log`,
    //! and model/message/update/run/view code lives under `tui`.
    ```

    New:
    ```rust
    //! Launch progress surface model and UI ownership — the launch-cockpit TUI.
    //!
    //! This crate owns the launch cockpit boundary. Non-visual launch
    //! orchestration lives in `progress`, build-log capture lives in `build_log`,
    //! and model/message/update/run/view code lives under `tui`.
    //!
    //! **Architecture Invariant** (W4 — enforced by `cargo-deny` + `cargo xtask lint arch --strict`):
    //! `jackin-launch` is a **presentation** crate (L3). Allowed workspace dependencies:
    //! `jackin-core` (L0 domain) · `jackin-diagnostics` (L2 infra) · `jackin-tui` (L3, same layer).
    //! Only **entry/glue** crates (L4: `jackin`, `jackin-capsule`) may directly depend on
    //! `jackin-launch`. Infrastructure and application crates (L1/L2) must not.
    ```

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no output (format is unchanged; all edits are `//!` comments, const values, and TOML)
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-xtask` → all tests pass (including the updated arch/tests.rs)
- `cargo run -p jackin-xtask --locked -- lint --strict` → `arch gate OK — N workspace deps checked, 0 forbidden edges not crossed` (strict passes because FORBIDDEN_EDGES is empty)
- `cargo deny check bans` → no violations (the new `wrappers` entries match the actual direct consumers of `jackin-tui`, `jackin-launch`, `jackin-diagnostics` after A1–A4)
- `cargo run -p jackin-xtask --locked -- lint` → all gates pass (file-size, test-layout, arch)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
- `cargo run -p jackin-xtask --locked -- lint --strict` exits 0 with the arch-gate "OK" message
- `cargo deny check bans` exits 0
- All five formerly-forbidden edges (`jackin-env→jackin-launch`, `jackin-docker→jackin-launch`, `jackin-runtime→jackin-tui`, `jackin-config→jackin-diagnostics`, `jackin-manifest→jackin-diagnostics`) are absent from `cargo xtask lint arch --dump` output
- `deny.toml` has three new `wrappers` ban entries for `jackin-tui`, `jackin-launch`, `jackin-diagnostics`
- `FORBIDDEN_EDGES` in `crates/jackin-xtask/src/arch.rs` is `&[]`
- `.github/workflows/ci.yml` calls `cargo run -p jackin-xtask --locked -- lint --strict`
- All seven affected `lib.rs` files have `//! **Architecture Invariant**` sections naming allowed and forbidden workspace deps

**Rollback:** `git restore deny.toml crates/jackin-xtask/src/arch.rs crates/jackin-xtask/src/arch/tests.rs crates/jackin-xtask/src/main.rs .github/workflows/ci.yml crates/jackin-env/src/lib.rs crates/jackin-docker/src/lib.rs crates/jackin-runtime/src/lib.rs crates/jackin-config/src/lib.rs crates/jackin-manifest/src/lib.rs crates/jackin-diagnostics/src/lib.rs crates/jackin-launch/src/lib.rs`

**Open questions:**
1. **`jackin-launch` wrappers list**: After A1/A2, the only direct consumer of `jackin-launch` in `Cargo.toml` will be the entry crate that injects the launch-cockpit UI into the runtime via the port trait. Current code shows `jackin-runtime` is the only consumer; after A2 removes it, the `jackin` binary must add `jackin-launch` as a direct dep for injection. Confirm whether `jackin` binary's `Cargo.toml` is updated in A1/A2, and whether any other L4 entry crate (`jackin-capsule`) also needs `jackin-launch`. The `wrappers = ["jackin"]` value in Step 3 assumes only the `jackin` binary is a direct consumer post-A2; if `jackin-capsule` also needs it, add it to the wrappers list.
2. **State of `arch/tests.rs` after A2 and A4**: Steps 2 and 6 assume FORBIDDEN_EDGES is already `&[]` when A5 runs, meaning A2 removed `("jackin-runtime","jackin-tui")` and A4 removed the `jackin-config`/`jackin-manifest` entries. If A2/A4 left FORBIDDEN_EDGES non-empty and also did not update `arch/tests.rs`, the executor must first verify the actual state and adapt Steps 5–6 accordingly (the test currently asserts 3 problems; that test must be updated by whichever PR last touches FORBIDDEN_EDGES, which may be A5 itself rather than A2/A4).
3. **`jackin-runtime` dep list in Step 11**: The header written in Step 11 lists `jackin-launch` as allowed. After A2 removes the `jackin-runtime → jackin-launch` dep, `jackin-launch` must be removed from the allowed list too. Confirm the final `Cargo.toml` of `jackin-runtime` after A2 and adjust the invariant header in Step 11 accordingly — drop `jackin-launch` from the allowed-deps list if it is no longer a direct dep.

---

### B1 — Rename jackin-launch → jackin-launch-tui (D4)

- **Goal:** Rename the `jackin-launch` crate (directory, package name, every dependent manifest, and every `jackin_launch::` use-path) to `jackin-launch-tui` so the name announces "launch cockpit TUI", resolving P3 from the codebase-health roadmap.
- **Preconditions:** A1 and A2 must be DONE first (value types leave `jackin-launch` before rename; roadmap B1 note: "Prereq: A1/A2 (types already left)"). If A1/A2 are not yet merged, this slice still compiles and behaves identically — only the crate name changes — but the precondition is the roadmap ordering.
- **Pattern:** Parallel Change (rename directory + manifest first, migrate all call-sites to the new name, verify).
- **Touches:**
  - Renamed: `crates/jackin-launch/` → `crates/jackin-launch-tui/` (entire directory)
  - Modified: `/Cargo.toml` (root workspace members)
  - Modified: `crates/jackin-launch-tui/Cargo.toml` (package name field)
  - Modified: `crates/jackin-runtime/Cargo.toml` (dependency name + path)
  - Modified: `crates/jackin-runtime/src/runtime/progress.rs` (all `jackin_launch::` → `jackin_launch_tui::`)
  - Modified: `crates/jackin-runtime/src/runtime/launch/restore.rs` (all `jackin_launch::` → `jackin_launch_tui::`)
  - Modified: `crates/jackin-runtime/src/runtime/launch/progress_helpers.rs` (one `jackin_launch::`)
  - Modified: `crates/jackin-runtime/src/runtime/launch/progress_helpers/tests.rs` (one `use jackin_launch::`)
  - Modified: `crates/jackin-runtime/src/isolation/finalize.rs` (one `jackin_launch::`)
  - Modified: `crates/jackin-runtime/src/isolation/git_inspect.rs` (three `jackin_launch::`)
  - Modified: `crates/jackin-launch-tui/src/lib.rs` (update `//!` Architecture-Invariant header)
  - Modified: `crates/jackin-xtask/src/arch.rs` (update two comment lines referencing `jackin-launch`)
  - Modified: `test-layout-allowlist.toml` (one path entry)
  - Modified: `.github/workflows/ci.yml` (one path filter)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Rename the crate directory using git:
   ```
   git mv crates/jackin-launch crates/jackin-launch-tui
   ```

2. In `crates/jackin-launch-tui/Cargo.toml`, change line 2 from:
   ```toml
   name = "jackin-launch"
   ```
   to:
   ```toml
   name = "jackin-launch-tui"
   ```

3. In `/Cargo.toml` (root), change the workspace members entry on line 16 from:
   ```toml
       "crates/jackin-launch",
   ```
   to:
   ```toml
       "crates/jackin-launch-tui",
   ```

4. In `crates/jackin-runtime/Cargo.toml`, change line 25 from:
   ```toml
   jackin-launch = { version = "0.6.0-dev", path = "../jackin-launch" }
   ```
   to:
   ```toml
   jackin-launch-tui = { version = "0.6.0-dev", path = "../jackin-launch-tui" }
   ```

5. In `crates/jackin-launch-tui/src/lib.rs`, replace the `//!` header block (lines 1–5):
   ```rust
   //! Launch progress surface model and UI ownership.
   //!
   //! This crate owns the launch cockpit boundary. Non-visual launch
   //! orchestration lives in `progress`, build-log capture lives in `build_log`,
   //! and model/message/update/run/view code lives under `tui`.
   ```
   with:
   ```rust
   //! Launch cockpit TUI — the presentation surface for `jackin load`.
   //!
   //! Architecture Invariant: this crate is a **presentation** crate.
   //! Allowed dependencies: `jackin-core`, `jackin-diagnostics`, `jackin-tui`.
   //! Infrastructure crates (`jackin-docker`, `jackin-env`, `jackin-runtime`)
   //! must NOT depend on this crate; use the port traits in `jackin-core` instead.
   //! Model/message/update/run/view code lives under `tui`; progress helpers
   //! live under `progress`.
   ```

6. In `crates/jackin-runtime/src/runtime/progress.rs`, replace every occurrence of `jackin_launch::` with `jackin_launch_tui::`. The affected lines are 11, 12, 13, 15, 17, 22, 24, 28, 32, 37, 39, 40, 139, 158, 170, 183, 185, 198, 199, 200, 209, 223. Also update the module doc comment on line 1 from:
   ```rust
   //! Re-export of `jackin-launch` progress types plus host-side prelaunch helpers.
   ```
   to:
   ```rust
   //! Re-export of `jackin-launch-tui` progress types plus host-side prelaunch helpers.
   ```
   Use a global find-and-replace of the literal string `jackin_launch::` → `jackin_launch_tui::` in this file only.

7. In `crates/jackin-runtime/src/runtime/launch/restore.rs`, replace every occurrence of `jackin_launch::` with `jackin_launch_tui::`. The affected lines are 17, 31, 53, 81, 85, 88, 91. Use a global find-and-replace of `jackin_launch::` → `jackin_launch_tui::` in this file only.

8. In `crates/jackin-runtime/src/runtime/launch/progress_helpers.rs`, replace the single occurrence on line 35:
   ```rust
               return Err(jackin_launch::LaunchCancelled::err());
   ```
   with:
   ```rust
               return Err(jackin_launch_tui::LaunchCancelled::err());
   ```

9. In `crates/jackin-runtime/src/runtime/launch/progress_helpers/tests.rs`, replace line 3:
   ```rust
   use jackin_launch::{LaunchCancelled, LaunchDiagnostics};
   ```
   with:
   ```rust
   use jackin_launch_tui::{LaunchCancelled, LaunchDiagnostics};
   ```

10. In `crates/jackin-runtime/src/isolation/finalize.rs`, replace the single occurrence on line 207:
    ```rust
        let worktrees_per_record: Vec<Vec<jackin_launch::WorktreeInspect>> = records
    ```
    with:
    ```rust
        let worktrees_per_record: Vec<Vec<jackin_launch_tui::WorktreeInspect>> = records
    ```

11. In `crates/jackin-runtime/src/isolation/git_inspect.rs`, replace every occurrence of `jackin_launch::` with `jackin_launch_tui::`. The affected lines are 70, 73, 80. Use a global find-and-replace of `jackin_launch::` → `jackin_launch_tui::` in this file only.

12. In `crates/jackin-xtask/src/arch.rs`, update lines 10–11 in the module-level `//!` docstring from:
    ```
    //! | jackin-env → jackin-launch | launch is a TUI; env is infra. |
    //! | jackin-docker → jackin-launch | docker is infra; launch is a TUI. |
    ```
    to:
    ```
    //! | jackin-env → jackin-launch-tui | launch-tui is a TUI; env is infra. |
    //! | jackin-docker → jackin-launch-tui | docker is infra; launch-tui is a TUI. |
    ```

13. In `test-layout-allowlist.toml`, replace the entry on line 24:
    ```toml
        "crates/jackin-launch/src/tui/view.rs",
    ```
    with:
    ```toml
        "crates/jackin-launch-tui/src/tui/view.rs",
    ```

14. In `.github/workflows/ci.yml`, replace line 122:
    ```yaml
              - 'crates/jackin-launch/**'
    ```
    with:
    ```yaml
              - 'crates/jackin-launch-tui/**'
    ```

15. Update `docs/content/docs/roadmap/codebase-health-enforcement.mdx`: replace every occurrence of the string `jackin-launch` that refers to the crate name (not to the concept of "launch") with `jackin-launch-tui`. Occurrences to update (crate-name references):
    - Line 17: `` `jackin-launch/src/tui/app.rs` `` → `` `jackin-launch-tui/src/tui/app.rs` ``
    - Line 18: `` `jackin-launch/src/build_log.rs` `` → `` `jackin-launch-tui/src/build_log.rs` ``, and `` `jackin-launch::build_log::*` `` → `` `jackin-launch-tui::build_log::*` ``
    - Line 83: `` `jackin-launch` (renamed/folded — see D4) `` → `` `jackin-launch-tui` (see D4) ``
    - Line 84 (D4 section header and body): all references to `jackin-launch` as the old crate name can stay as-is since they are historical; the result reference `jackin-launch-tui` is already in that paragraph.
    - Line 92 (D4 body): `rename it \`jackin-launch-tui\`` is already present — no change needed.
    - Line 225: `` `jackin-launch(-tui)` `` — change to `` `jackin-launch-tui` ``
    - Line 233: `` **B1 — rename `jackin-launch` → `jackin-launch-tui` (D4).** `` — already names the target; add `[x]` to check it off when this PR merges.
    - Do NOT change the prose description "rename `jackin-launch` → `jackin-launch-tui`" in the B1 bullet (it names the rename action) — only update references to the crate in its new state.

16. Update `docs/content/docs/reference/getting-oriented/codebase-map.mdx`: replace all crate-name occurrences of `jackin-launch` with `jackin-launch-tui` and update any path references from `crates/jackin-launch/` to `crates/jackin-launch-tui/`.

17. Update `docs/content/docs/reference/tui/architecture.mdx`, `docs/content/docs/reference/tui/navigation.mdx`, `docs/content/docs/reference/tui/chrome.mdx`, `docs/content/docs/reference/tui/dialogs.mdx`, `docs/content/docs/reference/adrs/adr-003-ratatui.mdx`, `docs/content/docs/reference/adrs/adr-001-single-crate-vs-workspace.mdx`, `docs/content/docs/reference/runtime/diagnostics.mdx`: in each file, replace crate-name occurrences of `jackin-launch` that refer to the renamed crate with `jackin-launch-tui`. Do NOT change occurrences that describe the launch concept rather than the crate name.

18. Update `docs/content/docs/roadmap/index.mdx`: replace the crate-name references `jackin-launch` → `jackin-launch-tui` where they name the crate (not the launch operation).

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → no formatting violations
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings; specifically confirms `jackin-launch-tui` resolves correctly as dependency of `jackin-runtime`
- `cargo nextest run -p jackin-launch-tui` → all tests pass
- `cargo nextest run -p jackin-runtime` → all tests pass (this exercises all migrated `jackin_launch_tui::` call sites in `progress.rs`, `restore.rs`, `progress_helpers.rs`, `finalize.rs`, `git_inspect.rs`)
- `cargo nextest run --workspace` → entire workspace green; no crate still names `jackin_launch` as a dependency
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all pass; specifically confirms `test-layout-allowlist.toml` path `crates/jackin-launch-tui/src/tui/view.rs` resolves
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → refresh budget if any file under `crates/jackin-launch-tui/` changed line count (it should not — files are unchanged)
- `cargo run -p jackin-xtask --locked -- lint tests --print-allowlist` → confirm `crates/jackin-launch-tui/src/tui/view.rs` is recognized (replaces old `crates/jackin-launch/` entry)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `cargo metadata --no-deps | grep '"name"' | grep jackin-launch` returns exactly `"name": "jackin-launch-tui"` and no result for `"jackin-launch"` (without the `-tui` suffix); `grep -r 'jackin_launch::' crates/` returns zero results; `grep -r '"crates/jackin-launch"' Cargo.toml` returns zero results.

**Rollback:** `git restore Cargo.toml crates/jackin-runtime/Cargo.toml crates/jackin-runtime/src/runtime/progress.rs crates/jackin-runtime/src/runtime/launch/restore.rs crates/jackin-runtime/src/runtime/launch/progress_helpers.rs crates/jackin-runtime/src/runtime/launch/progress_helpers/tests.rs crates/jackin-runtime/src/isolation/finalize.rs crates/jackin-runtime/src/isolation/git_inspect.rs crates/jackin-xtask/src/arch.rs test-layout-allowlist.toml .github/workflows/ci.yml` then `git mv crates/jackin-launch-tui crates/jackin-launch`.

**Open questions:** none — all paths, line numbers, and symbol names are confirmed from the real code.


## B2 — Delete the 19 binary shim modules (P1, D6)

---

### B2 — Delete the 19 binary shim modules (P1, D6)

#### Goal

Remove every `pub use jackin_X::*` pass-through file in `crates/jackin/src/`. After this slice the only modules declared in `crates/jackin/src/lib.rs` are modules that own real code (`app`, `cli`, `console`, `error`, `preflight`, `role_authoring`, `selector`, `tui`, `workspace`).

---

#### Preconditions

- Branch diverged from `main` at the commit that introduced the backend scaffolding (`8e50267`).
- `cargo nextest run -p jackin --lib --all-features` is green.
- No open PR touching any of the 19 shim files.

---

#### Pattern

**Parallel-change order per shim**

1. Update every call site in `crates/jackin/src/` and `crates/jackin/src/bin/` from `crate::<shim>::Symbol` to `jackin_<owner>::[sub::module::]Symbol`.
2. Update every integration test in `crates/jackin/tests/` from `jackin::<shim>::Symbol` to `jackin_<owner>::[sub::module::]Symbol`.
3. Add any missing owning crates to `[dev-dependencies]` in `crates/jackin/Cargo.toml` so integration tests can see them.
4. Migrate orphaned test files: move or delete shim child `tests.rs` files before deleting the parent shim.
5. Delete the shim file and remove its `pub mod <shim>;` entry from `lib.rs`.

Commit rule: one commit per shim file (or one commit per small cluster), push immediately after each. Keep `cargo check -p jackin` green at each commit.

**No logic changes.** Type signatures, function bodies, and test assertions are copied verbatim; only the import path changes.

---

#### Shim inventory sub-table

| # | Shim path (crates/jackin/src/) | Owning crate(s) | Child files (action) | `crates/jackin/src/` call-site files | `crates/jackin/tests/` call-site files |
|---|---|---|---|---|---|
| 1 | `agent.rs` | `jackin_core` (Agent, ParseAgentError); type alias `AgentChoiceState` — unused, delete with shim | — | `app.rs`, `app/config_cmd.rs`, `app/context.rs`, `app/context/tests.rs`, `app/load_cmd.rs`, `app/tests.rs`, `cli/prewarm.rs`, `cli/role.rs`, `cli/role/tests.rs`, `cli/tests.rs`, `cli/workspace.rs`, `cli/workspace/tests.rs`, `workspace/tests.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs`, `tests/sentinel_role.rs` |
| 2 | `agent_binary.rs` | `jackin_image::agent_binary` | — | `cli/prewarm.rs`, `bin/build_jackin_capsule.rs` | `tests/common.rs` |
| 3 | `binary_artifact.rs` | `jackin_image::binary_artifact` | — | `bin/build_jackin_capsule.rs` | — |
| 4 | `capsule_binary.rs` | `jackin_image::capsule_binary` | — | `cli/prewarm.rs`, `preflight.rs`, `bin/build_jackin_capsule.rs` | `tests/common.rs` |
| 5 | `config.rs` | `jackin_config` (all symbols); `crate::workspace::validate_workspace_config` cross-ref — no callers, silently drops; DriftDetection exception comment disappears naturally | — | `app.rs`, `app/config_cmd.rs`, `app/context.rs`, `app/helpers.rs`, `app/load_cmd.rs`, `app/restore.rs`, `app/token_cmd.rs`, `app/workspace_cmd.rs`, `cli/prewarm.rs`, `workspace/tests.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs`, `tests/frame_time.rs`, `tests/manager_flow.rs`, `tests/workspace_config_crud.rs` |
| 6 | `derived_image.rs` | `jackin_image::derived_image` | — | — | `tests/amp_launch.rs`, `tests/codex_launch.rs`, `tests/dind_e2e.rs` |
| 7 | `diagnostics.rs` | `jackin_diagnostics` | — | `app.rs`, `console/tui/run.rs` | — |
| 8 | `docker.rs` | `jackin_docker` (CommandRunner, RunOptions); `jackin_docker::shell_runner` (ShellRunner, redact_env_args) | — | `app.rs`, `app/load_cmd.rs`, `app/prune_cmd.rs`, `app/restore.rs`, `app/workspace_cmd.rs`, `cli/prewarm.rs`, `console/effects.rs`, `console/services.rs`, `console/tui/run.rs` | `tests/common.rs`, `tests/per_mount_isolation_e2e.rs` |
| 9 | `docker_client.rs` | `jackin_docker::docker_client` (BollardDockerClient, ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome); `jackin_runtime::runtime::test_support::FakeDockerClient` (test-only) | `docker_client/tests.rs` — DELETE (trivial `size_of` shim test, not present in owning crate; not worth porting) | `app.rs`, `app/context.rs`, `app/context/tests.rs`, `app/helpers.rs`, `app/load_cmd.rs`, `app/prune_cmd.rs`, `app/restore.rs`, `app/tests.rs`, `app/workspace_cmd.rs`, `cli/status.rs`, `console/services.rs`, `preflight.rs` | `tests/common.rs` |
| 10 | `env_model.rs` | `jackin_core::env_model` | `env_model/tests.rs` — MOVE to `jackin-core/src/env_model/tests.rs` (see Step 3) | `app/config_cmd.rs`, `app/workspace_cmd.rs` | — |
| 11 | `env_resolver.rs` | `jackin_env` (EnvPrompter, PromptResult, ResolvedEnv, resolve_env, resolve_env_with_overrides) | — | — | `tests/sentinel_role.rs` |
| 12 | `instance.rs` | `jackin_runtime::instance` (glob); sub-modules `manifest`, `naming` | — | `app.rs`, `app/context.rs`, `app/helpers.rs`, `app/load_cmd.rs`, `app/restore.rs`, `cli/status.rs`, `cli/usage.rs`, `console/services.rs`, `preflight.rs` | `tests/dind_e2e.rs` |
| 13 | `isolation.rs` | `jackin_runtime::isolation::{branch,cleanup,finalize,materialize,state}` (sub-module globs); `jackin_core::{MountIsolation, ParseMountIsolationError}` | `isolation/tests.rs` — MOVE to `jackin-core/src/isolation/tests.rs` (see Step 2) | `app/config_cmd.rs`, `app/context.rs`, `app/context/tests.rs`, `app/tests.rs`, `app/workspace_cmd.rs`, `cli/workspace.rs`, `preflight.rs`, `workspace/planner/tests.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs`, `tests/manager_flow.rs`, `tests/manager_flow/secrets.rs`, `tests/manager_flow/secrets/env_key.rs`, `tests/manager_flow/secrets/overrides.rs`, `tests/per_mount_isolation_e2e.rs`, `tests/workspace_config_crud.rs` |
| 14 | `manifest.rs` | `jackin_manifest::manifest` (AmpConfig, ClaudeConfig, ClaudeMarketplaceConfig, CodexConfig, EnvVarDecl, HookEntry, HooksConfig, IdentityConfig, KimiConfig, ManifestWarning, OpencodeConfig, RoleManifest, load_role_manifest); `jackin_core::env_model` (JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_ENV_NAME, JACKIN_ENV_VALUE — no callers outside manifest shim itself) | `manifest/migrations.rs`, `manifest/validate.rs`, `manifest/validate/tests.rs` — DELETE all three (duplicated verbatim in `jackin-manifest/src/migrations/`, `jackin-manifest/src/validate/`, `jackin-manifest/src/validate/tests.rs`) | `app/context.rs`, `role_authoring.rs` | `tests/agent_validation.rs`, `tests/migration_fixtures.rs`, `tests/sentinel_role.rs` |
| 15 | `operator_env.rs` | `jackin_env` (most symbols: OpRunner, resolve_env_value, is_valid_env_name, parse_host_ref, OpAccount, OpCache, OpField, OpItem, OpItemCreateParams, OpStructRunner, OpVault, OpWriteRunner, default_op_struct_runner, OpCli, CLAUDE_OAUTH_TOKEN_ENV, EnvLayer, lookup_operator_env_raw, merge_layers, print_launch_diagnostic, resolve_op_uri_to_ref, resolve_operator_env, resolve_operator_env_with, validate_reserved_names, test_support); `jackin_core` (EnvValue, FieldTarget, OpRef); `jackin_core::op_reference` (OpReferenceParts, parse_op_reference) | — | `app/config_cmd.rs`, `app/tests.rs`, `app/token_cmd.rs` | `tests/manager_flow.rs`, `tests/manager_flow/secrets.rs`, `tests/manager_flow/secrets/env_key.rs`, `tests/manager_flow/secrets/overrides.rs` |
| 16 | `paths.rs` | `jackin_core` (JackinPaths) | — | `app.rs`, `app/config_cmd.rs`, `app/context.rs`, `app/helpers.rs`, `app/load_cmd.rs`, `app/prune_cmd.rs`, `app/restore.rs`, `app/token_cmd.rs`, `app/workspace_cmd.rs`, `cli/diagnostics.rs`, `cli/doctor.rs`, `cli/prewarm.rs`, `cli/status.rs`, `cli/usage.rs`, `console/effects.rs`, `preflight.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs`, `tests/common.rs`, `tests/manager_flow.rs`, `tests/workspace_config_crud.rs` |
| 17 | `repo.rs` | `jackin_manifest::repo` (CachedRepo, RoleRepoValidationError, ValidatedRoleRepo, validate_role_repo) | `repo/tests.rs` — DELETE (duplicated in `jackin-manifest/src/repo/tests.rs`) | `app/context.rs`, `app/context/tests.rs`, `console/effects.rs`, `role_authoring.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs` |
| 18 | `repo_contract.rs` | `jackin_manifest::repo_contract` (BASE_DOCKERFILE_FROM, DOCKERFILE_NAME, MANIFEST_FILENAME, published_image_labels, published_image_repository); `jackin_manifest::repo::RoleRepoValidationError` (test-only re-export — move to owning-crate test path) | `repo_contract/tests.rs` — DELETE (duplicated in `jackin-manifest/src/repo_contract/tests.rs`) | `role_authoring.rs` | — |
| 19 | `runtime.rs` | `jackin_runtime::runtime` (all symbols and sub-modules: attach, drift, docker_profile, logs, naming, progress, snapshot); contains one real wrapper `register_agent_repo` — not a re-export, must be inlined at 2 call sites before shim deletion | — | `app.rs`, `app/context.rs`, `app/helpers.rs`, `app/load_cmd.rs`, `app/prune_cmd.rs`, `app/restore.rs`, `app/workspace_cmd.rs`, `cli/prewarm.rs`, `cli/role.rs`, `cli/usage.rs`, `console/effects.rs`, `console/services.rs` | `tests/amp_launch.rs`, `tests/codex_launch.rs` |

---

#### Touches

**Modified:**
- `crates/jackin/Cargo.toml` — add 4 entries to `[dev-dependencies]`
- `crates/jackin/src/lib.rs` — remove 19 `pub mod <shim>;` entries
- `crates/jackin-core/src/env_model.rs` — extract inline tests to sibling file
- `crates/jackin-core/src/isolation.rs` — add `#[cfg(test)] mod tests;`
- All `crates/jackin/src/` files listed in column 5 of the sub-table above (import path changes only)
- All `crates/jackin/tests/` files listed in column 6 of the sub-table above (import path changes only)
- `crates/jackin/src/bin/build_jackin_capsule.rs` — 3 import path changes

**Created:**
- `crates/jackin-core/src/isolation/tests.rs` — moved from `crates/jackin/src/isolation/tests.rs`
- `crates/jackin-core/src/env_model/tests.rs` — merged from inline tests in `jackin-core/src/env_model.rs` + `crates/jackin/src/env_model/tests.rs`

**Deleted (shims):**
`crates/jackin/src/agent.rs`, `agent_binary.rs`, `binary_artifact.rs`, `capsule_binary.rs`, `config.rs`, `derived_image.rs`, `diagnostics.rs`, `docker.rs`, `docker_client.rs`, `env_model.rs`, `env_resolver.rs`, `instance.rs`, `isolation.rs`, `manifest.rs`, `operator_env.rs`, `paths.rs`, `repo.rs`, `repo_contract.rs`, `runtime.rs`

**Deleted (shim children):**
`crates/jackin/src/manifest/migrations.rs`, `manifest/validate.rs`, `manifest/validate/tests.rs`, `docker_client/tests.rs`, `repo/tests.rs`, `repo_contract/tests.rs`

**Moved (and source deleted):**
`crates/jackin/src/isolation/tests.rs` → `crates/jackin-core/src/isolation/tests.rs`
`crates/jackin/src/env_model/tests.rs` → merged into `crates/jackin-core/src/env_model/tests.rs`

---

#### Steps

#### Step 1 — Add missing dev-dependencies to `crates/jackin/Cargo.toml`

Integration tests need the owning crates directly. Current `[dev-dependencies]` already has `jackin-config`, `jackin-env`, `jackin-runtime`. Add:

```toml
jackin-core     = { version = "0.6.0-dev", path = "../jackin-core" }
jackin-docker   = { version = "0.6.0-dev", path = "../jackin-docker" }
jackin-image    = { version = "0.6.0-dev", path = "../jackin-image" }
jackin-manifest = { version = "0.6.0-dev", path = "../jackin-manifest" }
```

Coverage: `jackin-core` for `JackinPaths`, `MountIsolation`, `EnvValue`, `Agent`; `jackin-docker` for `CommandRunner`, `RunOptions`, docker-client types; `jackin-image` for `derived_image`, `agent_binary`, `capsule_binary`; `jackin-manifest` for repo types, `manifest`, `migrations`, `validate`.

#### Step 2 — Migrate `isolation/tests.rs` to `jackin-core`

`crates/jackin/src/isolation/tests.rs` tests `MountIsolation` and `ParseMountIsolationError`, both owned by `jackin-core::isolation`. The file is self-contained (`use super::*;` only; 50 lines).

1. Create directory `crates/jackin-core/src/isolation/`.
2. Copy `crates/jackin/src/isolation/tests.rs` to `crates/jackin-core/src/isolation/tests.rs` verbatim — `use super::*;` resolves to `jackin_core::isolation::*` with no changes needed.
3. In `crates/jackin-core/src/isolation.rs` add at end of file: `#[cfg(test)] mod tests;`
4. Delete `crates/jackin/src/isolation/tests.rs`.

#### Step 3 — Migrate `env_model/tests.rs` to `jackin-core`

`jackin-core/src/env_model.rs` has two inline tests (lines 183–201) violating the "no inline tests" hard rule (crates/AGENTS.md). Merge with the shim tests being evicted.

1. Create directory `crates/jackin-core/src/env_model/`.
2. Create `crates/jackin-core/src/env_model/tests.rs`:
   - Start with `//! Tests for \`env_model\`.` header and `use super::*;`.
   - Copy the 2 existing inline test functions from `jackin-core/src/env_model.rs` (`open_links_allowed_accepts_unset_and_non_deny_values`, `open_links_allowed_rejects_deny_values`).
   - Append all tests from `crates/jackin/src/env_model/tests.rs` (lines 4–131). The call `crate::manifest::EnvVarDecl` in `topological_env_order_is_deterministic_for_independent_prompts` becomes `jackin_core::manifest::EnvVarDecl` in the new location — same type, zero change to source text.
3. In `crates/jackin-core/src/env_model.rs` replace the inline `#[cfg(test)] mod tests { … }` block (lines 183–201) with `#[cfg(test)] mod tests;`.
4. Delete `crates/jackin/src/env_model/tests.rs`.

#### Step 4 — Delete duplicate shim child test files

These are duplicated verbatim in the owning crate and carry no new coverage:

| File to delete | Canonical copy in owning crate |
|---|---|
| `crates/jackin/src/docker_client/tests.rs` | trivial `size_of` shim test; no copy needed |
| `crates/jackin/src/manifest/validate/tests.rs` | `crates/jackin-manifest/src/validate/tests.rs` (994 lines; superset) |
| `crates/jackin/src/repo/tests.rs` | `crates/jackin-manifest/src/repo/tests.rs` |
| `crates/jackin/src/repo_contract/tests.rs` | `crates/jackin-manifest/src/repo_contract/tests.rs` |

Then delete the child shim files with no tests:
`crates/jackin/src/manifest/migrations.rs`, `crates/jackin/src/manifest/validate.rs`.

#### Step 5 — Inline `register_agent_repo` wrapper at its 2 call sites

`runtime.rs` shim contains one non-re-export function. Inline its body before deleting the shim.

`crates/jackin/src/console/services.rs` line 574:
```rust
// before
crate::runtime::register_agent_repo(paths, selector, git_url, runner, debug).await?;
// after
jackin_runtime::runtime::register_agent_repo(paths, selector, git_url, runner, debug).await?;
```

`crates/jackin/src/cli/prewarm.rs` line 293:
```rust
// before
let result = crate::runtime::register_agent_repo(
// after
let result = jackin_runtime::runtime::register_agent_repo(
```

#### Step 6 — Update imports in `crates/jackin/src/` production files

Global substitution rules (apply across all affected files; only paths change):

| Old path prefix | New path prefix |
|---|---|
| `crate::agent::` | `jackin_core::` |
| `crate::agent_binary::` | `jackin_image::agent_binary::` |
| `crate::binary_artifact::` | `jackin_image::binary_artifact::` |
| `crate::capsule_binary::` | `jackin_image::capsule_binary::` |
| `crate::config::` | `jackin_config::` |
| `crate::derived_image::` | `jackin_image::derived_image::` |
| `crate::diagnostics::` | `jackin_diagnostics::` |
| `crate::docker::CommandRunner` / `crate::docker::RunOptions` | `jackin_docker::CommandRunner` / `jackin_docker::RunOptions` |
| `crate::docker::ShellRunner` / `crate::docker::redact_env_args` | `jackin_docker::shell_runner::ShellRunner` / `jackin_docker::shell_runner::redact_env_args` |
| `crate::docker_client::` (non-test) | `jackin_docker::docker_client::` |
| `crate::docker_client::FakeDockerClient` (test) | `jackin_runtime::runtime::test_support::FakeDockerClient` |
| `crate::env_model::` | `jackin_core::env_model::` |
| `crate::env_resolver::` | `jackin_env::` |
| `crate::instance::` | `jackin_runtime::instance::` |
| `crate::instance::manifest::` | `jackin_runtime::instance::manifest::` |
| `crate::instance::naming::` | `jackin_runtime::instance::naming::` |
| `crate::isolation::MountIsolation` | `jackin_core::MountIsolation` |
| `crate::isolation::ParseMountIsolationError` | `jackin_core::ParseMountIsolationError` |
| `crate::isolation::branch::` | `jackin_runtime::isolation::branch::` |
| `crate::isolation::cleanup::` | `jackin_runtime::isolation::cleanup::` |
| `crate::isolation::finalize::` | `jackin_runtime::isolation::finalize::` |
| `crate::isolation::materialize::` | `jackin_runtime::isolation::materialize::` |
| `crate::isolation::state::` | `jackin_runtime::isolation::state::` |
| `crate::manifest::` (types/functions) | `jackin_manifest::manifest::` |
| `crate::manifest::JACKIN_*` constants | `jackin_core::env_model::JACKIN_*` |
| `crate::manifest::migrations::` | `jackin_manifest::migrations::` |
| `crate::manifest::validate::` | `jackin_manifest::validate::` |
| `crate::operator_env::EnvValue` / `crate::operator_env::FieldTarget` / `crate::operator_env::OpRef` | `jackin_core::EnvValue` / `jackin_core::FieldTarget` / `jackin_core::OpRef` |
| `crate::operator_env::OpReferenceParts` / `crate::operator_env::parse_op_reference` | `jackin_core::op_reference::OpReferenceParts` / `jackin_core::op_reference::parse_op_reference` |
| `crate::operator_env::` (all other symbols) | `jackin_env::` |
| `crate::paths::JackinPaths` | `jackin_core::JackinPaths` |
| `crate::repo::` | `jackin_manifest::repo::` |
| `crate::repo_contract::` (non-test) | `jackin_manifest::repo_contract::` |
| `crate::repo_contract::RoleRepoValidationError` (test) | `jackin_manifest::repo::RoleRepoValidationError` |
| `crate::runtime::` | `jackin_runtime::runtime::` |
| `crate::runtime::attach::` | `jackin_runtime::runtime::attach::` |
| `crate::runtime::drift::` | `jackin_runtime::runtime::drift::` |
| `crate::runtime::docker_profile::` | `jackin_runtime::runtime::docker_profile::` |
| `crate::runtime::logs::` | `jackin_runtime::runtime::logs::` |
| `crate::runtime::naming::` | `jackin_runtime::runtime::naming::` |
| `crate::runtime::progress::` | `jackin_runtime::runtime::progress::` |
| `crate::runtime::snapshot::` | `jackin_runtime::runtime::snapshot::` |
| `crate::runtime::FakeRunner` (test) | `jackin_runtime::runtime::test_support::FakeRunner` |
| `crate::runtime::test_support::` (test) | `jackin_runtime::runtime::test_support::` |

For files that declare `use crate::runtime;` or `use crate::runtime as runtime;` and then call `runtime::SomeSymbol`, replace the `use` with `use jackin_runtime::runtime;` — the call sites then need no further change.

#### Step 7 — Update `crates/jackin/src/bin/build_jackin_capsule.rs`

Three import paths to update:
```rust
// before
use jackin::binary_artifact::{chmod_executable, container_arch};
use jackin::capsule_binary::REQUIRED_VERSION;
use jackin::paths::JackinPaths;
// after
use jackin_image::binary_artifact::{chmod_executable, container_arch};
use jackin_image::capsule_binary::REQUIRED_VERSION;
use jackin_core::JackinPaths;
```

Note: `build_jackin_capsule.rs` is a `[[bin]]` target compiled as part of the `jackin` crate, so it can import from workspace sibling crates already in `[dependencies]`. No `Cargo.toml` change needed.

#### Step 8 — Update imports in `crates/jackin/tests/` integration tests

Apply the same substitution rules from Step 6, replacing the `jackin::` crate prefix with the owning crate prefix:

| Old | New |
|---|---|
| `jackin::agent::Agent` / `jackin::agent::ParseAgentError` | `jackin_core::Agent` / `jackin_core::ParseAgentError` |
| `jackin::agent_binary::` | `jackin_image::agent_binary::` |
| `jackin::capsule_binary::` | `jackin_image::capsule_binary::` |
| `jackin::config::` | `jackin_config::` |
| `jackin::derived_image::` | `jackin_image::derived_image::` |
| `jackin::docker::CommandRunner` / `jackin::docker::RunOptions` | `jackin_docker::CommandRunner` / `jackin_docker::RunOptions` |
| `jackin::docker_client::*` | `jackin_docker::docker_client::*` |
| `jackin::env_resolver::` | `jackin_env::` |
| `jackin::instance::naming::` | `jackin_runtime::instance::naming::` |
| `jackin::isolation::MountIsolation` | `jackin_core::MountIsolation` |
| `jackin::isolation::finalize::` | `jackin_runtime::isolation::finalize::` |
| `jackin::isolation::materialize::` | `jackin_runtime::isolation::materialize::` |
| `jackin::isolation::state::` | `jackin_runtime::isolation::state::` |
| `jackin::manifest::` | `jackin_manifest::manifest::` |
| `jackin::manifest::migrations::` | `jackin_manifest::migrations::` |
| `jackin::manifest::validate::` | `jackin_manifest::validate::` |
| `jackin::operator_env::EnvValue` | `jackin_core::EnvValue` |
| `jackin::operator_env::EnvValue::as_persisted_str` | `jackin_core::EnvValue::as_persisted_str` |
| `jackin::operator_env::{OpAccount, OpField, OpItem, OpVault, …}` | `jackin_env::{OpAccount, OpField, OpItem, OpVault, …}` |
| `jackin::paths::JackinPaths` | `jackin_core::JackinPaths` |
| `jackin::repo::CachedRepo` | `jackin_manifest::repo::CachedRepo` |
| `jackin::repo::validate_role_repo` | `jackin_manifest::repo::validate_role_repo` |
| `jackin::runtime::{LoadOptions, load_role}` | `jackin_runtime::runtime::{LoadOptions, load_role}` |

`jackin::selector::RoleSelector` — NOT changed; `selector` is not a shim.
`jackin::workspace::*` — NOT changed; `workspace` is not a shim.

#### Step 9 — Delete the 19 shim files and clean up `lib.rs`

Delete all 19 files listed in the "Deleted (shims)" section of Touches.

In `crates/jackin/src/lib.rs`, remove the 19 `pub mod` lines:

```rust
// remove all of these:
pub mod agent;
pub mod agent_binary;
pub mod binary_artifact;
pub mod capsule_binary;
pub mod config;
pub mod derived_image;
pub mod diagnostics;
pub mod docker;
pub mod docker_client;
pub mod env_model;
pub mod env_resolver;
pub mod instance;
pub mod isolation;
pub mod manifest;
pub mod operator_env;
pub mod paths;
pub mod repo;
pub mod repo_contract;
pub mod runtime;
```

After removal, run `cargo clippy -p jackin --all-targets --all-features -- -D warnings` and fix any newly surfaced `unused_imports` warnings in the remaining modules. The `#![allow(clippy::redundant_pub_crate)]` at the top of `lib.rs` may be removable if no remaining module triggers it — check after clippy passes.

---

#### Verify

```sh
# After each per-shim commit (fast check):
cargo check -p jackin --all-targets

# After Step 9 (all shims deleted):
cargo clippy -p jackin --all-targets --all-features -- -D warnings

# Full library unit tests:
cargo nextest run -p jackin --lib --all-features

# Full test suite including integration (non-Docker):
cargo nextest run -p jackin --all-features

# Owning-crate tests for migrated test files:
cargo nextest run -p jackin-core --all-features

# Full workspace check:
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Unused-dep scanner (must stay clean):
cargo shear --fix
```

---

#### Done when

1. `crates/jackin/src/lib.rs` lists exactly 9 top-level modules: `app`, `cli`, `console`, `error`, `preflight`, `role_authoring`, `selector`, `tui`, `workspace`.
2. Zero occurrences of `crate::agent`, `crate::agent_binary`, `crate::binary_artifact`, `crate::capsule_binary`, `crate::config`, `crate::derived_image`, `crate::diagnostics`, `crate::docker`, `crate::docker_client`, `crate::env_model`, `crate::env_resolver`, `crate::instance`, `crate::isolation`, `crate::manifest`, `crate::operator_env`, `crate::paths`, `crate::repo`, `crate::repo_contract`, `crate::runtime` anywhere in `crates/jackin/`.
3. Zero occurrences of `jackin::<any-deleted-shim>::` in `crates/jackin/tests/`.
4. `cargo nextest run -p jackin --all-features` passes.
5. `cargo nextest run -p jackin-core --all-features` passes (covers migrated `isolation/tests.rs` and `env_model/tests.rs`).
6. `cargo shear` exits 0.
7. `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0.

---

#### Rollback

```sh
git restore crates/jackin/src/ crates/jackin/tests/ crates/jackin/Cargo.toml \
    crates/jackin-core/src/env_model.rs crates/jackin-core/src/isolation.rs
git clean -f crates/jackin-core/src/env_model/ crates/jackin-core/src/isolation/
```

Each shim commit is independently revertable via `git revert <sha>`.

---

#### Open questions

_None blocking._

- `jackin-core/src/env_model.rs` currently has two inline `#[cfg(test)] mod tests { … }` — a pre-existing violation of the crates/AGENTS.md hard rule. Step 3 fixes this as a side effect of the migration. Confirm with maintainer that fixing the inline-test violation in the same PR is desired.
- `docker_client/tests.rs` contains only `assert_eq!(std::mem::size_of::<FakeDockerClient>(), N)`. After the shim is deleted `FakeDockerClient` lives in `jackin_runtime::runtime::test_support`. The test can be ported there if size regression tracking is desired; the playbook leaves it deleted unless the maintainer flags it.
- The `build_jackin_capsule.rs` binary uses `jackin_image` directly after Step 7; confirm `jackin-image` is already in `crates/jackin/[dependencies]` (it is, per current `Cargo.toml`) so no production-dep change is needed.

---

### C1 — Carve jackin-host out of jackin-runtime (cold)

- **Goal:** Extract host OS integration (desktop, clipboard, caffeinate/keep-awake) from `crates/jackin-runtime/src/runtime/` into a new `crates/jackin-host/` crate at layer L2 (infrastructure/adapters), with `jackin-runtime` depending on `jackin-host` (not the reverse).
- **Preconditions:** none (cold path — no E0 perf benchmark required)
- **Pattern:** crate-carve recipe
- **Touches:**
  - Created: `crates/jackin-host/Cargo.toml`, `crates/jackin-host/src/lib.rs`
  - Moved (git mv): `crates/jackin-runtime/src/runtime/caffeinate.rs` → `crates/jackin-host/src/caffeinate.rs`, `crates/jackin-runtime/src/runtime/caffeinate/tests.rs` → `crates/jackin-host/src/caffeinate/tests.rs`, `crates/jackin-runtime/src/runtime/host_clipboard.rs` → `crates/jackin-host/src/host_clipboard.rs`, `crates/jackin-runtime/src/runtime/host_clipboard/tests.rs` → `crates/jackin-host/src/host_clipboard/tests.rs`, `crates/jackin-runtime/src/runtime/host_desktop.rs` → `crates/jackin-host/src/host_desktop.rs`, `crates/jackin-runtime/src/runtime/host_desktop/tests.rs` → `crates/jackin-host/src/host_desktop/tests.rs`
  - Modified: `crates/jackin-core/Cargo.toml`, `crates/jackin-core/src/lib.rs`
  - Created: `crates/jackin-core/src/test_support.rs`
  - Modified: `crates/jackin-runtime/src/runtime/test_support.rs`, `crates/jackin-runtime/src/runtime.rs`, `crates/jackin-runtime/src/runtime/attach.rs`, `crates/jackin-runtime/src/runtime/host_attach.rs`, `crates/jackin-runtime/Cargo.toml`
  - Modified: `Cargo.toml` (workspace members)
  - Modified: `docs/content/docs/reference/getting-oriented/codebase-map.mdx`, `docs/content/docs/roadmap/codebase-health-enforcement.mdx`

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

#### Prerequisite A — move FakeRunner + FakeDockerClient to jackin-core so caffeinate tests can use them without a circular dev-dep

1. **Edit `crates/jackin-core/Cargo.toml`** — append at end of file:
   ```toml
   [features]
   test-support = []
   ```

2. **Create `crates/jackin-core/src/test_support.rs`** with the exact content below (adapted from `crates/jackin-runtime/src/runtime/test_support.rs` — `FakeRunner` and `FakeDockerClient` only; the jackin-image-dependent helpers stay in runtime):
   ```rust
   //! Shared test-only fakes: `FakeRunner` (CommandRunner) and `FakeDockerClient` (DockerApi).
   //!
   //! Gated by the `test-support` cargo feature; never compiled in production builds.
   
   #![expect(
       clippy::expect_used,
       clippy::unwrap_used,
       reason = "test support fixture setup should fail immediately with source location"
   )]
   
   use std::collections::{HashMap, VecDeque};
   
   use crate::{CommandRunner, RunOptions};
   use crate::{ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome};
   
   #[expect(
       missing_debug_implementations,
       reason = "FakeRunner stores one-shot side-effect closures that cannot be formatted."
   )]
   #[derive(Default)]
   pub struct FakeRunner {
       pub recorded: Vec<String>,
       pub run_recorded: Vec<String>,
       pub run_options: Vec<RunOptions>,
       pub fail_on: Vec<String>,
       pub fail_with: Vec<(String, String)>,
       pub capture_queue: VecDeque<String>,
       pub side_effects: Vec<(String, Box<dyn FnOnce()>)>,
   }
   
   #[cfg_attr(
       not(test),
       expect(dead_code, reason = "test helper impl is consumed by test targets")
   )]
   impl FakeRunner {
       pub fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
           Self {
               capture_queue: VecDeque::from(outputs),
               ..Default::default()
           }
       }
   
       /// Number of capture calls `load_role` makes before reaching role-
       /// specific logic: 2 identity lookups (`git config user.name`,
       /// `git config user.email`).
       const LOAD_PREAMBLE_CAPTURES: usize = 2;
   
       pub fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
           let mut queue = VecDeque::with_capacity(Self::LOAD_PREAMBLE_CAPTURES + N);
           for _ in 0..Self::LOAD_PREAMBLE_CAPTURES {
               queue.push_back(String::new());
           }
           queue.extend(outputs);
           Self {
               capture_queue: queue,
               ..Default::default()
           }
       }
   }
   
   impl FakeRunner {
       fn check_command(&mut self, command: &str) -> anyhow::Result<()> {
           if let Some((_, message)) = self
               .fail_with
               .iter()
               .find(|(pattern, _)| command.contains(pattern))
           {
               let message = message.clone();
               anyhow::bail!("{message}");
           }
           if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
               anyhow::bail!("command failed: {command}");
           }
           if let Some(pos) = self
               .side_effects
               .iter()
               .position(|(pattern, _)| command.contains(pattern))
           {
               let (_, callback) = self.side_effects.remove(pos);
               callback();
           }
           Ok(())
       }
   }
   
   impl CommandRunner for FakeRunner {
       async fn run(
           &mut self,
           program: &str,
           args: &[&str],
           _cwd: Option<&std::path::Path>,
           opts: &RunOptions,
       ) -> anyhow::Result<()> {
           let command = format!("{} {}", program, args.join(" "));
           self.run_options.push(opts.clone());
           self.run_recorded.push(command.clone());
           self.recorded.push(command.clone());
           self.check_command(&command)
       }
   
       async fn capture(
           &mut self,
           program: &str,
           args: &[&str],
           _cwd: Option<&std::path::Path>,
       ) -> anyhow::Result<String> {
           let command = format!("{} {}", program, args.join(" "));
           self.recorded.push(command.clone());
           self.check_command(&command)?;
           Ok(self.capture_queue.pop_front().unwrap_or_default())
       }
   
       async fn capture_secret(
           &mut self,
           program: &str,
           args: &[&str],
           cwd: Option<&std::path::Path>,
       ) -> anyhow::Result<String> {
           self.capture(program, args, cwd).await
       }
   }
   
   #[derive(Debug)]
   pub struct FakeDockerClient {
       pub recorded: std::cell::RefCell<Vec<String>>,
       pub inspect_queue: std::cell::RefCell<VecDeque<ContainerState>>,
       pub inspect_state_by_name: std::cell::RefCell<HashMap<String, ContainerState>>,
       pub list_containers_queue: std::cell::RefCell<VecDeque<Vec<ContainerRow>>>,
       pub list_networks_queue: std::cell::RefCell<VecDeque<Vec<NetworkRow>>>,
       pub list_image_tags_queue: std::cell::RefCell<VecDeque<Vec<String>>>,
       pub remove_image_queue: std::cell::RefCell<VecDeque<RemoveImageOutcome>>,
       pub exec_capture_queue: std::cell::RefCell<VecDeque<String>>,
       pub inspect_image_labels_queue: std::cell::RefCell<VecDeque<HashMap<String, String>>>,
       pub inspect_network_queue: std::cell::RefCell<VecDeque<Option<NetworkRow>>>,
       pub fail_with: Vec<(String, String)>,
       pub created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
       #[expect(
           clippy::type_complexity,
           reason = "test record tuple mirrors the API signature; factoring adds indirection without clarity"
       )]
       pub created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>, bool)>>,
   }
   
   impl Default for FakeDockerClient {
       fn default() -> Self {
           Self {
               recorded: std::cell::RefCell::new(Vec::new()),
               inspect_queue: std::cell::RefCell::new(VecDeque::new()),
               inspect_state_by_name: std::cell::RefCell::new(HashMap::new()),
               list_containers_queue: std::cell::RefCell::new(VecDeque::new()),
               list_networks_queue: std::cell::RefCell::new(VecDeque::new()),
               list_image_tags_queue: std::cell::RefCell::new(VecDeque::new()),
               remove_image_queue: std::cell::RefCell::new(VecDeque::new()),
               exec_capture_queue: std::cell::RefCell::new(VecDeque::new()),
               inspect_image_labels_queue: std::cell::RefCell::new(VecDeque::new()),
               inspect_network_queue: std::cell::RefCell::new(VecDeque::new()),
               fail_with: Vec::new(),
               created_containers: std::cell::RefCell::new(Vec::new()),
               created_networks: std::cell::RefCell::new(Vec::new()),
           }
       }
   }
   
   impl FakeDockerClient {
       fn check_fail(&self, op: &str) -> anyhow::Result<()> {
           if let Some((_, msg)) = self
               .fail_with
               .iter()
               .find(|(pat, _)| op.contains(pat.as_str()))
           {
               anyhow::bail!("{msg}");
           }
           Ok(())
       }
   
       fn record(&self, entry: &str) {
           self.recorded.borrow_mut().push(entry.to_owned());
       }
   
       fn ignore_if_missing(result: anyhow::Result<()>) -> anyhow::Result<()> {
           result.or_else(|e| {
               if e.to_string().to_ascii_lowercase().contains("no such") {
                   Ok(())
               } else {
                   Err(e)
               }
           })
       }
   
       fn pop_inspect(&self) -> ContainerState {
           self.inspect_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or(ContainerState::NotFound)
       }
   
       fn pop_list_containers(&self) -> Vec<ContainerRow> {
           self.list_containers_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or_default()
       }
   
       fn pop_list_networks(&self) -> Vec<NetworkRow> {
           self.list_networks_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or_default()
       }
   
       fn pop_list_image_tags(&self) -> Vec<String> {
           self.list_image_tags_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or_default()
       }
   
       fn pop_remove_image(&self) -> RemoveImageOutcome {
           self.remove_image_queue
               .borrow_mut()
               .pop_front()
               .expect("remove_image called but remove_image_queue is empty")
       }
   
       fn pop_exec_capture(&self) -> String {
           self.exec_capture_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or_default()
       }
   
       fn pop_inspect_image_labels(&self) -> HashMap<String, String> {
           self.inspect_image_labels_queue
               .borrow_mut()
               .pop_front()
               .unwrap_or_default()
       }
   
       fn pop_inspect_network(&self) -> Option<NetworkRow> {
           self.inspect_network_queue
               .borrow_mut()
               .pop_front()
               .flatten()
       }
   }
   
   impl DockerApi for FakeDockerClient {
       async fn inspect_container_state(&self, name: &str) -> ContainerState {
           let op = format!("docker inspect {name}");
           self.record(&op);
           if let Some((_, msg)) = self
               .fail_with
               .iter()
               .find(|(pat, _)| op.contains(pat.as_str()))
           {
               let msg = msg.clone();
               let lower = msg.to_ascii_lowercase();
               if lower.contains("no such object")
                   || lower.contains("no such container")
                   || lower.contains("no such image")
               {
                   return ContainerState::NotFound;
               }
               return ContainerState::InspectUnavailable(msg);
           }
           if let Some(state) = self.inspect_state_by_name.borrow().get(name) {
               return state.clone();
           }
           self.pop_inspect()
       }
   
       async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
           let op = format!("docker rm -f {name}");
           self.record(&op);
           Self::ignore_if_missing(self.check_fail(&op))
       }
   
       async fn list_containers(
           &self,
           label_filters: &[&str],
           all: bool,
       ) -> anyhow::Result<Vec<ContainerRow>> {
           let filter_str = label_filters.join(" --filter ");
           let op = if all {
               format!("docker ps -a --filter {filter_str}")
           } else {
               format!("docker ps --filter {filter_str}")
           };
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_list_containers())
       }
   
       async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()> {
           let op = format!("create_container:{name}");
           self.record(&op);
           self.check_fail(&op)?;
           self.created_containers
               .borrow_mut()
               .push((name.to_owned(), spec));
           Ok(())
       }
   
       async fn start_container(&self, name: &str) -> anyhow::Result<()> {
           let op = format!("start_container:{name}");
           self.record(&op);
           self.check_fail(&op)
       }
   
       async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
           let op = format!("docker volume rm {name}");
           self.record(&op);
           Self::ignore_if_missing(self.check_fail(&op))
       }
   
       async fn create_network(
           &self,
           name: &str,
           labels: HashMap<String, String>,
           internal: bool,
       ) -> anyhow::Result<()> {
           let op = format!("docker network create {name}");
           self.record(&op);
           self.created_networks
               .borrow_mut()
               .push((name.to_owned(), labels, internal));
           self.check_fail(&op)
       }
   
       async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
           let op = format!("docker network rm {name}");
           self.record(&op);
           Self::ignore_if_missing(self.check_fail(&op))
       }
   
       async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
           let filter_str = label_filters.join(" --filter ");
           let op = format!("docker network ls --filter {filter_str}");
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_list_networks())
       }
   
       async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
           let op = format!("docker network inspect {name}");
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_inspect_network())
       }
   
       async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
           let op = format!("docker images --filter reference={reference_filter}");
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_list_image_tags())
       }
   
       async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
           let op = format!("docker rmi {name}");
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_remove_image())
       }
   
       async fn inspect_image_labels(
           &self,
           image: &str,
       ) -> anyhow::Result<HashMap<String, String>> {
           let op = format!("docker inspect image:{image}");
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_inspect_image_labels())
       }
   
       async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
           let op = format!("docker pull {image}");
           self.record(&op);
           self.check_fail(&op)
       }
   
       async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String> {
           let op = format!("docker exec {} {}", container, cmd.join(" "));
           self.record(&op);
           self.check_fail(&op)?;
           Ok(self.pop_exec_capture())
       }
   }
   ```

3. **Edit `crates/jackin-core/src/lib.rs`** — insert after the last `pub mod` line (after `pub mod worktree_dirty;`):
   ```rust
   #[cfg(any(test, feature = "test-support"))]
   pub mod test_support;
   ```

4. **Edit `crates/jackin-runtime/src/runtime/test_support.rs`** — replace the `FakeRunner` struct definition, its two `impl FakeRunner` blocks, and the `CommandRunner` impl entirely with a single re-export line. Also replace the `fake_docker` module block and its `pub use fake_docker::FakeDockerClient;` line. Keep `install_all_test_stubs`, `TEST_DOCKERFILE_FROM`, `TEST_MANIFEST_TOML`, `seed_valid_role_repo`, and `first_temp_role_repo` unchanged. Specifically:

   Replace:
   ```rust
   #[expect(
       missing_debug_implementations,
       reason = "FakeRunner stores one-shot side-effect closures that cannot be formatted."
   )]
   #[derive(Default)]
   pub struct FakeRunner {
   ```
   ...through the end of the `impl CommandRunner for FakeRunner { ... }` block...
   
   With:
   ```rust
   #[cfg(any(test, feature = "test-support"))]
   pub use jackin_core::test_support::FakeRunner;
   ```

   Replace the block:
   ```rust
   #[cfg(any(test, feature = "test-support"))]
   pub use fake_docker::FakeDockerClient;
   
   #[cfg(any(test, feature = "test-support"))]
   pub mod fake_docker {
   ```
   ...through the end of the `fake_docker` module closing `}`...
   
   With:
   ```rust
   #[cfg(any(test, feature = "test-support"))]
   pub use jackin_core::test_support::FakeDockerClient;
   ```

   Also add at the top of the file (after the existing doc comment if any):
   ```rust
   use jackin_core::{CommandRunner, RunOptions};
   ```
   becomes a removal if it was only used by `FakeRunner`. Check for remaining uses; if none, remove it.

   Note: `TEST_DOCKERFILE_FROM`, `TEST_MANIFEST_TOML`, `seed_valid_role_repo`, `first_temp_role_repo`, `install_all_test_stubs`, and `first_temp_role_repo` remain unchanged.

#### Phase 1 — create the new crate

5. **Create directory** `crates/jackin-host/src/` (with `mkdir -p crates/jackin-host/src`).

6. **Create `crates/jackin-host/Cargo.toml`** with the exact content:
   ```toml
   [package]
   name = "jackin-host"
   version = "0.6.0-dev"
   edition.workspace = true
   rust-version.workspace = true
   authors = ["Alexey Zhokhov <alexey@zhokhov.com>"]
   description = "Host OS integration: desktop open/reveal, clipboard image reads, caffeinate keep-awake reconciler."
   license.workspace = true
   publish = false
   
   repository.workspace = true
   
   [lib]
   name = "jackin_host"
   path = "src/lib.rs"
   
   [dependencies]
   jackin-core = { version = "0.6.0-dev", path = "../jackin-core" }
   jackin-docker = { version = "0.6.0-dev", path = "../jackin-docker" }
   jackin-diagnostics = { version = "0.6.0-dev", path = "../jackin-diagnostics" }
   jackin-protocol = { version = "0.6.0-dev", path = "../jackin-protocol" }
   
   anyhow = "1.0"
   fs2 = "0.4"
   tokio = { version = "1", features = ["rt", "macros"] }
   url = "2"
   
   [dev-dependencies]
   jackin-core = { version = "0.6.0-dev", path = "../jackin-core", features = ["test-support"] }
   jackin-docker = { version = "0.6.0-dev", path = "../jackin-docker" }
   tempfile = "3.20"
   tokio = { version = "1", features = ["test-util"] }
   
   [lints]
   workspace = true
   ```

7. **Create `crates/jackin-host/src/lib.rs`** with the exact content:
   ```rust
   //! jackin-host: host OS integration.
   //!
   //! Architecture Invariant: this crate is L2 infrastructure. Allowed dependencies:
   //! `jackin-core` (L0 domain), `jackin-docker` (L2 infra), `jackin-diagnostics` (L2 infra),
   //! `jackin-protocol` (L0 wire types). Must NOT depend on `jackin-runtime` (L1 application)
   //! or any presentation layer crate (`jackin-tui`, `jackin-launch`).
   
   pub mod caffeinate;
   pub mod host_clipboard;
   pub mod host_desktop;
   ```

8. **Edit root `Cargo.toml`** — in the `members = [` list, insert `"crates/jackin-host",` after the line `"crates/jackin-env",`:
   ```toml
       "crates/jackin-host",
   ```

#### Phase 2 — move the modules

9. **Create destination subdirectories:**
   ```bash
   mkdir -p crates/jackin-host/src/caffeinate
   mkdir -p crates/jackin-host/src/host_clipboard
   mkdir -p crates/jackin-host/src/host_desktop
   ```

10. **git mv the six files:**
    ```bash
    git mv crates/jackin-runtime/src/runtime/caffeinate.rs crates/jackin-host/src/caffeinate.rs
    git mv crates/jackin-runtime/src/runtime/caffeinate/tests.rs crates/jackin-host/src/caffeinate/tests.rs
    git mv crates/jackin-runtime/src/runtime/host_clipboard.rs crates/jackin-host/src/host_clipboard.rs
    git mv crates/jackin-runtime/src/runtime/host_clipboard/tests.rs crates/jackin-host/src/host_clipboard/tests.rs
    git mv crates/jackin-runtime/src/runtime/host_desktop.rs crates/jackin-host/src/host_desktop.rs
    git mv crates/jackin-runtime/src/runtime/host_desktop/tests.rs crates/jackin-host/src/host_desktop/tests.rs
    ```
    Then remove the now-empty source directories:
    ```bash
    rmdir crates/jackin-runtime/src/runtime/caffeinate
    rmdir crates/jackin-runtime/src/runtime/host_clipboard
    rmdir crates/jackin-runtime/src/runtime/host_desktop
    ```

#### Phase 3 — fix cross-boundary references in moved files

11. **Edit `crates/jackin-host/src/caffeinate.rs`** — two edits:

    a. Add two `const` declarations immediately before the `const PID_FILENAME` line (line 47 of the original):
    ```rust
    const LABEL_MANAGED: &str = "jackin.managed=true";
    const LABEL_KEEP_AWAKE: &str = "jackin.keep.awake=true";
    ```

    b. Replace the two-element slice inside `count_keep_awake_agents` (original lines 217–220):
    ```rust
            &[
                super::naming::LABEL_MANAGED,
                super::naming::LABEL_KEEP_AWAKE,
            ],
    ```
    with:
    ```rust
            &[LABEL_MANAGED, LABEL_KEEP_AWAKE],
    ```

12. **Edit `crates/jackin-host/src/host_clipboard.rs`** — five edits:

    a. Change `pub(super) fn is_image_paste_trigger` to `pub fn is_image_paste_trigger` (line 29 of original).

    b. Change `pub(super) async fn read_image_for_paste_trigger` to `pub async fn read_image_for_paste_trigger` (line 33 of original).

    c. Change `pub(super) async fn read_image_from_pasted_path` to `pub async fn read_image_from_pasted_path` (line 166 of original).

    d. Change all three `pub(super) async fn read_host_clipboard_image` variants (lines 229, 234, 239 of original) to `pub async fn read_host_clipboard_image`.

    e. Change all three `pub(super) async fn read_host_clipboard_text_path_image` variants (lines 244, 253, 262 of original) to `pub async fn read_host_clipboard_text_path_image`.

    f. Replace line 62 of the original (inside `paste_image_paths_enabled_for`):
    ```rust
        value.is_none_or(|value| super::universe::env_flag_enabled(Some(value)))
    ```
    with:
    ```rust
        value.is_none_or(|value| env_flag_enabled(Some(value)))
    ```

    g. Append the following private function immediately before the `fn find_subsequence` function (after the `paste_image_paths_enabled_for` function's closing `}`):
    ```rust
    fn env_flag_enabled(value: Option<impl AsRef<std::ffi::OsStr>>) -> bool {
        let Some(value) = value else {
            return false;
        };
        let Some(value) = value.as_ref().to_str() else {
            return true;
        };
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    }
    ```

13. **Edit `crates/jackin-host/src/host_desktop.rs`** — three visibility changes:

    a. Change `pub(super) fn open_host_url` to `pub fn open_host_url` (line 6 of original).

    b. Change `pub(super) fn reveal_host_file` to `pub fn reveal_host_file` (line 14 of original).

    c. Change `pub(super) fn open_host_file` to `pub fn open_host_file` (line 20 of original).

    Leave `host_reveal_command`, `host_file_open_command`, `host_open_command`, `host_open_command_with_policy`, and the private `run_host_desktop_command` at their original visibility.

14. **Edit `crates/jackin-host/src/caffeinate/tests.rs`** — replace the two broken import lines:

    Replace:
    ```rust
    use super::super::test_support::FakeRunner;
    ```
    with:
    ```rust
    use jackin_core::test_support::FakeRunner;
    ```

    Replace:
    ```rust
    use crate::runtime::test_support::FakeDockerClient;
    ```
    with:
    ```rust
    use jackin_core::test_support::FakeDockerClient;
    ```

#### Phase 4 — update jackin-runtime to use the new crate

15. **Edit `crates/jackin-runtime/src/runtime.rs`** — three edits:

    a. Remove the line `pub mod caffeinate;`.

    b. Remove the line `mod host_clipboard;`.

    c. Remove the line `mod host_desktop;`.

    d. Replace:
    ```rust
    pub use self::caffeinate::reconcile as reconcile_keep_awake;
    ```
    with:
    ```rust
    pub use jackin_host::caffeinate::reconcile as reconcile_keep_awake;
    ```

16. **Edit `crates/jackin-runtime/src/runtime/attach.rs`** — replace all four occurrences of `super::caffeinate::reconcile(` with `jackin_host::caffeinate::reconcile(`. Exact lines in the original: 459, 525, 614, 737. Each occurrence is a standalone call:
    ```rust
    super::caffeinate::reconcile(paths, docker, runner).await;
    ```
    becomes:
    ```rust
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    ```

17. **Edit `crates/jackin-runtime/src/runtime/host_attach.rs`** — replace the two import statements (original lines 34–38):

    Replace:
    ```rust
    use super::host_clipboard::{
        is_image_paste_trigger, read_host_clipboard_image, read_host_clipboard_text_path_image,
        read_image_for_paste_trigger, read_image_from_pasted_path,
    };
    use super::host_desktop::{open_host_file, open_host_url, reveal_host_file};
    ```
    with:
    ```rust
    use jackin_host::host_clipboard::{
        is_image_paste_trigger, read_host_clipboard_image, read_host_clipboard_text_path_image,
        read_image_for_paste_trigger, read_image_from_pasted_path,
    };
    use jackin_host::host_desktop::{open_host_file, open_host_url, reveal_host_file};
    ```

18. **Edit `crates/jackin-runtime/Cargo.toml`** — add `jackin-host` to `[dependencies]` after the `jackin-diagnostics` line:
    ```toml
    jackin-host = { version = "0.6.0-dev", path = "../jackin-host" }
    ```

#### Phase 5 — docs and ratchet

19. **Edit `docs/content/docs/reference/getting-oriented/codebase-map.mdx`** — in the crate-tier diagram (the `text` code block), insert a new line in Tier 2 (after the `jackin-docker` line) and update the `jackin-runtime` Tier 4 description:

    After:
    ```
            jackin-docker      BollardDockerClient + ShellRunner + net  → core, diagnostics
    ```
    insert:
    ```
            jackin-host        desktop open/reveal + clipboard image reads +
                               caffeinate keep-awake reconciler            → core, docker, diagnostics, protocol
    ```

    In the `jackin-runtime` Tier 4 line, remove "host desktop/clipboard" from what it contains (it is now in `jackin-host`):
    Before: `jackin-runtime     runtime + instance + isolation  → core, config, env, manifest, docker, image, diagnostics`
    After: `jackin-runtime     runtime + instance + isolation                   → core, config, env, manifest, docker, image, diagnostics, host`

    Also update the `codebase-map.mdx` prose reference that points to `crates/jackin-runtime/src/runtime/caffeinate.rs` — change it to point to `crates/jackin-host/src/caffeinate.rs`.

20. **Edit `docs/content/docs/roadmap/codebase-health-enforcement.mdx`** — find the line:
    ```
    - [ ] **C1 — carve `jackin-host`** out of `jackin-runtime`: desktop, clipboard, caffeinate/keep-awake. Cold path — no perf gate.
    ```
    Change `- [ ]` to `- [x]`.

21. **Run `cargo xtask lint files --print-budget`** and verify no lines changed for `crates/jackin-runtime/src/runtime/host_clipboard.rs`, `crates/jackin-runtime/src/runtime/host_desktop.rs`, or `crates/jackin-runtime/src/runtime/caffeinate.rs` in `file-size-budget.toml`. If any of those paths exist as grandfathered entries in `file-size-budget.toml`, delete those entries (the files no longer exist at those paths).

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no formatting violations
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-core` → all tests pass (FakeRunner/FakeDockerClient accessible under test-support feature)
- `cargo nextest run -p jackin-runtime` → all tests pass (existing tests use re-exported FakeRunner/FakeDockerClient; caffeinate tests now compile against jackin-host)
- `cargo nextest run -p jackin-host` → all tests pass (caffeinate, host_clipboard, host_desktop tests)
- `cargo nextest run --workspace` → all tests pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all green
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `cargo build --workspace` succeeds, `cargo nextest run --workspace` is all green, `cargo xtask lint` is green, and `crates/jackin-runtime/src/runtime/` no longer contains `caffeinate.rs`, `host_clipboard.rs`, or `host_desktop.rs`.

**Rollback:** `git restore --staged . && git restore .` to undo all unstaged edits; then `git checkout HEAD -- crates/jackin-runtime/src/runtime/caffeinate.rs crates/jackin-runtime/src/runtime/caffeinate/ crates/jackin-runtime/src/runtime/host_clipboard.rs crates/jackin-runtime/src/runtime/host_clipboard/ crates/jackin-runtime/src/runtime/host_desktop.rs crates/jackin-runtime/src/runtime/host_desktop/` to restore the moved files; then `git rm -r crates/jackin-host/` to remove the new crate.

**Open questions:**
- Step 4 requires careful surgery on `crates/jackin-runtime/src/runtime/test_support.rs` — the exact byte ranges of the FakeRunner struct + two impl blocks + CommandRunner impl that must be removed span lines 37–148 (approximately); verify the line range with `wc -l` before editing to avoid removing `install_all_test_stubs` and the other remaining helpers.
- The `jackin-host/Cargo.toml` tokio `features` list above includes only `["rt", "macros"]`. If `host_clipboard.rs` or `caffeinate.rs` use any additional tokio features at compile time, Clippy or the build will fail — add the missing features then.
- The `docs/content/docs/reference/getting-oriented/codebase-map.mdx` has a `<RepoFile>` link pointing to `crates/jackin-runtime/src/runtime/caffeinate.rs` (line 132 of the original file) — update that path to `crates/jackin-host/src/caffeinate.rs` as part of step 19; the `bun run check:repo-links` CI job will catch it if missed.

---

### C2 — Carve jackin-usage out of jackin-capsule (cold)

- **Goal:** Move the usage/pricing/telemetry/token-monitor subsystem out of `jackin-capsule` into a new `jackin-usage` crate, reducing the capsule god-crate and making the usage subsystem independently navigable.
- **Preconditions:** A5 (dependency-direction bans gate) must be done first per critical-path ordering. Additionally, two design decisions in **Open questions** below must be resolved before any step below is executed; executing without those decisions will produce a circular dependency or behavior change.
- **Pattern:** crate-carve recipe (Strangler Fig + Parallel Change)
- **Touches:**
  - **Created:** `crates/jackin-usage/` (new crate — all files listed in steps)
  - **Modified:** `crates/jackin-capsule/src/lib.rs`, `crates/jackin-capsule/Cargo.toml`, `crates/jackin-capsule/src/daemon.rs`, `crates/jackin-capsule/src/client.rs`, `crates/jackin-capsule/src/daemon/multiplexer_utils.rs`, `crates/jackin-capsule/src/daemon/tests.rs`, `crates/jackin-capsule/src/logging.rs`
  - **Modified (gates):** `Cargo.toml` (workspace members), `file-size-budget.toml`, `test-layout-allowlist.toml`
  - **Modified (docs):** `PROJECT_STRUCTURE.md`, `docs/content/docs/reference/getting-oriented/codebase-map.mdx`, `docs/content/docs/roadmap/codebase-health-enforcement.mdx`
  - **NOT moved:** `crates/jackin-capsule/src/alloc_telemetry.rs` — DHAT heap profiling, capsule-specific performance tooling with no usage/pricing logic; stays in `jackin-capsule`

**Structural blockers — resolve BEFORE executing any step:**

> **TODO(investigate): Blocker 1 — `clog!`/`cdebug!` macro circular dependency.**
> The `clog!` and `cdebug!` macros are defined in `crates/jackin-capsule/src/logging.rs` (lines 141–164) as `#[macro_export]` macros. Both macros expand to call `$crate::logging::write_line` and `$crate::telemetry::bridge_log` where `$crate = jackin_capsule`. The files to be moved call `crate::clog!`/`crate::cdebug!` at these exact sites:
> - `usage.rs`: 14 call sites
> - `telemetry.rs`: 2 call sites (lines 31, 33)
> - `telemetry_store.rs`: 1 call site (line 339)
> - `token_monitor.rs`: 1 call site (in `recompute_spend`, line 251)
> - `token_monitor/opencode.rs`: 4 call sites (lines 15, 19, 28, 38)
>
> After the move, `crate::clog!` inside `jackin-usage` resolves to `jackin_usage::clog!` — but no such macro exists in the new crate. Importing it from `jackin-capsule` creates a circular dependency (`jackin-capsule → jackin-usage → jackin-capsule`). Replacing the calls with `tracing::info!`/`tracing::debug!` directly changes observable behavior (removes lines from `/jackin/state/multiplexer.log` and changes the `[jackin-capsule]` log prefix). Three structurally valid resolutions exist but each has scope beyond C2:
> - **Option A**: Move `write_line`, `debug_enabled`, and the macro definitions into a new leaf crate (e.g. `jackin-capsule-logging`) that both `jackin-capsule` and `jackin-usage` depend on.
> - **Option B**: Extend `jackin-diagnostics` with the `write_line` function and redefine the macros there; both crates depend on `jackin-diagnostics` already.
> - **Option C**: Move `logging.rs` entirely to `jackin-usage` and have `jackin-capsule` re-export it via `pub mod logging { pub use jackin_usage::logging::*; }` — structurally valid since `$crate` in a re-exported macro resolves to the defining crate (`jackin_usage`), so all call sites in both crates would call `jackin_usage::logging::write_line`. Conceptually odd (daemon file-logging infrastructure in a usage crate) but mechanically clean.
> **The operator must pick one option and confirm it before C2 is executed.**

> **TODO(investigate): Blocker 2 — `telemetry_store/tests.rs` imports TUI type from jackin-capsule.**
> `crates/jackin-capsule/src/telemetry_store/tests.rs` line 1: `use crate::tui::components::dialog::Dialog;`; line 440: `Dialog::new_usage(view).usage_state()`. If `telemetry_store` moves to `jackin-usage`, this test would need `jackin-capsule::tui::components::dialog::Dialog`, creating a circular dependency (`jackin-usage → jackin-capsule → jackin-usage`). Two resolutions:
> - **Option A**: Remove or rewrite the `Dialog::new_usage` assertion in `telemetry_store/tests.rs` so the test exercises the snapshot data without TUI rendering. This is a test change that does NOT change production behavior, but the invariant "no test rewritten to accommodate a move" still applies — evaluate whether this is acceptable.
> - **Option B**: Keep `telemetry_store` in `jackin-capsule` (do not move it to `jackin-usage`). `usage.rs` calls `crate::telemetry_store::store_usage_snapshots` (line 394) and `telemetry_store.rs` calls `crate::usage::*` (lines 354–490); if `usage` moves out but `telemetry_store` stays, the cross-calls become cross-crate in both directions — `jackin-usage::usage → jackin-capsule::telemetry_store → jackin-usage::usage` — creating another circular dep. So option B forces also keeping `usage` in capsule (moving only `token_monitor` and optionally `telemetry`).
> **The operator must pick one option and confirm it before C2 is executed.**

---

**The steps below assume Blocker 1 → Option C (move `logging.rs` to `jackin-usage`, re-export from `jackin-capsule`) and Blocker 2 → Option A (remove the `Dialog` assertion from `telemetry_store/tests.rs`). If the operator chooses different options, adjust the steps accordingly.**

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Create directory `crates/jackin-usage/src/` (and sub-dirs that will be populated by git mv in later steps).

2. Create `crates/jackin-usage/Cargo.toml` with exact content:
```toml
[package]
name = "jackin-usage"
version = "0.6.0-dev"
edition.workspace = true
rust-version.workspace = true
authors = ["Alexey Zhokhov <alexey@zhokhov.com>"]
description = "Usage, pricing, telemetry, and token-monitor subsystem for the jackin-capsule in-container daemon."
license.workspace = true
repository.workspace = true
publish = false

[lib]
name = "jackin_usage"
path = "src/lib.rs"

[features]
default = []
dhat-heap = ["dep:dhat"]

[dependencies]
jackin-core = { version = "0.6.0-dev", path = "../jackin-core" }
jackin-protocol = { version = "0.6.0-dev", path = "../jackin-protocol" }
jackin-diagnostics = { version = "0.6.0-dev", path = "../jackin-diagnostics", features = ["otlp"] }
anyhow = "1.0"
base64 = "0.22"
chrono = { version = "0.4", default-features = false, features = ["clock"] }
fs2 = "0.4"
reqwest = { version = "0.13", default-features = false, features = ["blocking", "json", "rustls"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["rt", "sync", "macros", "time", "net"] }
tracing = "0.1"
turso = { version = "0.7.0-pre.7", default-features = false }
url = "2"
dhat = { version = "0.3.3", optional = true }

[lints]
workspace = true
```

3. Create `crates/jackin-usage/src/lib.rs` with exact content:
```rust
//! jackin-usage: usage/pricing/telemetry + token monitors for the jackin-capsule daemon.
//!
//! Architecture Invariant: allowed inward dependencies are `jackin-core`,
//! `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule`
//! (would be circular), `jackin-tui`, `jackin-console`, or any presentation crate.
//!
//! Logging infrastructure (`logging`, `clog!`, `cdebug!`) lives here so both
//! this crate and `jackin-capsule` can use the macros without a circular dep.
//! `jackin-capsule` re-exports `logging` and the macros from this crate.

pub mod logging;
pub mod telemetry;
pub(crate) mod telemetry_store;
pub(crate) mod token_monitor;
pub(crate) mod usage;
```

4. Run `git mv crates/jackin-capsule/src/logging.rs crates/jackin-usage/src/logging.rs` — moves the `write_line`, `init`, `debug_enabled` functions and the `clog!`/`cdebug!` macro definitions.

5. Run `git mv crates/jackin-capsule/src/telemetry.rs crates/jackin-usage/src/telemetry.rs`

6. Run `git mv crates/jackin-capsule/src/telemetry_store.rs crates/jackin-usage/src/telemetry_store.rs`

7. Run `git mv crates/jackin-capsule/src/telemetry_store/ crates/jackin-usage/src/telemetry_store/` — moves `telemetry_store/tests.rs`

8. Run `git mv crates/jackin-capsule/src/token_monitor.rs crates/jackin-usage/src/token_monitor.rs`

9. Run `git mv crates/jackin-capsule/src/token_monitor/ crates/jackin-usage/src/token_monitor/` — moves `amp.rs`, `amp/tests.rs`, `claude.rs`, `claude/tests.rs`, `codex.rs`, `codex/tests.rs`, `kimi.rs`, `kimi/tests.rs`, `opencode.rs`, `opencode/tests.rs`, `pricing.rs`, `pricing/tests.rs`, `tests.rs`

10. Run `git mv crates/jackin-capsule/src/usage.rs crates/jackin-usage/src/usage.rs`

11. Run `git mv crates/jackin-capsule/src/usage/ crates/jackin-usage/src/usage/` — moves `usage/format.rs`, `usage/tests.rs`

12. **Edit `crates/jackin-usage/src/telemetry_store/tests.rs`** (Blocker 2 fix — Option A): Remove line 1 (`use crate::tui::components::dialog::Dialog;`) and remove line 440 (`let state = Dialog::new_usage(view).usage_state().expect("usage state");`) and any following assertion that uses `state`. Replace the removed assertion with a comment `// Dialog rendering assertion removed: Dialog type lives in jackin-capsule (would create circular dep).` TODO(investigate): Confirm exact lines and assertions with the operator — the test spans lines 430–466 of `telemetry_store/tests.rs` and the `Dialog` reference is on line 440 only.

13. **Edit `crates/jackin-capsule/src/lib.rs`**: Remove these 5 module declarations:
    - `pub mod logging;` (currently line 25 — exact line TODO(investigate))
    - `pub mod telemetry;` (currently line 37)
    - `pub(crate) mod telemetry_store;` (currently line 38)
    - `pub(crate) mod token_monitor;` (currently line 39)
    - `pub(crate) mod usage;` (currently line 40)
    Add these re-exports in their place (keep the same block location):
    ```rust
    // Logging infrastructure lives in jackin-usage; re-export so all
    // capsule modules that call crate::clog!/ crate::cdebug! still work
    // — $crate in the macro expands to jackin_usage, which has write_line.
    pub mod logging {
        pub use jackin_usage::logging::*;
    }
    pub use jackin_usage::{clog, cdebug};
    pub mod telemetry {
        pub use jackin_usage::telemetry::*;
    }
    pub(crate) mod telemetry_store {
        pub(crate) use jackin_usage::telemetry_store::*;
    }
    pub(crate) mod token_monitor {
        pub(crate) use jackin_usage::token_monitor::*;
    }
    pub(crate) mod usage {
        pub(crate) use jackin_usage::usage::*;
    }
    ```
    TODO(investigate): The exact line numbers for the 5 removed declarations need to be verified against the current file (read `crates/jackin-capsule/src/lib.rs`). The declarations were on lines 25 and 37–40 at time of investigation but may shift.

14. **Edit `crates/jackin-capsule/Cargo.toml`**: Add `jackin-usage` dependency under `[dependencies]`:
    ```toml
    jackin-usage = { version = "0.6.0-dev", path = "../jackin-usage" }
    ```
    Keep `jackin-diagnostics` with `features = ["otlp"]` in capsule's own deps (capsule still initialises logging). Do NOT remove `reqwest`, `turso`, `fs2`, `url`, `base64`, `serde`, `serde_json`, `chrono` from jackin-capsule's Cargo.toml until `cargo shear` confirms they are unused in capsule itself — the codebase has other callers of these crates in capsule (e.g. `clipboard.rs` uses `base64`; `runtime_setup.rs` uses `reqwest`). TODO(investigate): Run `cargo shear` after the move to determine which deps can be dropped from jackin-capsule's manifest.

15. **Edit `Cargo.toml` (workspace root)**: Add `"crates/jackin-usage"` to the `members` array, after `"crates/jackin-capsule"`.

16. **Edit `crates/jackin-capsule/src/daemon.rs`** — update three use-import lines:
    - Line 73: `use crate::token_monitor::{TokenMonitor, TokenTotals};` → `use jackin_usage::token_monitor::{TokenMonitor, TokenTotals};`
    - Line 134: `use crate::usage::UsageCache;` → `use jackin_usage::usage::UsageCache;`
    - Line 367 (inline path): `Option<crate::usage::UsageRefreshTarget>` → `Option<jackin_usage::usage::UsageRefreshTarget>`
    - Line 830: `crate::telemetry::init()` — stays as-is because `crate::telemetry` is now re-exported from jackin-usage (re-export in step 13 makes this path still valid).
    - Line 831: `crate::alloc_telemetry::init_from_env()` — stays as-is; `alloc_telemetry` stays in capsule.
    TODO(investigate): Confirm exact line numbers by reading `crates/jackin-capsule/src/daemon.rs` lines 70–140 and 360–370 before editing.

17. **Edit `crates/jackin-capsule/src/client.rs`** — line 378:
    `crate::usage::run_claude_usage_diagnostic()` → `jackin_usage::usage::run_claude_usage_diagnostic()`
    TODO(investigate): Confirm exact line number and whether `jackin_usage` is already imported at the top of `client.rs`; if not, add `use jackin_usage;` or use the full path.

18. **Edit `crates/jackin-capsule/src/daemon/multiplexer_utils.rs`** — four sites:
    - Line 257: `Option<crate::usage::UsageRefreshTarget>` → `Option<jackin_usage::usage::UsageRefreshTarget>`
    - Line 262: `crate::usage::UsageRefreshTarget { agent, provider }` → `jackin_usage::usage::UsageRefreshTarget { agent, provider }`
    - Line 458: `Option<crate::usage::UsageRefreshTarget>` → `Option<jackin_usage::usage::UsageRefreshTarget>`
    - Line 462: `.map(|agent| crate::usage::UsageRefreshTarget {` → `.map(|agent| jackin_usage::usage::UsageRefreshTarget {`
    TODO(investigate): Confirm exact line numbers.

19. **Edit `crates/jackin-capsule/src/daemon/tests.rs`** — four sites (lines 351, 380, 418, 453):
    Each `crate::usage::UsageRefreshTarget {` → `jackin_usage::usage::UsageRefreshTarget {`
    TODO(investigate): Confirm exact line numbers.

20. **Edit `file-size-budget.toml`**: Remove the `[[production]]` entry for `crates/jackin-capsule/src/usage.rs` (lines 34–39 at time of investigation):
    ```toml
    [[production]]
    path = "crates/jackin-capsule/src/usage.rs"
    lines = 5983
    # Decomposition underway — first slice shipped (usage/format.rs, 414L);
    # the remaining per-provider splits (claude, codex, grok, amp) bring this
    # below the 2000L cap in follow-up PRs.
    ```
    `usage.rs` is now in `jackin-usage` where it is NOT yet grandfathered; it starts over under the 2000L production cap check for a new crate. Since `usage.rs` is 5981 lines, add a new grandfathered entry for `jackin-usage`:
    ```toml
    [[production]]
    path = "crates/jackin-usage/src/usage.rs"
    lines = 5981
    # God-crate carve C2 — moved from jackin-capsule. Decomposition of
    # usage/{claude,codex,grok,zai,kimi,minimax,amp,view,refresh}.rs is
    # the W5 follow-up that brings this below 2000L.
    ```

21. **Edit `test-layout-allowlist.toml`**: No entries need to be added (the moved `tests.rs` files have clean layout). If `cargo xtask lint tests` reports any violations from the moved files, add them here. No existing capsule entries are removed by this move.

22. **Edit `PROJECT_STRUCTURE.md`**: Add one bullet listing `crates/jackin-usage/` as entry/glue layer alongside `crates/jackin-capsule/`. Update `crates/jackin-capsule/` description to note that usage/telemetry/token-monitor subsystem moved to `jackin-usage`.

23. **Edit `docs/content/docs/reference/getting-oriented/codebase-map.mdx`**: Add `jackin-usage` to the L4 entry/glue row in the layer table. Update the prose paragraph that describes `jackin-capsule` to note that usage/pricing/telemetry/token-monitor lives in `jackin-usage` (same PR, same source-of-truth update).

24. **Edit `docs/content/docs/roadmap/codebase-health-enforcement.mdx`**: Mark the C2 checkbox as done (`- [x] **C2 — carve `jackin-usage`**`). Update the overall **Status** line at the top of the doc to reflect C2 landing.

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-usage` → all pass
- `cargo nextest run -p jackin-capsule` → all pass
- `cargo nextest run --workspace` → all pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK; arch informational
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → refresh `file-size-budget.toml` if `jackin-capsule/src/usage.rs` entry was the only over-cap file removed
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `cargo nextest run --workspace` passes; `jackin-capsule` no longer declares `mod usage`, `mod telemetry`, `mod telemetry_store`, `mod token_monitor`, `mod logging` directly (only re-exports from `jackin-usage`); `cargo run -p jackin-xtask --locked -- lint` exits 0; the `crates/jackin-usage/` crate is a workspace member; `file-size-budget.toml` no longer lists `crates/jackin-capsule/src/usage.rs`.

**Rollback:** `git restore crates/jackin-capsule/src/lib.rs crates/jackin-capsule/Cargo.toml Cargo.toml file-size-budget.toml test-layout-allowlist.toml` and `git rm -r crates/jackin-usage/`; then `git checkout -- crates/jackin-capsule/src/` to restore the git-mv'd files.

**Open questions:**

---

### D1 — Absorb runtime/image.rs into jackin-image (P5) + split it

- **Goal:** Move image-build orchestration out of `crates/jackin-runtime/src/runtime/image.rs` into `crates/jackin-image/`, splitting along three concerns (recipe/Dockerfile-gen, build-orchestration, cache-inspection), so that `jackin-runtime` delegates to `jackin-image` instead of owning the logic.
- **Preconditions:** A2 must be DONE (`LaunchProgress`, `LaunchStage`, and related progress value types relocated from `jackin-launch` to `jackin-core`). Without A2, `image.rs` cannot move because `prepare_runtime_binaries_for_agents`, `ensure_local_role_base`, and `build_agent_image` all take `Option<&mut LaunchProgress>` parameters, and `jackin-image` has no path to that type without adding `jackin-launch` as a dependency — which is the inversion this slate of work is fixing.
- **Pattern:** Parallel Change (expand in jackin-image, migrate callers, shrink jackin-runtime coordinator) + file-split inside the new crate
- **Touches:**
  - **Created:** `crates/jackin-image/src/naming.rs`, `crates/jackin-image/src/image_recipe.rs`, `crates/jackin-image/src/image_recipe/tests.rs`, `crates/jackin-image/src/image_decision.rs`, `crates/jackin-image/src/image_decision/tests.rs`, `crates/jackin-image/src/image_build.rs`, `crates/jackin-image/src/image_build/tests.rs`
  - **Modified:** `crates/jackin-image/src/lib.rs`, `crates/jackin-image/Cargo.toml`, `crates/jackin-runtime/src/runtime/image.rs`, `crates/jackin-runtime/src/runtime/image/tests.rs`, `crates/jackin-runtime/src/runtime/naming.rs`, `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`, `crates/jackin-runtime/src/instance/naming.rs` (re-export update), `crates/jackin-core/src/selector.rs` (gains `runtime_slug`), `file-size-budget.toml`, `docs/content/docs/reference/getting-oriented/codebase-map/` (update crate responsibilities)
  - **Unchanged:** `crates/jackin-runtime/src/lib.rs`, `crates/jackin-runtime/src/runtime/naming.rs` public surface for non-image labels, `crates/jackin-runtime/src/runtime/launch.rs`, `crates/jackin-runtime/src/runtime/prewarm_trigger.rs` (already imports `super::image::ImagePrewarmStatus` — re-export in coordinator keeps this working)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

**PART A — Sub-step: move `runtime_slug` to `jackin-core` so image-naming can reference it from `jackin-image`**

1. In `crates/jackin-core/src/selector.rs`, add a public free function immediately after the `impl RoleSelector` block (do NOT add it inside the impl — it is a free function used by path-string construction, not object behavior):

   ```rust
   /// Derive the role's canonical runtime slug (used for image-tag and
   /// repo-lock-file naming). A namespaced role becomes `namespace_name`;
   /// a bare role becomes `name`.
   pub fn runtime_slug(selector: &RoleSelector) -> String {
       selector.namespace.as_ref().map_or_else(
           || selector.name.clone(),
           |namespace| format!("{namespace}_{}", selector.name),
       )
   }
   ```

   Add `use crate::selector::RoleSelector;` at the top of the file if not already present (it already is, since `RoleSelector` is defined there).

2. In `crates/jackin-core/src/lib.rs`, re-export the new free function so callers can write `jackin_core::runtime_slug` (verify the exact export path after A2 is done):
   ```rust
   pub use selector::runtime_slug;
   ```

3. In `crates/jackin-runtime/src/instance/naming.rs`:
   - Remove the function body of `runtime_slug` (lines 17–22).
   - Replace with a re-export:
     ```rust
     pub use jackin_core::runtime_slug;
     ```
   - The `use jackin_core::selector::RoleSelector;` import at the top of the file (line 10) stays since it is still needed for other functions.

4. In `crates/jackin-runtime/src/runtime/repo_cache.rs`:
   - Line 8: the existing `use crate::instance::runtime_slug;` continues to work unchanged because `instance/naming.rs` now re-exports it. No edit needed.

5. Verify the sub-step compiles: `cargo check -p jackin-runtime -p jackin-core`.

**PART B — Add deps to `jackin-image`**

6. In `crates/jackin-image/Cargo.toml`, under `[dependencies]`, add:
   ```toml
   futures-util = "0.3"
   ```
   (`sha2`, `anyhow`, `serde`, `serde_json`, `tempfile`, `tokio`, `jackin-core`, `jackin-manifest`, `jackin-docker`, `jackin-diagnostics` are already present.)

   TODO(investigate): After A2 lands, confirm the exact crate and path for `LaunchProgress` and `LaunchStage` in `jackin-core`. If they are re-exported at `jackin_core::LaunchProgress` / `jackin_core::LaunchStage`, no new dep is needed (jackin-core is already a dep of jackin-image). If they are in a sub-crate, add that sub-crate here.

7. In `crates/jackin-image/Cargo.toml`, under `[dev-dependencies]`, add:
   ```toml
   jackin-runtime = { version = "0.6.0-dev", path = "../jackin-runtime", features = ["test-support"] }
   ```
   This is a dev-only dep (allowed by Cargo even though jackin-runtime depends on jackin-image as a regular dep; dev-deps do not participate in the regular dep resolution cycle). This gives the jackin-image tests access to `FakeDockerClient`, `FakeRunner`, and `TEST_DOCKERFILE_FROM`.

   TODO(investigate): Confirm Cargo permits this specific cycle in this workspace by running `cargo check --workspace` after step 7 before proceeding. If Cargo rejects it, the alternative is to create a `jackin-image-test-support` or shared `jackin-test-fixtures` crate — settle in an open question to the operator rather than guessing.

**PART C — Create `crates/jackin-image/src/naming.rs`**

8. Create `crates/jackin-image/src/naming.rs` with the following content (verbatim move from `crates/jackin-runtime/src/runtime/naming.rs`; replace `crate::instance::runtime_slug` with `jackin_core::runtime_slug`):

   ```rust
   //! Image tag and Docker label constants for jackin❯-built derived images.
   //!
   //! Architecture Invariant: depends only on `jackin-core` and `jackin-manifest`.

   use jackin_core::runtime_slug;
   use jackin_core::selector::RoleSelector;

   /// Prefix for jackin-managed Docker image names.
   pub(crate) const IMAGE_PREFIX: &str = "jk_";

   pub const LABEL_IMAGE_CONSTRUCT: &str = "jackin.construct.image";
   pub const LABEL_IMAGE_CONSTRUCT_VERSION: &str =
       jackin_manifest::LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION;
   pub const LABEL_IMAGE_ROLE_GIT_SHA: &str =
       jackin_manifest::LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA;
   pub const LABEL_IMAGE_RECIPE_HASH: &str = "jackin.image.recipe.hash";
   pub const LABEL_IMAGE_RECIPE_VERSION: &str = "jackin.image.recipe.version";
   pub const LABEL_IMAGE_AGENT_VERSION_PREFIX: &str = "jackin.agent";

   const SHORT_GIT_SHA_LEN: usize = 7;

   pub fn short_git_sha(sha: &str) -> &str {
       &sha[..sha.len().min(SHORT_GIT_SHA_LEN)]
   }

   fn tag_with_sha(name: String, role_git_sha: Option<&str>) -> String {
       match role_git_sha.filter(|sha| !sha.is_empty() && sha.bytes().all(|b| b.is_ascii_hexdigit())) {
           Some(sha) => format!("{name}:{}", short_git_sha(sha)),
           None => name,
       }
   }

   pub fn image_name(selector: &RoleSelector, role_git_sha: Option<&str>) -> String {
       tag_with_sha(
           format!("{IMAGE_PREFIX}{}", runtime_slug(selector)),
           role_git_sha,
       )
   }

   pub fn image_name_for_branch(
       selector: &RoleSelector,
       branch: &str,
       role_git_sha: Option<&str>,
   ) -> String {
       let slug = branch.replace('/', "-").to_ascii_lowercase();
       tag_with_sha(
           format!("{IMAGE_PREFIX}{}_{slug}", runtime_slug(selector)),
           role_git_sha,
       )
   }

   pub fn role_base_image_name(
       selector: &RoleSelector,
       branch: Option<&str>,
       role_git_sha: Option<&str>,
   ) -> String {
       let repo = match branch {
           Some(b) => {
               let slug = b.replace('/', "-").to_ascii_lowercase();
               format!("{IMAGE_PREFIX}{}_{slug}__base", runtime_slug(selector))
           }
           None => format!("{IMAGE_PREFIX}{}__base", runtime_slug(selector)),
       };
       tag_with_sha(repo, role_git_sha)
   }
   ```

   Visibility changes vs. source:
   - `IMAGE_PREFIX`: was `pub(super)` in naming.rs → `pub(crate)` in jackin-image (used by sibling modules within jackin-image only; kept crate-private so external crates don't depend on the prefix string)
   - `LABEL_IMAGE_CONSTRUCT`, `LABEL_IMAGE_CONSTRUCT_VERSION`, `LABEL_IMAGE_ROLE_GIT_SHA`, `LABEL_IMAGE_RECIPE_HASH`, `LABEL_IMAGE_RECIPE_VERSION`, `LABEL_IMAGE_AGENT_VERSION_PREFIX`: were `pub(super)` → `pub` (they are the stable label-key strings that jackin-runtime tests use via `crate::runtime::image::*` re-exports and that callers outside jackin-image may eventually need)
   - `image_name`, `image_name_for_branch`, `role_base_image_name`, `short_git_sha`: were `pub(super)` → `pub`
   - `SHORT_GIT_SHA_LEN`, `tag_with_sha`: private within the module (no pub)

**PART D — Remove moved naming symbols from `crates/jackin-runtime/src/runtime/naming.rs`**

9. In `crates/jackin-runtime/src/runtime/naming.rs`, delete the following symbols (they now live in `jackin_image::naming`):
   - `IMAGE_PREFIX` const (line 10)
   - `LABEL_IMAGE_CONSTRUCT` const (line 37)
   - `LABEL_IMAGE_CONSTRUCT_VERSION` const (lines 45–46)
   - `LABEL_IMAGE_ROLE_GIT_SHA` const (lines 63–64)
   - `LABEL_IMAGE_RECIPE_HASH` const (line 70)
   - `LABEL_IMAGE_RECIPE_VERSION` const (line 75)
   - `LABEL_IMAGE_AGENT_VERSION_PREFIX` const (line 81)
   - `SHORT_GIT_SHA_LEN` const (line 128)
   - `short_git_sha` fn (lines 131–133)
   - `tag_with_sha` fn (lines 146–151)
   - `image_name` fn (lines 119–124)
   - `image_name_for_branch` fn (lines 159–169)
   - `role_base_image_name` fn (lines 179–192)

   Keep: `LABEL_MANAGED`, `LABEL_KIND_ROLE`, `LABEL_KIND_DIND`, `LABEL_KIND_PREWARM_DIND`, `LABEL_PREWARM`, `LABEL_KEEP_AWAKE`, `LABEL_ROLE_KEY`, `LABEL_IMAGE_KEY`, `format_role_display`, `matching_family`, `dind_certs_volume`, `dind_container_name`, `role_network_name`.

   The `use crate::instance::runtime_slug;` import at the top of naming.rs (line 3) is no longer needed after the image-naming functions are gone. Delete it.

**PART E — Create `crates/jackin-image/src/image_recipe.rs` and its tests**

10. Create `crates/jackin-image/src/image_recipe.rs`. Move verbatim from `crates/jackin-runtime/src/runtime/image.rs` the following symbol cluster, updating `super::naming::*` imports to `use crate::naming::*`:

    Symbols to move:
    - Constants: `IMAGE_RECIPE_VERSION` (`&str = "v7"`, line 47), `LABEL_IMAGE_CAPSULE_VERSION` (line 49), `LABEL_IMAGE_MANIFEST_VERSION` (line 52), `HOST_IDENTITY_STRATEGY` (line 55)
    - Types: `ImageRecipe` (struct, lines 115–134), `ExpectedImageRecipe` (struct, lines 165–168)
    - Functions: `build_image_recipe` (lines 561–578), `build_image_recipe_with_construct_image` (lines 580–613), `render_runtime_dockerfile` (lines 624–642), `canonical_supported_agent_slugs` (lines 644–652), `supported_set_uses_cache_bust` (lines 659–664), `cache_bust_recipe_value` (lines 666–676), `expected_image_recipes` (lines 678–700), `hooks_hash` (lines 702–721), `plugin_recipe_hash` (lines 723–738), `recipe_labels` (lines 831–842), `recipe_diagnostic_labels` (lines 844–876), `hash_str` (lines 956–958), `sha256_hex` (lines 960–968)
    - `#[cfg(test)]` helpers: `image_recipe_label_map_for_test` (lines 879–898, pub(crate)), `image_recipe_label_map_for_install_test` (lines 900–931, private), `expected_image_recipe_for_test` (lines 933–954, private)

    Visibility changes:
    - `ImageRecipe`, `ExpectedImageRecipe`: were private → `pub(crate)` (used by sibling modules within jackin-image)
    - `IMAGE_RECIPE_VERSION`, `LABEL_IMAGE_CAPSULE_VERSION`, `LABEL_IMAGE_MANIFEST_VERSION`, `HOST_IDENTITY_STRATEGY`: were module-level constants → `pub(crate)`
    - `recipe_labels`, `recipe_diagnostic_labels`: were private → `pub(crate)` (used by `image_build`)
    - `build_image_recipe`, `expected_image_recipes`, `cache_bust_recipe_value`, `supported_set_uses_cache_bust`, `render_runtime_dockerfile`: were private → `pub(crate)` (used by `image_build` and `image_decision`)
    - `hash_str`, `sha256_hex`: private within `image_recipe` (only used internally)
    - `image_recipe_label_map_for_test`: was `pub(crate)` in jackin-runtime → `pub` in jackin-image (needed by jackin-runtime's launch/tests.rs which calls `crate::runtime::image::image_recipe_label_map_for_test`; after the move it will be re-exported via `jackin-runtime`'s image.rs coordinator as `#[cfg(test)] pub(crate) use jackin_image::image_recipe::image_recipe_label_map_for_test;`)

    Import edits in `image_recipe.rs`:
    - `use super::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_ROLE_GIT_SHA, LABEL_IMAGE_CAPSULE_VERSION_label...}` → `use crate::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_ROLE_GIT_SHA, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_CONSTRUCT_VERSION};`
    - `use jackin_image::capsule_binary;` → `use crate::capsule_binary;`
    - `use jackin_image::derived_image::{AgentInstall, render_derived_dockerfile};` → `use crate::derived_image::{AgentInstall, render_derived_dockerfile};`
    - All other imports (`jackin_core`, `jackin_manifest`, `serde`, `sha2`) use their crate names unchanged.

    Add `#[cfg(test)] mod tests;` at the bottom.

11. Create `crates/jackin-image/src/image_recipe/tests.rs`. Move from `crates/jackin-runtime/src/runtime/image/tests.rs` the following test functions (those that test symbols now in `image_recipe.rs`):

    TODO(investigate): Enumerate the exact test function names that test `image_recipe` symbols vs. those that test `image_decision` / `image_build` / prewarm-coordinator symbols. A mechanical way to determine this: grep the tests file for calls to each symbol cluster. The test for `image_recipe_label_map_for_test`, `build_image_recipe`, `render_runtime_dockerfile`, `canonical_supported_agent_slugs`, `recipe_labels`, `recipe_diagnostic_labels`, `hooks_hash`, `plugin_recipe_hash` belong in `image_recipe/tests.rs`.

    The test file header (imports) in `image_recipe/tests.rs` changes from:
    ```rust
    use super::*;
    use crate::runtime::test_support::{FakeDockerClient, FakeRunner, TEST_DOCKERFILE_FROM};
    ```
    to:
    ```rust
    use super::*;
    use jackin_runtime::runtime::test_support::{FakeDockerClient, FakeRunner, TEST_DOCKERFILE_FROM};
    ```
    (The `jackin-runtime` dev-dep added in step 7 makes this possible.)

**PART F — Create `crates/jackin-image/src/image_decision.rs` and its tests**

12. Create `crates/jackin-image/src/image_decision.rs`. Move verbatim from `image.rs` the following cluster:

    Symbols to move:
    - `ImageInvalidationReason` enum + impl (lines 58–93)
    - `ImageDecision` enum + impl (lines 95–163)
    - `decide_role_image` async fn (lines 271–528)
    - `build_decision` fn (lines 530–546)
    - `decision_base_image_override` fn (lines 548–559)
    - `classify_image_labels` fn (lines 740–764)
    - `recipe_label_mismatch` fn (lines 766–779)
    - `emit_image_decision` fn (lines 781–795)
    - `emit_image_reuse` fn (lines 797–817)
    - `emit_image_refresh_background` fn (lines 819–829)
    - `PublishedImageFreshness` enum (lines 2596–2600)
    - `published_image_freshness` async fn (lines 2602–2644)
    - `published_image_is_stale` async fn (lines 2646–2656)
    - `local_role_base_labels_match` fn (lines 2658–2683)

    Visibility changes:
    - `ImageInvalidationReason`: was `pub(super)` → `pub` (exported from jackin-image; needed by jackin-runtime)
    - `ImageDecision`: was `pub(super)` → `pub`
    - `decide_role_image`: was `pub(super)` → `pub`
    - `build_decision`, `decision_base_image_override`, `classify_image_labels`, `recipe_label_mismatch`: private within `image_decision`
    - `emit_image_decision`, `emit_image_reuse`, `emit_image_refresh_background`: private within `image_decision`
    - `PublishedImageFreshness`, `published_image_freshness`, `published_image_is_stale`, `local_role_base_labels_match`: private

    Import edits in `image_decision.rs`:
    - `use super::naming::*` → `use crate::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA};`
    - `use super::progress::{LaunchProgress, LaunchStage};` → DELETE (this module does not use LaunchProgress)
    - `use super::repo_cache::*` → DELETE (this module does not use repo_cache)
    - `use crate::runtime::image::image_recipe::*` → `use crate::image_recipe::{expected_image_recipes, ExpectedImageRecipe};`
    - Add `use crate::version_check;`
    - Add `use jackin_diagnostics;`

    Add `#[cfg(test)] mod tests;` at the bottom.

13. Create `crates/jackin-image/src/image_decision/tests.rs` with the tests from `image/tests.rs` that test `image_decision` symbols (those calling `decide_role_image`, `classify_image_labels`, `published_image_freshness`, etc.).

    TODO(investigate): enumerate exact test function names per step 11 instructions.

**PART G — Create `crates/jackin-image/src/image_build.rs` and its tests**

14. Create `crates/jackin-image/src/image_build.rs`. Move verbatim from `image.rs` the following cluster (these are the functions that take `Option<&mut LaunchProgress>` — this step requires A2 to be done first):

    Symbols to move:
    - `PreparedRuntimeBinaries` struct (lines 170–175)
    - `prepare_runtime_binaries_for_agents` async fn (lines 971–1037, pub(super))
    - `prepare_agent_binaries` async fn (lines 1625–1674)
    - `agent_binary_prepare_summary` fn (lines 1676–1695)
    - `ensure_local_role_base` async fn (lines 1714–1881)
    - `build_agent_image` async fn (lines 1888–2221, pub(super))
    - `role_git_sha_for_recipe` async fn (lines 2235–2254, pub(super))
    - `git_head_sha` async fn (lines 2225–2233)
    - `should_stream_build_output` fn (lines 2256–2258)
    - `local_image_output_arg` fn (lines 2260–2262)
    - `emit_non_containerd_image_store_note` async fn (lines 2264–2291)
    - `docker_info_uses_containerd_store` fn (lines 2293–2300)
    - `docker_build_env` fn (lines 2302–2313)
    - `BuildContextStats` struct (lines 2315–2319)
    - `emit_build_context_snapshot` fn (lines 2321–2348)
    - `ImageBuildSourceDiagnostic` struct (lines 2350–2356)
    - `emit_image_build_source` fn (lines 2358–2379)
    - `build_context_stats` fn (lines 2381–2385)
    - `collect_build_context_stats` fn (lines 2387–2405)
    - `dockerfile_requests_github_token_secret` fn (lines 2407–2419)
    - `dockerfile_body_requests_github_token_secret` fn (lines 2421–2426)
    - `dockerfile_requests_role_git_sha_arg` fn (lines 2428–2440)
    - `dockerfile_body_requests_role_git_sha_arg` fn (lines 2442–2457)
    - `emit_docker_build_step_diagnostics` fn (lines 2459–2470)
    - `DockerBuildStep` struct (lines 2472–2478)
    - `parse_docker_build_steps` fn (lines 2480–2496)
    - `parse_buildkit_line` fn (lines 2498–2509)
    - `is_buildkit_step_description` fn (lines 2511–2519)
    - `parse_completed_buildkit_step` fn (lines 2521–2541)
    - `split_buildkit_duration` fn (lines 2543–2551)
    - `parse_buildkit_duration_ms` fn (lines 2553–2570)
    - `emit_compact_image_warning` fn (lines 2572–2574)
    - `compact_image_warning_line` fn (lines 2576–2578)
    - `extract_agent_version` async fn (lines 2688–2746)
    - `record_built_agent_version` async fn (lines 2748–2784)
    - `resolve_github_token` async fn (lines 2793–2809)

    Visibility changes:
    - `PreparedRuntimeBinaries`: was `pub(super)` → `pub` (needed by jackin-runtime's prewarm coordinator)
    - `prepare_runtime_binaries_for_agents`: was `pub(super)` → `pub`
    - `build_agent_image`: was `pub(super)` → `pub`
    - `role_git_sha_for_recipe`: was `pub(super)` → `pub`
    - All others: `pub(crate)` for things used by sibling modules, private for pure internal helpers

    Import edits in `image_build.rs`:
    - `use super::progress::{LaunchProgress, LaunchStage};` → `use jackin_core::{LaunchProgress, LaunchStage};` (exact path depends on A2 outcome — TODO(investigate))
    - `use super::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_ROLE_GIT_SHA, LABEL_IMAGE_AGENT_VERSION_PREFIX, image_name, image_name_for_branch, role_base_image_name, short_git_sha};` → `use crate::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_ROLE_GIT_SHA, LABEL_IMAGE_AGENT_VERSION_PREFIX, image_name, image_name_for_branch, role_base_image_name, short_git_sha};`
    - `use crate::image_recipe::{build_image_recipe, recipe_labels, supported_set_uses_cache_bust, cache_bust_recipe_value};`
    - `use crate::image_decision::ImageInvalidationReason;`
    - `use jackin_image::capsule_binary;` → `use crate::capsule_binary;`
    - `use jackin_image::derived_image::{AgentInstall, create_derived_build_context_for_agents, create_role_base_build_context};` → `use crate::derived_image::{AgentInstall, create_derived_build_context_for_agents, create_role_base_build_context};`
    - `use jackin_image::version_check;` → `use crate::version_check;`
    - `use futures_util::future::try_join_all;` stays (now in jackin-image's own dep)
    - `use jackin_core::agent::Agent;`, `use jackin_core::paths::JackinPaths;`, `use jackin_core::{CommandRunner, RunOptions};` stay unchanged
    - `use jackin_docker::docker_client::DockerApi;` stays
    - `#[cfg(not(test))] use jackin_docker::ShellRunner;` → remove (ShellRunner is not used in image_build itself; it was used by the prewarm functions that stay in jackin-runtime)
    - `#[cfg(not(test))] use jackin_docker::docker_client::BollardDockerClient;` → remove (same reason)
    - `use jackin_manifest::repo::CachedRepo;` stays
    - `use super::progress::*` references to LaunchProgress inside `ensure_local_role_base` and `build_agent_image` become `LaunchProgress` from the new import

    Add `#[cfg(test)] mod tests;` at the bottom.

15. Create `crates/jackin-image/src/image_build/tests.rs` with the tests from `image/tests.rs` that test `image_build` symbols (those calling `prepare_runtime_binaries_for_agents`, `build_agent_image`, `ensure_local_role_base`, `docker_build_env`, `docker_info_uses_containerd_store`, `role_git_sha_for_recipe`, `dockerfile_body_requests_*`, `parse_docker_build_steps`, etc.).

    TODO(investigate): enumerate exact test function names per step 11 instructions.

**PART H — Update `crates/jackin-image/src/lib.rs`**

16. Add module declarations and crate-root re-exports to `crates/jackin-image/src/lib.rs`:

    ```rust
    pub mod image_build;
    pub mod image_decision;
    pub mod image_recipe;
    pub(crate) mod naming;
    ```

    Add to crate-root re-exports:
    ```rust
    pub use image_build::{PreparedRuntimeBinaries, build_agent_image, prepare_runtime_binaries_for_agents, role_git_sha_for_recipe};
    pub use image_decision::{ImageDecision, ImageInvalidationReason, decide_role_image};
    pub use naming::{LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, image_name, image_name_for_branch, role_base_image_name, short_git_sha};
    #[cfg(test)]
    pub use image_recipe::image_recipe_label_map_for_test;
    ```

    Update the existing `//!` Architecture-Invariant header to note the new allowed deps (add `jackin-launch` progress types if they landed in a new crate, or confirm `jackin-core` for those).

**PART I — Reduce `crates/jackin-runtime/src/runtime/image.rs` to a thin prewarm coordinator**

17. In `crates/jackin-runtime/src/runtime/image.rs`, replace the entire file with the following structure:

    Keep verbatim:
    - `ImagePrewarmStatus` enum (pub, lines 178–183)
    - `RoleImagePrewarmRow` struct (pub, lines 186–193)
    - `prewarm_role_images` async fn (pub, #[cfg(not(test))], lines 202–269) — already uses `super::repo_cache`; keep as-is
    - `prewarm_agent_image` async fn (#[cfg(not(test))], lines 1386–1418)
    - `prewarm_agent_image_from_validated_repo` async fn (lines 1420–1568)
    - `prewarm_sibling_images_concurrently` async fn (#[cfg(not(test))], lines 1235–1277)
    - `prewarm_sibling_image` async fn (#[cfg(not(test))], lines 1586–1623)
    - `SiblingImagePrewarmOutcome` enum (#[cfg(not(test))], lines 1380–1383)
    - `spawn_sibling_runtime_prewarm` fn (pub(super), lines 1039–1122) — NOTE: this calls `super::launch::emit_prewarm_launch_plan`; keep as-is since it stays in jackin-runtime
    - `spawn_sibling_image_prewarm` fn (pub(super), lines 1124–1233)
    - `spawn_selected_image_refresh` fn (pub(super), lines 1279–1365)
    - `sibling_agents` fn (lines 1367–1377)
    - `prewarm_launch_plan_reason` fn (lines 1570–1583)

    Add re-exports from jackin-image at the top of the imports section (these allow all existing `crate::runtime::image::*` call sites in jackin-runtime to work unchanged):
    ```rust
    pub(super) use jackin_image::{
        ImageDecision, ImageInvalidationReason, PreparedRuntimeBinaries,
        build_agent_image, decide_role_image, prepare_runtime_binaries_for_agents,
        role_git_sha_for_recipe,
    };
    pub(super) use jackin_image::{
        LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CONSTRUCT,
        LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_RECIPE_HASH,
        LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA,
        image_name, image_name_for_branch, role_base_image_name, short_git_sha,
    };
    #[cfg(test)]
    pub(crate) use jackin_image::image_recipe_label_map_for_test;
    ```

    Remove from the imports:
    - `use super::naming::{...image naming...}` (all the image naming symbols now come from the re-export above)
    - `use super::progress::{LaunchProgress, LaunchStage};` (no longer needed; the staying functions don't use LaunchProgress)
    - `use jackin_image::capsule_binary;`, `use jackin_image::derived_image::{...};`, `use jackin_image::version_check;` (those are now used inside jackin-image itself, not here)
    - `use futures_util::future::try_join_all;`
    - `use serde::Serialize;`
    - `use sha2::{Digest as _, Sha256};`
    - `use std::collections::{BTreeMap, HashMap};` (check if any staying function still needs these; `prewarm_agent_image_from_validated_repo` doesn't; if not needed, remove)

    Keep in imports:
    - `use super::naming::{LABEL_IMAGE_ROLE_GIT_SHA, image_name, image_name_for_branch};` — WAIT: these are now re-exported above from jackin-image, so the `use super::naming::` import is gone; all these come from `jackin_image::*`
    - `use super::launch::emit_prewarm_launch_plan;` — stays (used in `prewarm_agent_image_from_validated_repo` and `spawn_sibling_runtime_prewarm`)
    - `use super::progress::{LaunchProgress, LaunchStage};` — DELETE (already noted above; staying functions don't need it)
    - `use super::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};` — stays (used by the prewarm functions)
    - `#[cfg(not(test))] use jackin_docker::ShellRunner;` — stays (used by prewarm_agent_image and prewarm_sibling_image)
    - `#[cfg(not(test))] use jackin_docker::docker_client::BollardDockerClient;` — stays
    - `use jackin_core::agent::Agent;`, `use jackin_core::paths::JackinPaths;`, `use jackin_core::selector::RoleSelector;` — stays (used by prewarm functions)
    - `use jackin_manifest::repo::CachedRepo;` — stays

    Add `#[cfg(test)] mod tests;` at the bottom (already exists).

**PART J — Prune `crates/jackin-runtime/src/runtime/image/tests.rs`**

18. Remove from `crates/jackin-runtime/src/runtime/image/tests.rs` the test functions that have been moved to jackin-image's test files (steps 13 and 15). Keep only the tests for:
    - `spawn_sibling_runtime_prewarm`
    - `spawn_sibling_image_prewarm`
    - `spawn_selected_image_refresh`
    - `prewarm_agent_image_from_validated_repo`

    TODO(investigate): Enumerate the exact function names from `image/tests.rs` that must stay vs. move. A mechanical process: for every `#[test]` / `#[tokio::test]` function in `image/tests.rs`, grep its body for the primary symbol it exercises and place it in the matching destination. Known staying tests (by calling `spawn_sibling_runtime_prewarm`): `sibling_runtime_prewarm_runs_in_background` (line 324), `sibling_runtime_prewarm_skips_when_image_was_rebuilt` (line 379). Known staying tests (prewarm_agent_image_from_validated_repo): lines 1536 and 1630 call sites.

**PART K — Update `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`**

19. In `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` line 23, change:
    ```rust
    use crate::runtime::naming::{image_name, image_name_for_branch};
    ```
    to:
    ```rust
    use jackin_image::{image_name, image_name_for_branch};
    ```
    (These functions moved to jackin-image in step 8 and are re-exported at the crate root in step 16.)

    Lines 744–745 in `launch_pipeline.rs` that call `image_name(selector, None)` and `image_name_for_branch(selector, b, None)` need no change since the names are the same.

**PART L — Update `file-size-budget.toml`**

20. Run `cargo xtask lint files --print-budget` to get the refreshed budget after the move. The entry for `crates/jackin-runtime/src/runtime/image.rs` (currently `lines = 2812`) will drop significantly. If the reduced `image.rs` falls below 2000 lines, delete its entry from `file-size-budget.toml`. The new files in jackin-image (`image_build.rs`, `image_decision.rs`, `image_recipe.rs`) start below the 2000-line cap so they need no entries.

**PART M — Update crate headers and docs**

21. Update `crates/jackin-image/src/lib.rs` `//!` header to list the new allowed deps (add `jackin-manifest`, `jackin-docker`, `jackin-diagnostics` if not already listed; clarify that `jackin-launch` is NOT a dep — the progress types come from `jackin-core` after A2).

22. Update `crates/jackin-runtime/src/lib.rs` `//!` header: remove "image build" from the crate's stated responsibilities (it now lives in jackin-image).

23. Check `docs/content/docs/reference/getting-oriented/codebase-map/` (or equivalent contributor doc) and update the `jackin-image` crate description to include image-build orchestration.

24. Check `PROJECT_STRUCTURE.md` for any mention of `runtime/image.rs` and update to the new home.

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-image` → all pass
- `cargo nextest run -p jackin-runtime` → all pass
- `cargo nextest run --workspace` → all pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + arch gates green; if image.rs dropped under the 2000L production cap, also run `cargo run -p jackin-xtask --locked -- lint files --print-budget` and delete the entry from `file-size-budget.toml`
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
1. `crates/jackin-runtime/src/runtime/image.rs` contains only the prewarm coordinator (prewarm_role_images, prewarm_agent_image, prewarm_agent_image_from_validated_repo, spawn_sibling_*, spawn_selected_*, sibling_agents, prewarm_launch_plan_reason, SiblingImagePrewarmOutcome, ImagePrewarmStatus, RoleImagePrewarmRow) — all build/recipe/decision logic is gone.
2. `crates/jackin-image/src/` contains `naming.rs`, `image_recipe.rs`, `image_decision.rs`, `image_build.rs` and their sibling `tests.rs` files.
3. `crates/jackin-runtime/src/runtime/naming.rs` no longer declares any `LABEL_IMAGE_*` constants or any of the image-naming functions.
4. `cargo xtask lint files` does not flag any of the new jackin-image files (all start under the 2000-line production cap).
5. All workspace tests pass unmodified.

**Rollback:** `git restore crates/jackin-image/ crates/jackin-runtime/ crates/jackin-core/ file-size-budget.toml` and delete the new files: `git rm crates/jackin-image/src/naming.rs crates/jackin-image/src/image_recipe.rs crates/jackin-image/src/image_decision.rs crates/jackin-image/src/image_build.rs` and their `tests.rs` siblings.

**Open questions:**
1. **A2 exact outcome (hard blocker):** After A2 lands, what are the exact Rust paths for `LaunchProgress` and `LaunchStage`? The playbook writes `use jackin_core::{LaunchProgress, LaunchStage};` but the actual path depends on how A2 is implemented. Executor must verify before writing that import in `image_build.rs` (step 14).
2. **Dev-dep cycle viability:** The playbook adds `jackin-runtime` as a `[dev-dependency]` of `jackin-image` with `features = ["test-support"]` so that test utilities (`FakeDockerClient`, `FakeRunner`, `TEST_DOCKERFILE_FROM`) are accessible to jackin-image's tests. Cargo generally permits this when the forward direction is a regular dep; confirm with `cargo check --workspace` immediately after step 7. If Cargo rejects it, the fallback is to move `FakeDockerClient`, `FakeRunner`, `TEST_DOCKERFILE_FROM` into jackin-image itself or into a shared `jackin-test-fixtures` crate.
3. **Exact test-function partitioning (step 11 / 15 / 18):** The `image/tests.rs` file (2111 lines) mixes tests for functions across all three destination clusters. Before writing the partitioned test files, manually inspect each `#[test]` / `#[tokio::test]` function in `image/tests.rs` and classify it by the primary symbol it calls. Do NOT guess — misplacing a test that calls a staying function into a moved test file will break compilation.
4. **`runtime_slug` re-export path in `jackin-core`:** The playbook adds `pub use selector::runtime_slug;` to `jackin-core/src/lib.rs` so callers can write `jackin_core::runtime_slug`. Confirm that `selector` is the correct module name in `jackin-core` (it is `src/selector.rs` per current inspection, so `selector::runtime_slug` is correct) and that no existing `pub use` in `lib.rs` conflicts.
5. **`LABEL_IMAGE_KEY` vs. the moved `LABEL_IMAGE_*` set:** `LABEL_IMAGE_KEY` (`"jackin.image"`, a container label that records the derived image name on running containers) is used by `cleanup.rs` and stays in `runtime/naming.rs`. It is NOT moved. Confirm this is intentional — the constant name starts with `LABEL_IMAGE_` but it is semantically a container attribute, not an image attribute, and `cleanup.rs` must continue to reach it via `super::naming::LABEL_IMAGE_KEY`.

---

### D2 — Dedup env-resolution homes (P5)

- **Goal:** Make `jackin-core::env_model` the single canonical home for the env-model vocabulary (constants, `is_reserved`, `extract_interpolation_refs`, `topological_env_order`), and delete the `jackin/src/env_model.rs` shim module that creates a redundant second home inside the binary crate.
- **Preconditions:** none (boundary fix A0 already shipped; this is a pure shim deletion)
- **Pattern:** Parallel Change — migrate the two call sites to the canonical path, move tests to the canonical location, then delete the shim.
- **Touches:**
  - `crates/jackin-core/src/env_model.rs` (modified: replace inline `#[cfg(test)] mod tests { ... }` block with `#[cfg(test)] mod tests;` declaration)
  - `crates/jackin-core/src/env_model/tests.rs` (created: receives all tests from both sources)
  - `crates/jackin/src/env_model.rs` (deleted)
  - `crates/jackin/src/env_model/tests.rs` (deleted)
  - `crates/jackin/src/lib.rs` (modified: remove `pub(crate) mod env_model;` line 32)
  - `crates/jackin/src/app/config_cmd.rs` (modified: line 275)
  - `crates/jackin/src/app/workspace_cmd.rs` (modified: line 555)
  - `test-layout-allowlist.toml` (modified: remove the `crates/jackin-core/src/env_model.rs` entry)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Create the file `crates/jackin-core/src/env_model/tests.rs` with exactly the following content (combines the 2 inline tests from `jackin-core/src/env_model.rs` lines 185–200 with the 11 tests from `jackin/src/env_model/tests.rs`, deduplicated by test-function name; no duplicate names exist across the two sources):

```rust
use super::*;

#[test]
fn open_links_allowed_accepts_unset_and_non_deny_values() {
    assert!(open_links_allowed(None));
    assert!(open_links_allowed(Some("")));
    assert!(open_links_allowed(Some("allow")));
    assert!(open_links_allowed(Some("yes")));
}

#[test]
fn open_links_allowed_rejects_deny_values() {
    for value in ["deny", "off", "no"] {
        assert!(!open_links_allowed(Some(value)));
    }
}

#[test]
fn reserved_runtime_env_vars_covers_every_previously_reserved_name() {
    // Each name previously in manifest::RESERVED_RUNTIME_ENV_VARS
    // AND in runtime's old RUNTIME_OWNED_ENV_VARS must be present.
    let names: Vec<&str> = RESERVED_RUNTIME_ENV_VARS.iter().map(|(n, _)| *n).collect();
    for sentinel in &[
        "JACKIN",               // in-container sentinel (was JACKIN)
        "JACKIN_DIND_HOSTNAME", // was manifest JACKIN_DIND_HOSTNAME_ENV_NAME value
        "JACKIN_CONTAINER_NAME",
        "JACKIN_INSTANCE_ID",
        "JACKIN_AGENT", // injected per agent session — agent slug (claude/codex/amp)
        "JACKIN_AGENT_CODENAME", // unique per-tab codename, never reused in container lifetime
        "JACKIN_ROLE",  // runtime-owned role selector key
        "JACKIN_GIT_COAUTHOR_TRAILER",
        "JACKIN_GIT_DCO",
        "DOCKER_HOST",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "TESTCONTAINERS_HOST_OVERRIDE",
    ] {
        assert!(
            names.contains(sentinel),
            "reserved env list must include {sentinel} for previous manifest/runtime coverage"
        );
    }
}

#[test]
fn is_reserved_accepts_all_sentinel_names() {
    for sentinel in &[
        "JACKIN",
        "JACKIN_DIND_HOSTNAME",
        "JACKIN_CONTAINER_NAME",
        "JACKIN_INSTANCE_ID",
        "JACKIN_AGENT",
        "JACKIN_AGENT_CODENAME",
        "JACKIN_ROLE",
        "JACKIN_GIT_COAUTHOR_TRAILER",
        "JACKIN_GIT_DCO",
        "DOCKER_HOST",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "TESTCONTAINERS_HOST_OVERRIDE",
    ] {
        assert!(
            is_reserved(sentinel),
            "{sentinel} must be recognized as reserved"
        );
    }
}

#[test]
fn is_reserved_rejects_user_names() {
    assert!(!is_reserved("MY_USER_VAR"));
    assert!(!is_reserved("PATH"));
    assert!(!is_reserved(""));
}

#[test]
fn jackin_git_dco_is_reserved() {
    assert!(
        is_reserved(JACKIN_GIT_DCO_ENV_NAME),
        "JACKIN_GIT_DCO must be reserved so manifests cannot override the DCO hook signal"
    );
}

#[test]
fn extract_interpolation_refs_finds_single_ref() {
    assert_eq!(
        extract_interpolation_refs("Branch for ${env.PROJECT}:"),
        vec!["PROJECT"]
    );
}

#[test]
fn extract_interpolation_refs_finds_multiple_refs() {
    assert_eq!(
        extract_interpolation_refs("${env.TEAM}/${env.PROJECT}"),
        vec!["TEAM", "PROJECT"]
    );
}

#[test]
fn extract_interpolation_refs_returns_empty_for_no_refs() {
    assert!(extract_interpolation_refs("plain text").is_empty());
}

#[test]
fn extract_interpolation_refs_ignores_non_env_namespace() {
    assert!(extract_interpolation_refs("${other.FOO}").is_empty());
    assert!(extract_interpolation_refs("${FOO}").is_empty());
}

#[test]
fn extract_interpolation_refs_returns_empty_name_for_empty_env_ref() {
    assert_eq!(extract_interpolation_refs("${env.}"), vec![""]);
}

#[test]
fn extract_interpolation_refs_handles_unclosed_brace() {
    assert!(extract_interpolation_refs("${env.OPEN").is_empty());
}

#[test]
fn topological_env_order_is_deterministic_for_independent_prompts() {
    fn decl(depends_on: &[&str]) -> crate::manifest::EnvVarDecl {
        crate::manifest::EnvVarDecl {
            default_value: None,
            interactive: true,
            skippable: false,
            prompt: None,
            options: Vec::new(),
            depends_on: depends_on.iter().map(|dep| (*dep).to_owned()).collect(),
        }
    }

    let declarations = std::collections::BTreeMap::from([
        ("BRANCH".to_owned(), decl(&["env.SELECT_PROJECT"])),
        ("FREE_TEXT".to_owned(), decl(&[])),
        ("SELECT_PROJECT".to_owned(), decl(&[])),
    ]);

    assert_eq!(
        topological_env_order(&declarations).unwrap(),
        ["FREE_TEXT", "SELECT_PROJECT", "BRANCH"]
    );
}
```

2. Edit `crates/jackin-core/src/env_model.rs`: replace the entire `#[cfg(test)] mod tests { … }` inline block (lines 183–201 — the block starting with `#[cfg(test)]` and ending with the closing `}`) with the single line:

```rust
#[cfg(test)]
mod tests;
```

3. Edit `crates/jackin/src/app/config_cmd.rs` line 275: replace

```rust
                if crate::env_model::is_reserved(&key) {
```

with

```rust
                if jackin_core::env_model::is_reserved(&key) {
```

4. Edit `crates/jackin/src/app/workspace_cmd.rs` line 555: replace

```rust
                if crate::env_model::is_reserved(&key) {
```

with

```rust
                if jackin_core::env_model::is_reserved(&key) {
```

5. Edit `crates/jackin/src/lib.rs` line 32: remove the line

```rust
pub(crate) mod env_model;
```

(The surrounding lines are `pub mod docker_client;` on line 31 and `pub mod env_resolver;` on line 33; after removal, those two lines become adjacent.)

6. Delete the file `crates/jackin/src/env_model.rs` (8 lines: the shim `pub(crate) use jackin_core::env_model::*;` and its `#[cfg(test)] mod tests;` declaration).

   Command: `git rm crates/jackin/src/env_model.rs`

7. Delete the file `crates/jackin/src/env_model/tests.rs` (130 lines: the tests now consolidated in step 1).

   Command: `git rm crates/jackin/src/env_model/tests.rs`

   After deletion the directory `crates/jackin/src/env_model/` becomes empty and Git removes it automatically.

8. Edit `test-layout-allowlist.toml`: remove the line

```
    "crates/jackin-core/src/env_model.rs",
```

   (currently line 21 of the file). This entry was there because `jackin-core/src/env_model.rs` had inline `#[cfg(test)] mod tests { ... }` — that violation is now fixed by step 2.

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → exits 0, no formatting diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exits 0, zero warnings
- `cargo nextest run -p jackin-core` → all tests pass (confirms the moved tests compile and pass in their new home)
- `cargo nextest run -p jackin` → all tests pass (confirms `config_cmd`, `workspace_cmd`, and integration tests compile and pass without the shim)
- `cargo nextest run --workspace` → all tests pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout gates green; the `crates/jackin-core/src/env_model.rs` entry no longer appears in the test-layout violation list (it was removed from the allowlist in step 8 and the file is now compliant)
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → run only if `crates/jackin-core/src/env_model.rs` was in `file-size-budget.toml` (it is NOT — confirmed by grep returning no output); skip if not listed
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `crates/jackin/src/env_model.rs` and `crates/jackin/src/env_model/tests.rs` no longer exist; `cargo nextest run --workspace` is green; `cargo xtask lint` is green; `grep -r 'crate::env_model' crates/jackin/src/` returns no output; `crates/jackin-core/src/env_model/tests.rs` exists and all 13 test functions are present.

**Rollback:** `git restore crates/jackin-core/src/env_model.rs crates/jackin/src/lib.rs crates/jackin/src/app/config_cmd.rs crates/jackin/src/app/workspace_cmd.rs test-layout-allowlist.toml && git restore --staged crates/jackin/src/env_model.rs crates/jackin/src/env_model/tests.rs && git checkout -- crates/jackin/src/env_model.rs crates/jackin/src/env_model/tests.rs && rm -f crates/jackin-core/src/env_model/tests.rs`

**Open questions:** none

---

### D3 — Dedup op_cache (P5)

- **Goal:** Collapse the two `op_cache` homes to one — keep the canonical generic implementation in `jackin-core`, delete the thin re-export shim in `jackin-console`, and repoint the single in-crate caller.
- **Preconditions:** none
- **Pattern:** Parallel Change (caller migrates to the canonical path; shim is deleted)
- **Touches:** `crates/jackin-console/src/op_cache.rs` (deleted), `crates/jackin-console/src/lib.rs` (mod declaration removed), `crates/jackin-console/src/tui/components/op_picker.rs` (type alias path updated); `crates/jackin-core/src/op_cache.rs` and its `tests.rs` are untouched

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Edit `crates/jackin-console/src/tui/components/op_picker.rs`, lines 341–343.**
   Current text (lines 341–343):
   ```rust
   /// Session-scoped metadata cache for picker drill-down panes.
   pub type OpPickerCache =
       crate::op_cache::OpCache<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;
   ```
   Replace the path `crate::op_cache::OpCache` with `jackin_core::op_cache::OpCache`:
   ```rust
   /// Session-scoped metadata cache for picker drill-down panes.
   pub type OpPickerCache =
       jackin_core::op_cache::OpCache<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;
   ```
   No other change in this file. (`jackin-console` already lists `jackin-core` as a direct dependency in its `Cargo.toml`.)

2. **Edit `crates/jackin-console/src/lib.rs`, line 13.**
   Remove the line:
   ```rust
   pub mod op_cache;
   ```
   The surrounding lines are `pub mod mount_info_cache;` (line 12) and `pub mod services;` (line 14). After removal those two lines become adjacent. No other change in this file.

3. **Delete `crates/jackin-console/src/op_cache.rs`.**
   Run:
   ```sh
   git rm crates/jackin-console/src/op_cache.rs
   ```
   The file contains only the 7-line re-export `pub use jackin_core::op_cache::{DEFAULT_ACCOUNT_KEY, OpCache};`. Deleting it is safe because no code outside `jackin-console` imports `jackin_console::op_cache` (verified by grep), and the one in-crate caller (`op_picker.rs`) was migrated in step 1.

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exits 0, no output
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exits 0, no warnings
- `cargo nextest run -p jackin-core` → all tests pass (canonical impl untouched)
- `cargo nextest run -p jackin-console` → all tests pass (op-picker and state tests unchanged)
- `cargo nextest run --workspace` → full suite green
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + arch gates all pass
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `find crates/jackin-console/src -name "op_cache.rs"` returns no output AND `cargo nextest run --workspace` exits 0.

**Rollback:** `git restore crates/jackin-console/src/lib.rs crates/jackin-console/src/tui/components/op_picker.rs && git checkout -- crates/jackin-console/src/op_cache.rs`

**Open questions:** none

---

### D4 — Dedup resource naming (P5)

- **Goal:** Consolidate the three derived-resource-naming helpers (`dind_certs_volume`, `dind_container_name`, `role_network_name`) into the canonical home `crates/jackin-runtime/src/instance/naming.rs`, removing them from `crates/jackin-runtime/src/runtime/naming.rs`, so that every container-name–derived string comes from one module.
- **Preconditions:** none
- **Pattern:** Parallel Change — add symbols to the new home first, repoint all callers, then delete from the old home (done in a single atomic commit since both homes are in the same crate and `pub(crate)` visibility means the compiler enforces consistency at compile time).
- **Touches:** `crates/jackin-runtime` only — exactly 7 files modified, 0 files created/deleted.

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Add the three functions to `crates/jackin-runtime/src/instance/naming.rs`.**

   In the file, locate the exact block (lines 133–145 in the current file):
   ```rust
   fn random_instance_id() -> String {
       const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
       let mut value = rand::random::<u64>();
       let mut id = String::with_capacity(INSTANCE_ID_LEN);
       for _ in 0..INSTANCE_ID_LEN {
           id.push(ALPHABET[(value & 0b1_1111) as usize] as char);
           value >>= 5;
       }
       id
   }

   #[cfg(test)]
   mod tests;
   ```

   Replace with:
   ```rust
   fn random_instance_id() -> String {
       const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
       let mut value = rand::random::<u64>();
       let mut id = String::with_capacity(INSTANCE_ID_LEN);
       for _ in 0..INSTANCE_ID_LEN {
           id.push(ALPHABET[(value & 0b1_1111) as usize] as char);
           value >>= 5;
       }
       id
   }

   /// Docker volume name for the TLS client certificates shared between the
   /// `DinD` sidecar (writer) and the role container (reader).
   pub(crate) fn dind_certs_volume(container_name: &str) -> String {
       format!("{container_name}-dind-certs")
   }

   pub(crate) fn dind_container_name(container_name: &str) -> String {
       format!("{container_name}-dind")
   }

   pub(crate) fn role_network_name(container_name: &str) -> String {
       format!("{container_name}-net")
   }

   #[cfg(test)]
   mod tests;
   ```

2. **Move the `dind_certs_volume` test to `crates/jackin-runtime/src/instance/naming/tests.rs`.**

   In `instance/naming/tests.rs`, locate the exact block at the end of the file:
   ```rust
       assert!(name.len() <= 58, "{name}");
       assert!(is_dns_label(&format!("{name}-dind")));
   }


   ```

   Replace with:
   ```rust
       assert!(name.len() <= 58, "{name}");
       assert!(is_dns_label(&format!("{name}-dind")));
   }

   #[test]
   fn dind_certs_volume_derives_from_container_name() {
       assert_eq!(
           dind_certs_volume("jk-agent-smith"),
           "jk-agent-smith-dind-certs"
       );
       assert_eq!(
           dind_certs_volume("jk-k7p9m2xq-chainargos-thearchitect"),
           "jk-k7p9m2xq-chainargos-thearchitect-dind-certs"
       );
   }
   ```

   (The `use super::*;` at line 2 of `instance/naming/tests.rs` already imports all items from `instance/naming.rs`, including `pub(crate)` items, so no import line is needed.)

3. **Update `crates/jackin-runtime/src/instance/manifest.rs`: repoint three call sites and the doc comment.**

   Locate the exact block (lines 70–79 in the current file):
   ```rust
       /// Invariant: all derived names follow the same suffix conventions used
       /// by `runtime::naming` helpers, so `docker inspect` on any of the four
       /// names produces results consistent with the naming registry.
       pub fn from_container_name(container_name: &str) -> Self {
           Self {
               role_container: container_name.to_owned(),
               dind_container: Some(crate::runtime::naming::dind_container_name(container_name)),
               network: crate::runtime::naming::role_network_name(container_name),
               certs_volume: Some(crate::runtime::naming::dind_certs_volume(container_name)),
           }
   ```

   Replace with:
   ```rust
       /// Invariant: all derived names follow the same suffix conventions used
       /// by `instance::naming` helpers, so `docker inspect` on any of the four
       /// names produces results consistent with the naming registry.
       pub fn from_container_name(container_name: &str) -> Self {
           Self {
               role_container: container_name.to_owned(),
               dind_container: Some(crate::instance::naming::dind_container_name(container_name)),
               network: crate::instance::naming::role_network_name(container_name),
               certs_volume: Some(crate::instance::naming::dind_certs_volume(container_name)),
           }
   ```

4. **Update `crates/jackin-runtime/src/runtime/cleanup.rs`: split the `super::naming` import.**

   Locate the exact block (lines 24–27):
   ```rust
   use super::naming::{
       LABEL_IMAGE_KEY, LABEL_KIND_DIND, LABEL_KIND_ROLE, LABEL_MANAGED, LABEL_ROLE_KEY,
       dind_certs_volume, role_network_name,
   };
   ```

   Replace with:
   ```rust
   use super::naming::{
       LABEL_IMAGE_KEY, LABEL_KIND_DIND, LABEL_KIND_ROLE, LABEL_MANAGED, LABEL_ROLE_KEY,
   };
   use crate::instance::naming::{dind_certs_volume, role_network_name};
   ```

   (The usages at lines 269–270, `dind_certs_volume(&info.role)` and `role_network_name(&info.role)`, are unchanged — only the import path changes.)

5. **Update `crates/jackin-runtime/src/runtime/launch.rs`: split the `super::naming` import.**

   Locate the exact line (line 70):
   ```rust
   use super::naming::{LABEL_KEEP_AWAKE, LABEL_KIND_ROLE, LABEL_MANAGED, dind_certs_volume};
   ```

   Replace with:
   ```rust
   use super::naming::{LABEL_KEEP_AWAKE, LABEL_KIND_ROLE, LABEL_MANAGED};
   use crate::instance::naming::dind_certs_volume;
   ```

   (The usage at line 671, `let certs_volume = dind_certs_volume(container_name);`, is unchanged.)

6. **Update `crates/jackin-runtime/src/runtime/launch/launch_dind.rs`: repoint two relative-path calls.**

   Locate the exact block (lines 270–271):
   ```rust
       let dind = super::super::naming::dind_container_name(&base);
       let network = super::super::naming::role_network_name(&base);
   ```

   Replace with:
   ```rust
       let dind = crate::instance::naming::dind_container_name(&base);
       let network = crate::instance::naming::role_network_name(&base);
   ```

7. **Update `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`: repoint two absolute-path calls.**

   Locate the exact block (lines 1172–1173):
   ```rust
               .unwrap_or_else(|| crate::runtime::naming::dind_container_name(&container_name));
           let certs_volume = crate::runtime::naming::dind_certs_volume(&container_name);
   ```

   Replace with:
   ```rust
               .unwrap_or_else(|| crate::instance::naming::dind_container_name(&container_name));
           let certs_volume = crate::instance::naming::dind_certs_volume(&container_name);
   ```

8. **Remove the three functions from `crates/jackin-runtime/src/runtime/naming.rs`.**

   Locate the exact block (lines 194–208 in the current file):
   ```rust

   /// Docker volume name for the TLS client certificates shared between the
   /// `DinD` sidecar (writer) and the role container (reader).
   pub(crate) fn dind_certs_volume(container_name: &str) -> String {
       format!("{container_name}-dind-certs")
   }

   pub(crate) fn dind_container_name(container_name: &str) -> String {
       format!("{container_name}-dind")
   }

   pub(crate) fn role_network_name(container_name: &str) -> String {
       format!("{container_name}-net")
   }

   #[cfg(test)]
   mod tests;
   ```

   Replace with:
   ```rust

   #[cfg(test)]
   mod tests;
   ```

9. **Remove the `dind_certs_volume_derives_from_container_name` test from `crates/jackin-runtime/src/runtime/naming/tests.rs`.**

   Locate the exact block (lines 93–104):
   ```rust

   #[test]
   fn dind_certs_volume_derives_from_container_name() {
       assert_eq!(
           dind_certs_volume("jk-agent-smith"),
           "jk-agent-smith-dind-certs"
       );
       assert_eq!(
           dind_certs_volume("jk-k7p9m2xq-chainargos-thearchitect"),
           "jk-k7p9m2xq-chainargos-thearchitect-dind-certs"
       );
   }

   #[test]
   fn format_agent_display_appends_instance_id() {
   ```

   Replace with:
   ```rust

   #[test]
   fn format_agent_display_appends_instance_id() {
   ```

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no formatting differences
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-runtime` → all pass (includes the moved `dind_certs_volume_derives_from_container_name` test now under `instance::naming::tests`)
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all OK (no file crosses the 2000L cap; `runtime/naming.rs` shrinks from 209L to ~197L; `instance/naming.rs` grows from 145L to ~158L; neither is listed in `file-size-budget.toml`; neither `naming/tests.rs` is listed in `test-layout-allowlist.toml`)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:** `crate::runtime::naming::dind_certs_volume`, `crate::runtime::naming::dind_container_name`, and `crate::runtime::naming::role_network_name` no longer exist; all three are found only at `crate::instance::naming::{dind_certs_volume,dind_container_name,role_network_name}`; `cargo nextest run -p jackin-runtime` is green.

**Rollback:** `git restore crates/jackin-runtime/src/runtime/naming.rs crates/jackin-runtime/src/runtime/naming/tests.rs crates/jackin-runtime/src/instance/naming.rs crates/jackin-runtime/src/instance/naming/tests.rs crates/jackin-runtime/src/instance/manifest.rs crates/jackin-runtime/src/runtime/cleanup.rs crates/jackin-runtime/src/runtime/launch.rs crates/jackin-runtime/src/runtime/launch/launch_dind.rs crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`

**Open questions:** none

---

### E0 — Launch/attach benchmark + enable lto=thin (prerequisite)

- **Goal:** Add a representative `jackin load` / attach Criterion benchmark to `jackin-runtime` and enable `lto = "thin"` in the workspace release profile, so E1/E2 carve PRs can measure and prove no performance regression.
- **Preconditions:** none
- **Pattern:** config/CI edit (root `Cargo.toml`) + new bench file
- **Touches:** root `Cargo.toml`, `crates/jackin-runtime/Cargo.toml` (modified), `crates/jackin-runtime/benches/launch_attach.rs` (created)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. **Edit `/Users/donbeave/Projects/jackin-project/jackin/Cargo.toml` lines 132–133** — add `lto = "thin"` to `[profile.release]`.

   Find the block:
   ```toml
   [profile.release]
   strip = "symbols"
   ```
   Replace with:
   ```toml
   [profile.release]
   lto = "thin"
   strip = "symbols"
   ```

2. **Edit `/Users/donbeave/Projects/jackin-project/jackin/crates/jackin-runtime/Cargo.toml`** — add `criterion` to `[dev-dependencies]`.

   Find the block:
   ```toml
   [dev-dependencies]
   tokio = { version = "=1.52.3", features = ["test-util"] }
   ```
   Replace with:
   ```toml
   [dev-dependencies]
   criterion = { version = "=0.8.2", features = ["html_reports"] }
   tokio = { version = "=1.52.3", features = ["test-util"] }
   ```

3. **Edit `/Users/donbeave/Projects/jackin-project/jackin/crates/jackin-runtime/Cargo.toml`** — add `[[bench]]` entry immediately after the `[features]` section (before `[lints]`).

   Find the block:
   ```toml
   [lints]
   workspace = true
   ```
   Replace with:
   ```toml
   [[bench]]
   name = "launch_attach"
   harness = false

   [lints]
   workspace = true
   ```

4. **Create directory** `crates/jackin-runtime/benches/` (it does not currently exist).

   Run: `mkdir -p /Users/donbeave/Projects/jackin-project/jackin/crates/jackin-runtime/benches`

5. **Create `/Users/donbeave/Projects/jackin-project/jackin/crates/jackin-runtime/benches/launch_attach.rs`** with the following exact content:

   ```rust
   //! Launch/attach hot-path benchmark — baseline for the E1/E2 carve perf gate.
   //!
   //! Measures the in-process CPU-only operations on the `jackin load` / attach
   //! critical path that will span new crate boundaries when `jackin-isolation`
   //! (E1) and `jackin-instance` (E2) are carved from `jackin-runtime`.
   //!
   //! Run with:
   //! ```sh
   //! cargo bench -p jackin-runtime --bench launch_attach
   //! ```
   //! Record the numbers in the E0 PR description as the baseline. Future carve
   //! PRs (E1, E2) must show no measurable regression against these numbers.

   use criterion::{Criterion, criterion_group, criterion_main};
   use jackin_core::agent::Agent;
   use jackin_core::selector::RoleSelector;
   use jackin_runtime::instance::manifest::{DockerResources, InstanceManifest, NewInstanceManifest};
   use jackin_runtime::instance::naming::{
       class_family_matches_with_slug, compact_component, container_name_with_id,
   };
   use jackin_runtime::isolation::materialize::{clone_path_for, worktree_path_for};
   use std::path::Path;

   // Representative fixtures.
   const WORKSPACE: &str = "myworkspace";
   const ROLE: &str = "myrole";
   const NAMESPACE: &str = "myns";
   const INSTANCE_ID: &str = "ab12cd34";
   const STATE_DIR: &str = "/home/runner/.jackin/data/jk-ab12cd34-myws-myrole";
   const CONTAINER_NAME: &str = "jk-ab12cd34-myws-myrole";
   const DST: &str = "/workspace";

   fn make_selector() -> RoleSelector {
       RoleSelector {
           name: ROLE.to_owned(),
           namespace: Some(NAMESPACE.to_owned()),
       }
   }

   fn new_manifest_input() -> NewInstanceManifest<'static> {
       NewInstanceManifest {
           container_base: CONTAINER_NAME,
           workspace_name: Some(WORKSPACE),
           workspace_label: WORKSPACE,
           workdir: DST,
           host_workdir_fingerprint: "abc123fingerprint0000000000000000",
           role_key: "myns/myrole",
           role_display_name: "My Role",
           agent_runtime: Agent::Claude,
           role_source_git: "https://github.com/example/roles.git",
           role_source_ref: Some("main"),
           image_tag: "jk-myns-myrole:abc123",
           docker: DockerResources {
               role_container: CONTAINER_NAME.to_owned(),
               dind_container: None,
               network: "jk-myws".to_owned(),
               certs_volume: None,
           },
           role_git_sha: None,
           base_image_ref: None,
           base_image_digest: None,
           supported_agents: vec![Agent::Claude],
       }
   }

   // ── Naming: container_name_with_id (E2 hot path) ─────────────────────────────

   fn bench_container_name(c: &mut Criterion) {
       let selector = make_selector();
       c.bench_function("naming/container_name_with_id", |b| {
           b.iter(|| container_name_with_id(Some(WORKSPACE), &selector, INSTANCE_ID));
       });
   }

   // ── Naming: class_family_scan (attach container scan, E2 hot path) ───────────

   /// Simulates the inner loop of `jackin attach`: scan 20 running container
   /// names and collect those whose role slug matches the selector.
   fn bench_class_family_scan(c: &mut Criterion) {
       // 20 containers: 2 match (at indices 7 and 17), 18 do not.
       let containers: Vec<String> = (0u32..20)
           .map(|i| {
               if i % 10 == 7 {
                   format!("jk-{i:08x}-myworkspace-myrole")
               } else {
                   format!("jk-{i:08x}-myworkspace-otherrole{i}")
               }
           })
           .collect();
       let slug = compact_component(ROLE, "role");

       c.bench_function("naming/class_family_scan_20", |b| {
           b.iter(|| {
               containers
                   .iter()
                   .filter(|name| class_family_matches_with_slug(&slug, name))
                   .count()
           });
       });
   }

   // ── Isolation: mount path computation (E1 hot path) ──────────────────────────

   fn bench_mount_paths(c: &mut Criterion) {
       let state_dir = Path::new(STATE_DIR);
       let mut group = c.benchmark_group("isolation");

       group.bench_function("worktree_path_for", |b| {
           b.iter(|| worktree_path_for(state_dir, DST, CONTAINER_NAME));
       });

       group.bench_function("clone_path_for", |b| {
           b.iter(|| clone_path_for(state_dir, DST, CONTAINER_NAME));
       });

       group.finish();
   }

   // ── Instance: manifest construction + serialization (E2 hot path) ────────────

   fn bench_manifest_new(c: &mut Criterion) {
       c.bench_function("instance/manifest_new", |b| {
           b.iter(|| InstanceManifest::new(new_manifest_input()));
       });
   }

   fn bench_manifest_serialize(c: &mut Criterion) {
       let manifest = InstanceManifest::new(new_manifest_input());

       #[expect(
           clippy::unwrap_used,
           reason = "benchmark: serde_json serialization failure should abort the run immediately"
       )]
       c.bench_function("instance/manifest_serialize", |b| {
           b.iter(|| serde_json::to_string(&manifest).unwrap());
       });
   }

   criterion_group!(
       benches,
       bench_container_name,
       bench_class_family_scan,
       bench_mount_paths,
       bench_manifest_new,
       bench_manifest_serialize,
   );
   criterion_main!(benches);
   ```

6. **Record the baseline** — after step 5 lands (CI green), run the following locally on the development machine and paste the stdout output into the PR description under a `## E0 Baseline` heading:
   ```sh
   cargo bench -p jackin-runtime --bench launch_attach
   ```
   The five benchmark IDs to record are:
   - `naming/container_name_with_id`
   - `naming/class_family_scan_20`
   - `isolation/worktree_path_for`
   - `isolation/clone_path_for`
   - `instance/manifest_new`
   - `instance/manifest_serialize`

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exits 0, no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings or errors
- `cargo nextest run -p jackin-runtime` → all pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK; arch informational
- `cargo bench -p jackin-runtime --bench launch_attach` → all 6 benchmark IDs print timing numbers; exits 0
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
- `[profile.release]` in root `Cargo.toml` contains `lto = "thin"`
- `cargo bench -p jackin-runtime --bench launch_attach` runs to completion with all 6 benchmark IDs producing timing output
- Baseline numbers are recorded in the PR description (required before E1 or E2 opens)

**Rollback:**
```sh
git restore Cargo.toml crates/jackin-runtime/Cargo.toml
rm crates/jackin-runtime/benches/launch_attach.rs
rmdir crates/jackin-runtime/benches
```

**Open questions:** none

---

### E1 — Carve jackin-isolation out of jackin-runtime (GATED)

- **Goal:** Extract the mount-isolation subsystem (shared/worktree/clone materialization, cleanup, state persistence, and branch naming) from `crates/jackin-runtime/src/isolation/` into a new `crates/jackin-isolation/` crate, using the reusable crate-carve recipe, so `jackin-runtime` shrinks to the bootstrap pipeline.
- **Preconditions:** E0 MUST be done first (LTO `"thin"` in release profile + launch/attach benchmark baseline recorded). **E0 is NOT yet done** — `Cargo.toml` release profile has no `lto` key, no benchmark crate exists. Do not proceed until E0 lands.
- **Pattern:** crate-carve recipe (Strangler Fig / Parallel Change); hot-path gate required.
- **Touches:**
  - Created: `crates/jackin-isolation/` (new crate — `Cargo.toml`, `src/lib.rs`, `src/branch.rs`, `src/branch/tests.rs`, `src/cleanup.rs`, `src/cleanup/tests.rs`, `src/state.rs`, `src/state/tests.rs`, `src/materialize.rs`, `src/materialize/tests.rs`)
  - Retained in `jackin-runtime` (BLOCKED — see Open Questions): `src/isolation/finalize.rs`, `src/isolation/finalize/tests.rs`, `src/isolation/git_inspect.rs`, `src/isolation/git_inspect/tests.rs`
  - Modified: `crates/jackin-runtime/Cargo.toml` (add `jackin-isolation` dep), `crates/jackin-runtime/src/isolation.rs` (replace moved module bodies with re-exports from `jackin_isolation`), `crates/jackin/Cargo.toml` (add `jackin-isolation` dep if any call sites bypass the shim), `Cargo.toml` workspace `members`
  - Updated: `file-size-budget.toml`, `PROJECT_STRUCTURE.md`, codebase map page
  - Arch gate: `crates/jackin-xtask/src/arch.rs` `FORBIDDEN_EDGES` — add `("jackin-isolation", "jackin-tui")` and `("jackin-isolation", "jackin-launch")` once `finalize`/`git_inspect` are resolved

**Pre-carve investigation required (STOP — read before writing any steps):**

The following two modules in `crates/jackin-runtime/src/isolation/` cannot be moved verbatim without resolving circular/inverted dependency issues. They must either be excluded from this carve, or the blocking dependencies must be removed by a preparatory PR (which becomes a new slice between E0 and E1):

- **`finalize.rs` BLOCKED (circular + inverted):**
  - Line 29: `use crate::runtime::attach::JACKIN_STATUS_CMD;` — `pub const` in `jackin-runtime/src/runtime/attach.rs`. If `jackin-isolation` references this, it depends on `jackin-runtime`, but `jackin-runtime` also depends on `jackin-isolation` → circular.
  - Line 332: `crate::runtime::attach::parse_session_count(…)` — **`pub(crate)` function** in `jackin-runtime/src/runtime/attach.rs`; cannot be accessed from any external crate even with a dep edge.
  - Lines 30, 238, 252: `crate::runtime::progress::PromptContextLine`, `standalone_exit_dialog_with_inspect`, `standalone_error_popup` — these are presentation-layer calls (already a forbidden arch edge: `jackin-runtime → jackin-tui` in the gate, and these functions delegate to `jackin_launch`).
  - Lines 210, 416, 453, 474: `crate::instance::naming::instance_id_from_container_base` — this is actually a re-export from `jackin_core::constants::instance_id_from_container_base`, so it CAN be resolved by importing directly from `jackin_core` (not blocked by circularity).
  - `finalize/tests.rs` also uses `crate::runtime::test_support::FakeRunner` and `FakeDockerClient`, which are gated `#[cfg(any(test, feature = "test-support"))]` in `jackin-runtime`.

- **`git_inspect.rs` BLOCKED (inverted layer dep):**
  - Line ~50 (`worktree_inspect` return type): `jackin_launch::WorktreeInspect` — a presentation-layer struct in `crates/jackin-launch/src/lib.rs:34`.
  - Line ~55: `jackin_launch::FileDiff` — same file, line 21.
  - Moving `git_inspect.rs` to `jackin-isolation` (application layer) would make it depend on `jackin-launch` (presentation layer), which is an inverted dependency violating the target architecture.

TODO(investigate): Decide whether E1 excludes `finalize.rs` and `git_inspect.rs` (leaving them in `jackin-runtime` for a later slice), or whether a preparatory slice (E0.5) first moves `JACKIN_STATUS_CMD` + `parse_session_count` to `jackin-core`, relocates `WorktreeInspect`/`FileDiff` to `jackin-core`, and introduces port traits for the progress/dialog callbacks — making finalize.rs and git_inspect.rs clean to move. The steps below assume **only the four unblocked modules move** (branch, cleanup, state, materialize); finalize and git_inspect stay in `jackin-runtime`. The executor must get operator confirmation of this scope before proceeding.

TODO(investigate): `cleanup/tests.rs` and `finalize/tests.rs` (if finalize moves later) use `crate::runtime::test_support::FakeRunner` and `crate::runtime::test_support::FakeDockerClient`. Once cleanup.rs moves to `jackin-isolation`, its tests need `FakeRunner` accessible via `jackin_runtime` with `features = ["test-support"]` in dev-dependencies. Confirm this is the correct resolution (add `jackin-runtime = { …, features = ["test-support"] }` under `[dev-dependencies]` in `jackin-isolation/Cargo.toml`).

---

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit; STOP if any step fails):

**STOP: Confirm E0 is done before step 1.** Verify `Cargo.toml` `[profile.release]` contains `lto = "thin"` and a benchmark baseline file exists. If not, abort.

**STOP: Confirm operator scope decision** (finalize/git_inspect in or out) before step 1.

1. **Create the new crate directory and `Cargo.toml`.**

   Create `crates/jackin-isolation/Cargo.toml` with this exact content:

   ```toml
   [package]
   name = "jackin-isolation"
   version = "0.6.0-dev"
   edition.workspace = true
   rust-version.workspace = true
   authors = ["Alexey Zhokhov <alexey@zhokhov.com>"]
   description = "Mount isolation subsystem: shared/worktree/clone materialization, cleanup, branch naming, and state persistence."
   license.workspace = true
   readme = "README.md"
   publish = false
   repository.workspace = true

   [lib]
   name = "jackin_isolation"
   path = "src/lib.rs"

   [dependencies]
   jackin-core     = { version = "0.6.0-dev", path = "../jackin-core" }
   jackin-config   = { version = "0.6.0-dev", path = "../jackin-config" }
   jackin-diagnostics = { version = "0.6.0-dev", path = "../jackin-diagnostics" }

   anyhow    = "1.0"
   serde     = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   tracing   = "0.1"
   tempfile  = "3.20"

   [dev-dependencies]
   jackin-runtime = { version = "0.6.0-dev", path = "../jackin-runtime", features = ["test-support"] }
   tokio = { version = "=1.52.3", features = ["test-util"] }
   tempfile = "3.20"

   [features]
   test-support = []

   [lints]
   workspace = true
   ```

   Note: `jackin-runtime` is a **dev-dependency only** (for `FakeRunner` in tests). The production code of `jackin-isolation` must NOT depend on `jackin-runtime`.

2. **Create `crates/jackin-isolation/src/lib.rs`** with this exact content:

   ```rust
   //! jackin-isolation: mount isolation subsystem.
   //!
   //! # Architecture Invariant
   //!
   //! Allowed production dependencies (inward only):
   //! - `jackin-core` (domain types, `CommandRunner`, constants, `worktree_dirty`)
   //! - `jackin-config` (workspace config types: `ResolvedWorkspace`, `DirtyExitPolicy`)
   //! - `jackin-diagnostics` (debug telemetry macros)
   //!
   //! Must NOT depend on: `jackin-runtime`, `jackin-launch`, `jackin-tui`,
   //! `jackin-docker` (docker calls are in materialize via `CommandRunner` trait).
   //!
   //! Three isolation strategies: `Shared` (read-write bind), `Worktree` (git
   //! worktree clone, finalized post-attach), `Clone` (full directory copy,
   //! finalized post-attach). Sub-modules: `materialize` (bind-spec production),
   //! `cleanup` (forced removal), `state` (`IsolationRecord` persistence),
   //! `branch` (worktree branch naming).

   pub mod branch;
   pub mod cleanup;
   pub mod materialize;
   pub mod state;

   pub use jackin_core::MountIsolation;
   pub use jackin_core::ParseMountIsolationError;

   #[cfg(test)]
   mod tests;
   ```

3. **Create `crates/jackin-isolation/src/tests.rs`** — copy verbatim from `crates/jackin-runtime/src/isolation/tests.rs` but replace all `use super::*` with `use super::*;` (unchanged). The only edit needed: the module-level tests reference `MountIsolation` which is re-exported at crate root, so `use super::*` already resolves it. No changes required — copy byte-for-byte.

   ```
   git mv crates/jackin-runtime/src/isolation/tests.rs crates/jackin-isolation/src/tests.rs
   ```

   (Do NOT delete the original yet — the module will be replaced with a re-export shell in step 9.)

4. **`git mv` branch module files:**

   ```
   git mv crates/jackin-runtime/src/isolation/branch.rs     crates/jackin-isolation/src/branch.rs
   git mv crates/jackin-runtime/src/isolation/branch/tests.rs crates/jackin-isolation/src/branch/tests.rs
   ```

   In `crates/jackin-isolation/src/branch.rs`, the body is already free of `crate::` references (it is pure string derivation). No edits required — the content is byte-identical.

5. **`git mv` state module files:**

   ```
   git mv crates/jackin-runtime/src/isolation/state.rs           crates/jackin-isolation/src/state.rs
   git mv crates/jackin-runtime/src/isolation/state/tests.rs     crates/jackin-isolation/src/state/tests.rs
   ```

   In `crates/jackin-isolation/src/state.rs`, make exactly these edits:

   a. Line 18: `pub use crate::isolation::MountIsolation;`
      → `pub use crate::MountIsolation;`

   b. Lines 21: `pub use jackin_core::isolation_record::{CleanupStatus, IsolationRecord};`
      — unchanged (already uses `jackin_core` directly).

   c. In function `list_records_for_workspace` (approx. line 178):
      `if !name_str.starts_with(crate::instance::naming::CONTAINER_PREFIX_DASH) {`
      → `if !name_str.starts_with(jackin_core::constants::CONTAINER_PREFIX_DASH) {`

   In `crates/jackin-isolation/src/state/tests.rs`: the file uses `use super::*;` only, so no edits required.

6. **`git mv` cleanup module files:**

   ```
   git mv crates/jackin-runtime/src/isolation/cleanup.rs         crates/jackin-isolation/src/cleanup.rs
   git mv crates/jackin-runtime/src/isolation/cleanup/tests.rs   crates/jackin-isolation/src/cleanup/tests.rs
   ```

   In `crates/jackin-isolation/src/cleanup.rs`, make exactly these edits:

   a. Line 13: `use crate::isolation::state::{IsolationRecord, remove_record};`
      → `use crate::state::{IsolationRecord, remove_record};`

   b. Line 37 (inside `force_cleanup_isolated`):
      `if matches!(record.isolation, crate::isolation::MountIsolation::Clone) {`
      → `if matches!(record.isolation, crate::MountIsolation::Clone) {`

   c. Line 245 (inside `purge_isolated_for_container`):
      `let records = crate::isolation::state::read_records(container_state_dir)?;`
      → `let records = crate::state::read_records(container_state_dir)?;`

   In `crates/jackin-isolation/src/cleanup/tests.rs`, make exactly these edits:

   a. `use crate::isolation::MountIsolation;`
      → `use crate::MountIsolation;`

   b. `use crate::isolation::state::{CleanupStatus, read_records, write_records};`
      → `use crate::state::{CleanupStatus, read_records, write_records};`

   c. `use crate::runtime::test_support::FakeRunner;`
      → `use jackin_runtime::runtime::test_support::FakeRunner;`

7. **`git mv` materialize module files:**

   ```
   git mv crates/jackin-runtime/src/isolation/materialize.rs        crates/jackin-isolation/src/materialize.rs
   git mv crates/jackin-runtime/src/isolation/materialize/tests.rs  crates/jackin-isolation/src/materialize/tests.rs
   ```

   In `crates/jackin-isolation/src/materialize.rs`, make exactly these edits:

   a. Line 13: `use crate::isolation::MountIsolation;`
      → `use crate::MountIsolation;`

   b. Line 14: `use crate::isolation::branch::branch_name;`
      → `use crate::branch::branch_name;`

   c. Line 15: `use crate::isolation::state::{CleanupStatus, IsolationRecord, read_record, upsert_record};`
      → `use crate::state::{CleanupStatus, IsolationRecord, read_record, upsert_record};`

   Line 427: `use jackin_config::MountConfig;` — already uses `jackin_config` directly, no edit.

   In `crates/jackin-isolation/src/materialize/tests.rs`, make exactly these edits:

   a. Any `use crate::isolation::…` references → `use crate::…` (remove the `isolation::` segment).
      Specifically check for `use crate::isolation::materialize::…` → `use crate::materialize::…`
      and `use crate::isolation::MountIsolation` → `use crate::MountIsolation`.

   b. `use crate::runtime::test_support::FakeRunner;` (if present) → `use jackin_runtime::runtime::test_support::FakeRunner;`

   TODO(investigate): Verify the full import list in `materialize/tests.rs` — the preview showed only `use super::*` and `use std::path::PathBuf`, but the full 1337-line file may contain more `crate::isolation::*` references. Run `grep -n "crate::isolation\|crate::runtime\|crate::instance" crates/jackin-runtime/src/isolation/materialize/tests.rs` before moving.

8. **Add `crates/jackin-isolation` to workspace `members` in root `Cargo.toml`.**

   In `/Users/donbeave/Projects/jackin-project/jackin/Cargo.toml`, inside the `[workspace] members = [...]` array, add after `"crates/jackin-image",`:

   ```toml
       "crates/jackin-isolation",
   ```

9. **Replace `crates/jackin-runtime/src/isolation.rs` with a thin re-export shell.**

   The moved modules are gone; `finalize` and `git_inspect` stay. Replace the entire contents of `crates/jackin-runtime/src/isolation.rs` with:

   ```rust
   //! Mount isolation re-exports from `jackin-isolation`.
   //!
   //! The shared/worktree/clone materialization, cleanup, state persistence,
   //! and branch naming now live in the `jackin-isolation` crate. This module
   //! re-exports their public APIs so existing `crate::isolation::*` call sites
   //! inside `jackin-runtime` continue to compile unchanged.
   //!
   //! `finalize` and `git_inspect` remain here pending resolution of the
   //! inverted/circular dependencies documented in the E1 implementation plan.

   pub mod finalize;
   pub mod git_inspect;

   // Re-export the carved-out modules.
   pub use jackin_isolation::branch;
   pub use jackin_isolation::cleanup;
   pub use jackin_isolation::materialize;
   pub use jackin_isolation::state;

   pub use jackin_core::MountIsolation;
   pub use jackin_core::ParseMountIsolationError;

   #[cfg(test)]
   mod tests;
   ```

   The `tests.rs` file is already moved to `jackin-isolation`; the `#[cfg(test)] mod tests;` line in the old isolation.rs must be removed (or the tests.rs recreated). Since the tests in `isolation/tests.rs` test `MountIsolation` which is now in `jackin-isolation`, delete the `#[cfg(test)] mod tests;` line from the new shell above, and confirm `crates/jackin-runtime/src/isolation/tests.rs` no longer exists (it was git-moved in step 3).

   Corrected shell (no tests line):

   ```rust
   //! Mount isolation re-exports from `jackin-isolation`.
   //!
   //! The shared/worktree/clone materialization, cleanup, state persistence,
   //! and branch naming now live in the `jackin-isolation` crate. This module
   //! re-exports their public APIs so existing `crate::isolation::*` call sites
   //! inside `jackin-runtime` continue to compile unchanged.
   //!
   //! `finalize` and `git_inspect` remain here pending resolution of the
   //! inverted/circular dependencies documented in the E1 implementation plan.

   pub mod finalize;
   pub mod git_inspect;

   pub use jackin_isolation::branch;
   pub use jackin_isolation::cleanup;
   pub use jackin_isolation::materialize;
   pub use jackin_isolation::state;

   pub use jackin_core::MountIsolation;
   pub use jackin_core::ParseMountIsolationError;
   ```

10. **Update `finalize.rs` and `git_inspect.rs` sibling imports** (they stay in `jackin-runtime` but their sibling modules moved).

    In `crates/jackin-runtime/src/isolation/finalize.rs`:

    a. Line 27: `use crate::isolation::cleanup::force_cleanup_isolated;`
       → `use jackin_isolation::cleanup::force_cleanup_isolated;`

    b. Line 28: `use crate::isolation::state::{CleanupStatus, IsolationRecord, read_records, upsert_record};`
       → `use jackin_isolation::state::{CleanupStatus, IsolationRecord, read_records, upsert_record};`

    c. Line 210: `vec![crate::isolation::git_inspect::worktree_inspect(`
       — unchanged (git_inspect stays in the same crate, still accessible as `crate::isolation::git_inspect::worktree_inspect`).

    d. Lines 416, 453, 474: `crate::instance::naming::instance_id_from_container_base`
       — unchanged (still in same crate).

    In `crates/jackin-runtime/src/isolation/finalize/tests.rs`:

    a. `use crate::isolation::MountIsolation;` → `use jackin_isolation::MountIsolation;`

    b. `use crate::isolation::state::{CleanupStatus, IsolationRecord};` → `use jackin_isolation::state::{CleanupStatus, IsolationRecord};`

    c. `use crate::isolation::state::write_records;` → `use jackin_isolation::state::write_records;`

    In `crates/jackin-runtime/src/isolation/git_inspect.rs`:

    a. Line (import of `ChangedFile`, `parse_porcelain`): `use jackin_core::worktree_dirty::{ChangedFile, parse_porcelain};` — unchanged.

    No other imports reference the moved modules.

11. **Add `jackin-isolation` as a dependency of `jackin-runtime` in `crates/jackin-runtime/Cargo.toml`.**

    Under `[dependencies]`, add:

    ```toml
    jackin-isolation = { version = "0.6.0-dev", path = "../jackin-isolation" }
    ```

12. **Update `crates/jackin/src/isolation.rs`** (the binary shim).

    The shim currently re-exports from `jackin_runtime::isolation::*`. Since the moved modules are now re-exported by `jackin_runtime::isolation` anyway (step 9), the shim compiles unchanged. However, to document the new ownership, update the module-level comment:

    ```rust
    //! Isolation shim — materialization, cleanup, state, and branch naming now
    //! live in `jackin-isolation`; finalize and git_inspect remain in
    //! `jackin-runtime::isolation`. All are accessible through this shim via
    //! `jackin-runtime`'s re-export shell at `jackin_runtime::isolation`.
    ```

    The `pub use jackin_runtime::isolation::branch::*;` etc. lines compile because `jackin_runtime::isolation::branch` is now `pub use jackin_isolation::branch;` (step 9). No other edits required.

13. **Run the perf gate** (mandatory for hot-path carves per D1 binding condition):

    ```sh
    # Run the E0 benchmark against the E0 baseline. Record numbers in the PR body.
    # The exact command depends on the benchmark harness added in E0.
    # TODO(investigate): confirm the benchmark invocation installed by E0.
    ```

14. **Refresh the file-size ratchet.**

    The files that moved out of `jackin-runtime` were all under the 2000L production cap and do not appear in `file-size-budget.toml`. No ratchet entry to remove. Verify after the move:

    ```sh
    cargo run -p jackin-xtask --locked -- lint files --print-budget
    ```

    If any entry for a now-deleted file appears, remove it from `file-size-budget.toml`.

15. **Update `PROJECT_STRUCTURE.md` and the codebase-map page** to list `jackin-isolation` under L1 application/orchestration (per D3). Add a one-line entry: "mount isolation: shared/worktree/clone materialization, cleanup, state persistence, branch naming."

16. **Update `docs/content/docs/roadmap/codebase-health-enforcement.mdx`**: check the E1 slice box under Phase E.

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no output (all files already formatted; no logic edits were made)
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings (new crate inherits `[lints] workspace = true`; all imports explicit, no wildcards)
- `cargo nextest run -p jackin-isolation` → all tests pass
- `cargo nextest run -p jackin-runtime` → all tests pass (finalize/git_inspect tests still in place; cleanup/state/materialize unit tests now live in jackin-isolation)
- `cargo nextest run --workspace --all-features` → all tests pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all green (arch gate: jackin-isolation must not appear as a dependency of `jackin-tui` or `jackin-launch`; informational only until `--strict` is enabled)
- behavioral specs `runtime-launch` and `op-picker` pass **unmodified**
- *(hot-path gate)* launch/attach benchmark shows no measurable regression vs the E0 baseline numbers recorded in the PR; attach the numbers to the PR description

**Done when:** `cargo build --workspace` and `cargo nextest run --workspace --all-features` are green; `crates/jackin-isolation/` exists as a workspace member containing `branch`, `cleanup`, `state`, `materialize` (and their `tests.rs` siblings); `crates/jackin-runtime/src/isolation.rs` is a thin re-export shell retaining only `finalize` and `git_inspect` as inline sub-modules; no `crate::isolation::{branch,cleanup,state,materialize}` references remain in `jackin-runtime` source (only `crate::isolation::finalize` and `crate::isolation::git_inspect`).

**Rollback:** `git restore crates/jackin-runtime/src/isolation.rs crates/jackin-runtime/Cargo.toml crates/jackin/src/isolation.rs Cargo.toml` and `git rm -rf crates/jackin-isolation/` then `git checkout -- crates/jackin-runtime/src/isolation/`.

**Open questions:**

1. **E0 not done:** The release profile in `/Users/donbeave/Projects/jackin-project/jackin/Cargo.toml` `[profile.release]` has only `strip = "symbols"` — no `lto = "thin"`. No benchmark crate is present in the workspace. E1 must not proceed until E0 lands. Executor must not guess what E0 adds.

2. **finalize.rs circular dependency — scope decision required:** `finalize.rs` uses `crate::runtime::attach::JACKIN_STATUS_CMD` (pub const) and `crate::runtime::attach::parse_session_count` (pub(crate)) from `jackin-runtime`. If finalize.rs moves to `jackin-isolation`, the crate would need to depend on `jackin-runtime` for these symbols, but `jackin-runtime` depends on `jackin-isolation` → circular. The pub(crate) `parse_session_count` function cannot be called from outside `jackin-runtime` at all. **Decision needed:** does a preparatory slice first move `JACKIN_STATUS_CMD` and `parse_session_count` to `jackin-core`? Or does `finalize.rs` stay in `jackin-runtime` for E1 and move in a later slice?

3. **git_inspect.rs inverted dependency — scope decision required:** `git_inspect.rs::worktree_inspect` returns `jackin_launch::WorktreeInspect` (a presentation-layer struct). Moving it to `jackin-isolation` (L1 application) creates a L1 → L3 inverted dep. **Decision needed:** does `WorktreeInspect`/`FileDiff` first move to `jackin-core` in a preparatory slice? Or does `git_inspect.rs` stay in `jackin-runtime` for E1?

4. **FakeRunner dev-dependency:** `cleanup/tests.rs` (and `finalize/tests.rs` if finalize moves later) use `crate::runtime::test_support::FakeRunner` and `FakeDockerClient`, which are in `jackin-runtime` gated by `#[cfg(any(test, feature = "test-support"))]`. In `jackin-isolation`, the dev-dependency on `jackin-runtime` with `features = ["test-support"]` exposes `FakeRunner` as `jackin_runtime::runtime::test_support::FakeRunner`. Confirm this resolves correctly by running `cargo test -p jackin-isolation` after step 7.

5. **materialize/tests.rs full import audit:** Only the first 30 lines of `materialize/tests.rs` (1337 lines total) were inspected. Run `grep -n "crate::isolation\|crate::runtime\|crate::instance" crates/jackin-runtime/src/isolation/materialize/tests.rs` before executing step 7 to find all import rewrites needed.

6. **isolation/tests.rs after move:** After `git mv`-ing `isolation/tests.rs` to `jackin-isolation/src/tests.rs` (step 3), the `#[cfg(test)] mod tests;` line in `jackin-runtime/src/isolation.rs` must be removed. Confirm that nothing in `jackin-runtime` references `crate::isolation::tests::*` directly (unlikely but check with `grep -rn "isolation::tests" crates/jackin-runtime/src/`).

7. **Benchmark invocation:** The exact `cargo bench` or similar command added by E0 is unknown. The executor must not guess; read the E0 PR for the command.

---

### E2 — Carve `jackin-instance` out of `jackin-runtime` (GATED)

- **Goal:** Extract the `instance` subsystem (instance identity, manifest/index persistence, auth-state provisioning, and container naming) from `crates/jackin-runtime/src/instance*` into a new, independently compiled `crates/jackin-instance` crate.
- **Preconditions:** E0 (benchmark baseline + `lto = "thin"` in release profile) must be DONE; E1 (`jackin-isolation` carve) is RECOMMENDED before E2 so the isolation callers' paths are already settled — see Open questions.
- **Pattern:** crate-carve recipe + intra-crate pre-move (3 naming helpers relocated before the boundary forms) + Parallel Change re-export (`pub use jackin_instance as instance;`) so all downstream call sites require zero import edits.
- **Touches:** new crate `crates/jackin-instance/` (created); `crates/jackin-runtime/src/instance.rs`, `crates/jackin-runtime/src/instance/{auth,manifest,naming,tests}.rs` and their sub-test files (moved); `crates/jackin-runtime/src/runtime/naming.rs`, `crates/jackin-runtime/src/lib.rs`, `crates/jackin-runtime/Cargo.toml`; root `Cargo.toml`; `test-layout-allowlist.toml`; `PROJECT_STRUCTURE.md`; `docs/content/docs/reference/getting-oriented/codebase-map.mdx`; `docs/content/docs/roadmap/codebase-health-enforcement.mdx`.

---

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

**Phase A — Intra-runtime pre-move (resolve circular-dep before crate boundary forms)**

The function `DockerResources::from_container_name` in `crates/jackin-runtime/src/instance/manifest.rs` (lines 75–77) calls `crate::runtime::naming::{dind_container_name, role_network_name, dind_certs_volume}`. After the carve these would be a cross-crate cycle (`jackin-instance` → `jackin-runtime` → `jackin-instance`). Fix: relocate the three functions into `instance/naming.rs` before moving the files.

1. In `crates/jackin-runtime/src/instance/naming.rs`, insert the following three functions immediately before the `#[cfg(test)] mod tests;` line at the bottom of the file (they gain `pub` visibility because they must be reachable from `jackin-runtime` after the crate boundary forms):

   ```rust
   /// Docker volume name for the TLS client certificates shared between the
   /// `DinD` sidecar (writer) and the role container (reader).
   pub fn dind_certs_volume(container_name: &str) -> String {
       format!("{container_name}-dind-certs")
   }

   pub fn dind_container_name(container_name: &str) -> String {
       format!("{container_name}-dind")
   }

   pub fn role_network_name(container_name: &str) -> String {
       format!("{container_name}-net")
   }
   ```

2. In `crates/jackin-runtime/src/runtime/naming.rs`, replace the three function definitions (lines 196–207) with re-exports so callers in `runtime/` are unchanged:

   Old (remove these three function bodies, keep the doc comment above `dind_certs_volume`):
   ```rust
   pub(crate) fn dind_certs_volume(container_name: &str) -> String {
       format!("{container_name}-dind-certs")
   }

   pub(crate) fn dind_container_name(container_name: &str) -> String {
       format!("{container_name}-dind")
   }

   pub(crate) fn role_network_name(container_name: &str) -> String {
       format!("{container_name}-net")
   }
   ```

   New (add at the top of `runtime/naming.rs`, after existing `use` lines and before the constants):
   ```rust
   pub(crate) use crate::instance::naming::{
       dind_certs_volume, dind_container_name, role_network_name,
   };
   ```

3. In `crates/jackin-runtime/src/instance/manifest.rs`, update the three cross-module calls inside `DockerResources::from_container_name` (lines 75–77):

   Old:
   ```rust
           dind_container: Some(crate::runtime::naming::dind_container_name(container_name)),
           network: crate::runtime::naming::role_network_name(container_name),
           certs_volume: Some(crate::runtime::naming::dind_certs_volume(container_name)),
   ```
   New:
   ```rust
           dind_container: Some(crate::instance::naming::dind_container_name(container_name)),
           network: crate::instance::naming::role_network_name(container_name),
           certs_volume: Some(crate::instance::naming::dind_certs_volume(container_name)),
   ```

**Phase B — Create new crate scaffold**

4. Create `crates/jackin-instance/Cargo.toml` with the following exact content:

   ```toml
   [package]
   name = "jackin-instance"
   version = "0.6.0-dev"
   edition.workspace = true
   rust-version.workspace = true
   authors = ["Alexey Zhokhov <alexey@zhokhov.com>"]
   description = "Instance identity, manifest/index persistence, auth-state provisioning, and container naming for jackin❯ role containers."
   license.workspace = true
   readme = "README.md"
   publish = false
   repository.workspace = true

   [lib]
   name = "jackin_instance"
   path = "src/lib.rs"

   [dependencies]
   jackin-core       = { version = "0.6.0-dev", path = "../jackin-core" }
   jackin-config     = { version = "0.6.0-dev", path = "../jackin-config" }
   jackin-manifest   = { version = "0.6.0-dev", path = "../jackin-manifest" }
   jackin-diagnostics = { version = "0.6.0-dev", path = "../jackin-diagnostics" }

   tracing      = "0.1"
   anyhow       = "1.0"
   serde        = { version = "1.0", features = ["derive"] }
   serde_json   = "1.0"
   rand         = "0.10"
   chrono       = { version = "0.4", default-features = false, features = ["clock"] }
   sha2         = "0.11"
   hex          = "0.4"
   fs2          = "0.4"
   serde_yaml_ng = "0.10.0"
   directories  = "6.0"
   tempfile     = "3.20"

   [features]
   test-support = []

   [lints]
   workspace = true
   ```

**Phase C — Move files (git mv)**

5. `git mv crates/jackin-runtime/src/instance.rs crates/jackin-instance/src/lib.rs`
6. `git mv crates/jackin-runtime/src/instance/auth.rs crates/jackin-instance/src/auth.rs`
7. `git mv crates/jackin-runtime/src/instance/auth/tests.rs crates/jackin-instance/src/auth/tests.rs`
8. `git mv crates/jackin-runtime/src/instance/manifest.rs crates/jackin-instance/src/manifest.rs`
9. `git mv crates/jackin-runtime/src/instance/manifest/tests.rs crates/jackin-instance/src/manifest/tests.rs`
10. `git mv crates/jackin-runtime/src/instance/naming.rs crates/jackin-instance/src/naming.rs`
11. `git mv crates/jackin-runtime/src/instance/naming/tests.rs crates/jackin-instance/src/naming/tests.rs`
12. `git mv crates/jackin-runtime/src/instance/tests.rs crates/jackin-instance/src/tests.rs`

   (The now-empty directories `crates/jackin-runtime/src/instance/`, `crates/jackin-runtime/src/instance/auth/`, `crates/jackin-runtime/src/instance/manifest/`, `crates/jackin-runtime/src/instance/naming/` are removed automatically by git or can be removed with `rmdir`.)

**Phase D — Fix intra-module paths in moved files**

After the git mv, `crate` inside each moved file refers to `jackin_instance`, so `crate::instance::naming::` must become `crate::naming::`.

13. In `crates/jackin-instance/src/manifest.rs`, update four path expressions:

    a. Line 75 (after Phase A step 3 changed it to `crate::instance::naming::dind_container_name`):
    Old: `crate::instance::naming::dind_container_name(container_name)`
    New: `crate::naming::dind_container_name(container_name)`

    b. Line 76:
    Old: `crate::instance::naming::role_network_name(container_name)`
    New: `crate::naming::role_network_name(container_name)`

    c. Line 77:
    Old: `crate::instance::naming::dind_certs_volume(container_name)`
    New: `crate::naming::dind_certs_volume(container_name)`

    d. Line 177:
    Old: `crate::instance::naming::instance_id_from_container_base(`
    New: `crate::naming::instance_id_from_container_base(`

14. In `crates/jackin-instance/src/auth/tests.rs`, update three `use` statements:

    a. Line 2:
    Old: `use crate::instance::{AuthProvisionOutcome, PrepareResolvers, RoleState};`
    New: `use crate::{AuthProvisionOutcome, PrepareResolvers, RoleState};`

    b. Line 37:
    Old: `use crate::instance::validate_sync_source_dir;`
    New: `use crate::validate_sync_source_dir;`

    c. Line 1902 (the block starting with `use crate::instance::{`):
    Old: `use crate::instance::{`
    New: `use crate::{`

15. In `crates/jackin-instance/src/lib.rs` (the moved `instance.rs`), replace the opening `//!` doc comment block with the Architecture-Invariant header. Replace:

    ```rust
    //! Role instance lifecycle: instance index, role-state directory, auth
    //! provisioning, and container naming.
    //!
    //! An "instance" is the on-disk and in-Docker state for a single running or
    //! previously-run role container. `InstanceIndex` tracks container status;
    //! `RoleState` holds the credential and state files bind-mounted into the
    //! container at launch.
    //!
    //! Not responsible for: Docker network/image/DinD resource management
    //! (`runtime/`), or mount materialization (`isolation/materialize.rs`).
    ```

    With:

    ```rust
    //! `jackin-instance` — instance identity, manifest/index persistence, auth-state
    //! provisioning, and container naming for role containers.
    //!
    //! **Architecture invariant:** allowed deps are `jackin-core`, `jackin-config`,
    //! `jackin-manifest`, and `jackin-diagnostics`. Must NOT depend on
    //! `jackin-runtime`, `jackin-isolation`, `jackin-docker`, `jackin-tui`,
    //! or any presentation crate.
    //!
    //! An "instance" is the on-disk and in-Docker state for a single running or
    //! previously-run role container. `InstanceIndex` tracks container status;
    //! `RoleState` holds the credential and state files bind-mounted into the
    //! container at launch.
    //!
    //! Not responsible for: Docker network/image/DinD resource management
    //! (lives in `jackin-runtime`), or mount materialization (`jackin-isolation`).
    ```

**Phase E — Update `jackin-runtime`**

16. In `crates/jackin-runtime/src/lib.rs`, replace the line `pub mod instance;` (line 11) with the re-export alias:

    Old: `pub mod instance;`
    New: `pub use jackin_instance as instance;`

    This Parallel Change re-export means every `crate::instance::*` path in `jackin-runtime`'s other source files continues to resolve without any further edits, and every `jackin_runtime::instance::*` path in downstream crates (`jackin`, `jackin-console/tui/state/tests.rs`, etc.) also continues to compile unchanged.

17. In `crates/jackin-runtime/Cargo.toml`, make the following four changes:

    a. Add `jackin-instance` to `[dependencies]` (after `jackin-core`):
    ```toml
    jackin-instance = { version = "0.6.0-dev", path = "../jackin-instance" }
    ```

    b. Remove `rand = "0.10"` from `[dependencies]` (only used in `instance/naming.rs::random_instance_id`, which has moved).

    c. Remove `chrono = { version = "0.4", ... }` from `[dependencies]` (only used in `instance/manifest.rs::now_rfc3339`, which has moved).

    d. Remove `serde_yaml_ng = "0.10.0"` from `[dependencies]` (only used in `instance/auth.rs::parse_gh_hosts_yml`, which has moved).

    e. Move `tempfile = "3.20"` from `[dependencies]` to `[dev-dependencies]` (after the carve, `tempfile` is only used by tests remaining in `jackin-runtime`, e.g., `isolation/materialize/tests.rs` and `runtime/cleanup/tests.rs`).

    f. Update the `[features]` section so the `test-support` feature propagates into `jackin-instance`:

    Old:
    ```toml
    [features]
    test-support = []
    ```

    New:
    ```toml
    [features]
    test-support = ["jackin-instance/test-support"]
    ```

    This ensures that when the `jackin` binary enables `jackin-runtime/test-support` in its dev-dependencies, `InstanceIndex::read` (gated on `#[cfg(any(test, feature = "test-support"))]` in `jackin-instance/src/manifest.rs` line 567) becomes available for `jackin/src/console/tui/state/tests.rs`.

**Phase F — Workspace, CI, and docs**

18. In root `Cargo.toml`, add `"crates/jackin-instance"` to the `[workspace] members` array. Insert it in alphabetical order between `"crates/jackin-env"` and `"crates/jackin-image"`:

    ```toml
    "crates/jackin-instance",
    ```

19. In `test-layout-allowlist.toml`, update the stale path for the grandfathered `auth/tests.rs` violation:

    Old: `"crates/jackin-runtime/src/instance/auth/tests.rs",`
    New: `"crates/jackin-instance/src/auth/tests.rs",`

20. In `PROJECT_STRUCTURE.md`, add a new row for `jackin-instance` to the crate-summary table and update the `jackin-runtime` description to no longer mention `instance`. The existing line for `jackin-runtime` (search for `src/instance/**`) mentions `instance` under jackin-runtime — update or remove those cross-references so they point to `jackin-instance`.

21. In `docs/content/docs/reference/getting-oriented/codebase-map.mdx`:

    a. Update the crate-summary table row (line ~57):
    Old: `jackin-runtime     runtime + instance + isolation  → core, config, env, manifest, docker, image, diagnostics`
    New two rows (split the combined row):
    ```
    jackin-runtime     launch/bootstrap pipeline, image build, DinD  → core, config, env, manifest, docker, image, diagnostics, jackin-instance
    jackin-instance    instance identity, manifest, index, auth prep  → core, config, manifest, diagnostics
    ```

    b. Update three `<RepoFile>` tags that reference old `instance/` paths under `jackin-runtime`:
    - `crates/jackin-runtime/src/instance/auth.rs` → `crates/jackin-instance/src/auth.rs`
    - `crates/jackin-runtime/src/instance/naming.rs` → `crates/jackin-instance/src/naming.rs`
    - `crates/jackin-runtime/src/instance/manifest.rs` → `crates/jackin-instance/src/manifest.rs`

    c. Update the cross-reference table row (line ~295) for `auth.rs`:
    Old: `<RepoFile path="crates/jackin-runtime/src/instance/auth.rs">...`
    New: `<RepoFile path="crates/jackin-instance/src/auth.rs">...`

22. In `docs/content/docs/roadmap/codebase-health-enforcement.mdx`, check the E2 checkbox:

    Old: `- [ ] **E2 — carve \`jackin-instance\`**`
    New: `- [x] **E2 — carve \`jackin-instance\`**`

**Phase G — Perf gate (binding condition per D1/D3)**

23. Run the E0 launch/attach benchmark suite against the recorded E0 baseline. The output numbers must show no measurable regression. Attach the benchmark results to the PR description. If a regression is observed, STOP: do not merge; investigate whether LTO coverage needs to be expanded or whether the crate boundary has introduced unexpected non-inlined call overhead.

---

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-instance` → all tests in the new crate pass
- `cargo nextest run -p jackin-runtime` → all remaining runtime tests pass (isolation, attach, launch, cleanup, etc.)
- `cargo nextest run -p jackin` → all binary crate tests pass (includes `console/tui/state/tests.rs` which uses `jackin_runtime::instance::{InstanceIndex, ...}` via the re-export)
- `cargo nextest run --workspace --all-features` → full workspace green
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + arch gates all pass
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → run and verify no new over-cap files in `jackin-instance/`; update `file-size-budget.toml` if any instance file dropped under cap (none expected — all files are under the 2000L production cap and 10000L test cap)
- `cargo run -p jackin-xtask --locked -- lint tests --print-allowlist` → verify the updated `test-layout-allowlist.toml` path is accepted
- `cargo deny check licenses bans sources` → supply-chain gate passes for the new crate
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED (zero edits to those spec files)
- E0 benchmark shows no measurable regression vs baseline (Phase G step 23)

---

**Done when:** `cargo nextest run --workspace --all-features` is green, `cargo xtask lint` is green, `cargo deny check licenses bans sources` passes, the E0 benchmark shows no regression, and `crates/jackin-runtime/src/instance*` no longer exists in the repository.

**Rollback:** `git restore --staged . && git restore .` to undo all working-tree edits, then `git clean -fd crates/jackin-instance/` to remove the new crate directory. Because the steps involve `git mv`, a partial rollback after the mv may require `git mv` in reverse or `git checkout HEAD -- crates/jackin-runtime/src/instance*`.

**Open questions:**

1. **E1 ordering:** The roadmap allows E1 and E2 in any order after E0, but `isolation/finalize.rs` and `isolation/state.rs` (currently in `jackin-runtime`) use `crate::instance::naming::instance_id_from_container_base` and `crate::instance::naming::CONTAINER_PREFIX_DASH`. If E1 has already been done (so those files now live in `jackin-isolation`), they reference `jackin_runtime::instance::naming::…` which after E2 resolves via the re-export to `jackin_instance::naming::…` — this chain works. BUT: the executor must verify that `jackin-isolation` (if it exists) compiles cleanly after E2 is applied. If `jackin-isolation` imports directly from `jackin_runtime::instance::…` via an explicit `jackin-runtime` dep, it will continue to work through the re-export. If it imports via `jackin_instance` directly (which it would not at E1 time), no change needed. Executor must run `cargo nextest run -p jackin-isolation` (if the crate exists) as an additional verify step.

2. **`rand` version pinning:** `jackin-runtime/Cargo.toml` uses `rand = "0.10"`. Verify that this exact version is available in `Cargo.lock` and that adding it to `jackin-instance/Cargo.toml` resolves to the same locked version (no duplicate). Check `Cargo.lock` for `rand` after the first `cargo build`.

3. **`cargo shear` post-carve scan for jackin-runtime:** After removing `rand`, `chrono`, and `serde_yaml_ng` from `jackin-runtime/Cargo.toml` and moving `tempfile` to `[dev-dependencies]`, run `cargo shear` to confirm no other dep in `jackin-runtime` became orphaned as a side-effect of the carve.

4. **`dind_certs_volume` test in `runtime/naming/tests.rs`:** The test `dind_certs_volume_derives_from_container_name` at lines 93–99 of `crates/jackin-runtime/src/runtime/naming/tests.rs` tests a function that now lives in `jackin-instance::naming` (re-exported into `runtime/naming.rs` via `pub(crate) use crate::instance::naming::dind_certs_volume`). The test will still pass via the re-export. No action required, but the executor must confirm the test is not dropped silently by `cargo shear` flagging the re-export as unused.

---

### F1 — app_config_* fan-out → app_config/ coordinator + siblings (P8)

- **Goal:** Rename the four `app_config_*.rs` top-level siblings into coordinator + sibling layout: `app_config.rs` stays as the coordinator, and the four implementation files move to `app_config/{mounts,persist,roles,workspaces}.rs` with matching `tests.rs` files under `app_config/{mounts,persist,roles,workspaces}/tests.rs`.
- **Preconditions:** none
- **Pattern:** file-split (coordinator already exists; move flat siblings into child directory)
- **Touches:** `crates/jackin-config/src/app_config.rs`, `crates/jackin-config/src/lib.rs`, `crates/jackin-config/src/editor.rs`; creates `app_config/mounts.rs`, `app_config/mounts/tests.rs`, `app_config/persist.rs`, `app_config/persist/tests.rs`, `app_config/roles.rs`, `app_config/roles/tests.rs`, `app_config/workspaces.rs`, `app_config/workspaces/tests.rs`; deletes `app_config_mounts.rs`, `app_config_mounts/tests.rs`, `app_config_persist.rs`, `app_config_persist/tests.rs`, `app_config_roles.rs`, `app_config_roles/tests.rs`, `app_config_workspaces.rs`, `app_config_workspaces/tests.rs`

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Create new sibling directories:
   ```
   mkdir -p crates/jackin-config/src/app_config/mounts
   mkdir -p crates/jackin-config/src/app_config/persist
   mkdir -p crates/jackin-config/src/app_config/roles
   mkdir -p crates/jackin-config/src/app_config/workspaces
   ```

2. Copy `crates/jackin-config/src/app_config_mounts.rs` to `crates/jackin-config/src/app_config/mounts.rs`, then in the new file make exactly one edit — change line 9:
   - Old: `use crate::app_config::AppConfig;`
   - New: `use super::AppConfig;`
   All other content is identical to the source file.

3. Copy `crates/jackin-config/src/app_config_mounts/tests.rs` verbatim to `crates/jackin-config/src/app_config/mounts/tests.rs`. No edits.

4. Copy `crates/jackin-config/src/app_config_persist.rs` to `crates/jackin-config/src/app_config/persist.rs`, then in the new file make exactly two edits:

   Edit A — change line 11:
   - Old: `use crate::app_config::AppConfig;`
   - New: `use super::AppConfig;`

   Edit B — change line 292 (within the `AppConfig::load_or_init` impl):
   - Old: `            for &(name, git) in crate::app_config_roles::BUILTIN_ROLES {`
   - New: `            for &(name, git) in super::roles::BUILTIN_ROLES {`

5. Copy `crates/jackin-config/src/app_config_persist/tests.rs` verbatim to `crates/jackin-config/src/app_config/persist/tests.rs`. No edits.

6. Copy `crates/jackin-config/src/app_config_roles.rs` to `crates/jackin-config/src/app_config/roles.rs`, then in the new file make exactly one edit — change line 9:
   - Old: `use crate::app_config::AppConfig;`
   - New: `use super::AppConfig;`
   All other content is identical to the source file.

7. Copy `crates/jackin-config/src/app_config_roles/tests.rs` verbatim to `crates/jackin-config/src/app_config/roles/tests.rs`. No edits.

8. Copy `crates/jackin-config/src/app_config_workspaces.rs` to `crates/jackin-config/src/app_config/workspaces.rs`, then in the new file make exactly one edit — change line 3:
   - Old: `use crate::app_config::AppConfig;`
   - New: `use super::AppConfig;`
   All other content is identical to the source file.

9. Copy `crates/jackin-config/src/app_config_workspaces/tests.rs` verbatim to `crates/jackin-config/src/app_config/workspaces/tests.rs`. No edits.

10. Edit `crates/jackin-config/src/app_config.rs`: update the module-level doc comment (lines 5–7) and add four `pub mod` declarations before the `#[cfg(test)] mod tests;` line at the bottom.

    Doc comment change — replace:
    ```
    //! Behavior (load, save, workspace CRUD, mount resolution, role
    //! resolution) lives in the sibling `app_config_persist`,
    //! `app_config_workspaces`, `app_config_mounts`, and `app_config_roles`
    //! modules.
    ```
    With:
    ```
    //! Behavior (load, save, workspace CRUD, mount resolution, role
    //! resolution) lives in the child modules `mounts`, `persist`,
    //! `roles`, and `workspaces`.
    ```

    Append before `#[cfg(test)] mod tests;` (the final line of the file):
    ```rust
    pub mod mounts;
    pub mod persist;
    pub mod roles;
    pub mod workspaces;
    ```

    The final four lines of `app_config.rs` must read:
    ```rust
    pub mod mounts;
    pub mod persist;
    pub mod roles;
    pub mod workspaces;

    #[cfg(test)]
    mod tests;
    ```

11. Edit `crates/jackin-config/src/lib.rs`: replace lines 10–13 (the four `pub mod app_config_*` declarations) and update the three `pub use` lines (31–38) that reference them.

    Replace (lines 10–13):
    ```rust
    pub mod app_config_mounts;
    pub mod app_config_persist;
    pub mod app_config_roles;
    pub mod app_config_workspaces;
    ```
    With: *(delete these four lines entirely — the modules are now declared inside `app_config.rs`)*

    Replace (lines 31–38 in original numbering; adjust for the four deleted lines):
    ```rust
    pub use app_config_mounts::{GlobalMountRow, WorkspaceGlobalMountRows};
    pub use app_config_persist::{
        config_needs_split_migration, load_split_config, validate_reserved_env_names,
    };
    pub use app_config_roles::{
        BUILTIN_ROLES, build_github_env_layers, resolve_github_mode, resolve_mode,
        resolve_mode_with_trace, resolve_sync_source_dir,
    };
    ```
    With:
    ```rust
    pub use app_config::mounts::{GlobalMountRow, WorkspaceGlobalMountRows};
    pub use app_config::persist::{
        config_needs_split_migration, load_split_config, validate_reserved_env_names,
    };
    pub use app_config::roles::{
        BUILTIN_ROLES, build_github_env_layers, resolve_github_mode, resolve_mode,
        resolve_mode_with_trace, resolve_sync_source_dir,
    };
    ```

12. Edit `crates/jackin-config/src/editor.rs` line 16:
    - Old: `use crate::app_config_persist::{load_split_config, validate_reserved_env_names};`
    - New: `use crate::app_config::persist::{load_split_config, validate_reserved_env_names};`

13. Delete the old source files:
    ```
    rm crates/jackin-config/src/app_config_mounts.rs
    rm crates/jackin-config/src/app_config_mounts/tests.rs
    rmdir crates/jackin-config/src/app_config_mounts
    rm crates/jackin-config/src/app_config_persist.rs
    rm crates/jackin-config/src/app_config_persist/tests.rs
    rmdir crates/jackin-config/src/app_config_persist
    rm crates/jackin-config/src/app_config_roles.rs
    rm crates/jackin-config/src/app_config_roles/tests.rs
    rmdir crates/jackin-config/src/app_config_roles
    rm crates/jackin-config/src/app_config_workspaces.rs
    rm crates/jackin-config/src/app_config_workspaces/tests.rs
    rmdir crates/jackin-config/src/app_config_workspaces
    ```

14. Stage all deletions and new files with git:
    ```
    git add crates/jackin-config/src/
    git add -u crates/jackin-config/src/app_config_mounts.rs
    git add -u crates/jackin-config/src/app_config_persist.rs
    git add -u crates/jackin-config/src/app_config_roles.rs
    git add -u crates/jackin-config/src/app_config_workspaces.rs
    ```
    (Or simply `git add -A crates/jackin-config/src/` to stage all changes.)

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → exit 0, no output
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0, no warnings
- `cargo nextest run -p jackin-config` → all tests pass, zero failures
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK; arch section is informational only
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → run to confirm no grandfathered entries are needed for new files (all are well under 2000L); if any listed file in `file-size-budget.toml` was one of the deleted files, remove it from the budget file (none are expected — the old `app_config_*` files have no budget entries)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
- `crates/jackin-config/src/app_config_mounts.rs` and its three sibling `app_config_*.rs` files do not exist
- `crates/jackin-config/src/app_config/mounts.rs`, `app_config/persist.rs`, `app_config/roles.rs`, `app_config/workspaces.rs` exist
- Matching `tests.rs` files exist under `app_config/{mounts,persist,roles,workspaces}/tests.rs`
- `cargo nextest run -p jackin-config` exits 0
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0

**Rollback:** `git restore crates/jackin-config/src/` then `git clean -f crates/jackin-config/src/app_config/mounts.rs crates/jackin-config/src/app_config/mounts/ crates/jackin-config/src/app_config/persist.rs crates/jackin-config/src/app_config/persist/ crates/jackin-config/src/app_config/roles.rs crates/jackin-config/src/app_config/roles/ crates/jackin-config/src/app_config/workspaces.rs crates/jackin-config/src/app_config/workspaces/`

**Open questions:** none

---

### G0 — Build shared Elm runtime contract in jackin-tui (D5)

- **Goal:** Add `Component` and `View` trait definitions to `crates/jackin-tui/src/runtime.rs` and an Architecture-Invariant `//!` section to `crates/jackin-tui/src/lib.rs`, encoding the TEA contract described in D5; no stack is migrated in this slice.
- **Preconditions:** none
- **Pattern:** additive file edit — append new public trait symbols to existing `runtime.rs`; update `//!` header in `lib.rs`; update two docs files
- **Touches:** `crates/jackin-tui/src/runtime.rs` (modified), `crates/jackin-tui/src/lib.rs` (modified), `docs/content/docs/roadmap/codebase-health-enforcement.mdx` (modified), `docs/content/docs/reference/getting-oriented/codebase-map.mdx` (modified)

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Open `crates/jackin-tui/src/runtime.rs`. Locate the blank line at line 236 (between the closing `}` of the `UpdateResult<E>` impl block at line 235 and `#[cfg(test)]` at line 237). Insert the following block of text between line 235 and line 237, replacing `\n#[cfg(test)]` with the new text followed by `\n\n#[cfg(test)]`:

   ```rust
   /// TEA component contract: translates raw terminal input events into typed
   /// messages for the app's central `update` function.
   ///
   /// `Ev` is the surface-specific event type (e.g. [`crossterm::event::Event`]
   /// for keyboard/mouse-driven surfaces; raw bytes or a decoded action type for
   /// the in-container multiplexer). `Msg` is the domain message the central
   /// `update` consumes. Components maintain their own sub-state (e.g. cursor
   /// position, focus) but must not mutate app model state; they only produce
   /// messages.
   ///
   /// # Contract
   ///
   /// - `handle_event` is non-blocking and must not perform I/O.
   /// - Returning `None` means the event was not consumed; the runtime may
   ///   offer the event to the next component in the chain.
   /// - Returning `Some(msg)` means the event was consumed; the runtime calls
   ///   the central `update` with `msg`.
   pub trait Component<Ev, Msg> {
       fn handle_event(&mut self, event: &Ev) -> Option<Msg>;
   }

   /// TEA view contract: renders an app model into one rectangular region of a
   /// ratatui [`ratatui::Frame`].
   ///
   /// Implementations are observational: they read `model` but must not mutate
   /// it. All visible output (widget painting, cursor positioning, scroll
   /// indicators) flows through the `frame` and `area` arguments. `View` never
   /// drives subscriptions or spawns work.
   pub trait View<Model> {
       fn render(
           &self,
           model: &Model,
           frame: &mut ratatui::Frame<'_>,
           area: ratatui::layout::Rect,
       );
   }
   ```

   The exact old string to replace (use an Edit tool with this as `old_string`):

   ```
   }

   #[cfg(test)]
   mod tests;
   ```

   The exact new string (`new_string`):

   ```
   }

   /// TEA component contract: translates raw terminal input events into typed
   /// messages for the app's central `update` function.
   ///
   /// `Ev` is the surface-specific event type (e.g. [`crossterm::event::Event`]
   /// for keyboard/mouse-driven surfaces; raw bytes or a decoded action type for
   /// the in-container multiplexer). `Msg` is the domain message the central
   /// `update` consumes. Components maintain their own sub-state (e.g. cursor
   /// position, focus) but must not mutate app model state; they only produce
   /// messages.
   ///
   /// # Contract
   ///
   /// - `handle_event` is non-blocking and must not perform I/O.
   /// - Returning `None` means the event was not consumed; the runtime may
   ///   offer the event to the next component in the chain.
   /// - Returning `Some(msg)` means the event was consumed; the runtime calls
   ///   the central `update` with `msg`.
   pub trait Component<Ev, Msg> {
       fn handle_event(&mut self, event: &Ev) -> Option<Msg>;
   }

   /// TEA view contract: renders an app model into one rectangular region of a
   /// ratatui [`ratatui::Frame`].
   ///
   /// Implementations are observational: they read `model` but must not mutate
   /// it. All visible output (widget painting, cursor positioning, scroll
   /// indicators) flows through the `frame` and `area` arguments. `View` never
   /// drives subscriptions or spawns work.
   pub trait View<Model> {
       fn render(
           &self,
           model: &Model,
           frame: &mut ratatui::Frame<'_>,
           area: ratatui::layout::Rect,
       );
   }

   #[cfg(test)]
   mod tests;
   ```

2. Open `crates/jackin-tui/src/lib.rs`. Replace the existing `//!` doc-comment block (lines 1–9) with the same text extended by an Architecture Invariant section. The exact old string to replace:

   ```
   //! Shared TUI tokens, models, and components used by jackin❯'s
   //! terminal surfaces.
   //!
   //! Backend-neutral types such as RGB tokens, tab-cell layout, hint
   //! spans, text-field state, and scroll metrics stay at the crate
   //! root or in small helper modules. Ratatui components live under
   //! [`components`], with color adapters in [`theme`]. Surface crates
   //! own domain state and compose these pieces instead of re-declaring
   //! palette values or reimplementing visual primitives.
   ```

   The exact new string:

   ```
   //! Shared TUI tokens, models, and components used by jackin❯'s
   //! terminal surfaces.
   //!
   //! Backend-neutral types such as RGB tokens, tab-cell layout, hint
   //! spans, text-field state, and scroll metrics stay at the crate
   //! root or in small helper modules. Ratatui components live under
   //! [`components`], with color adapters in [`theme`]. Surface crates
   //! own domain state and compose these pieces instead of re-declaring
   //! palette values or reimplementing visual primitives.
   //!
   //! # Architecture Invariant
   //!
   //! `jackin-tui` is a **presentation-layer** design-system crate.
   //! Allowed upstream dependencies: `jackin-core` (domain vocabulary only).
   //! Must **not** depend on any application, infrastructure, or entry crate
   //! (`jackin-runtime`, `jackin-env`, `jackin-docker`, `jackin-launch*`,
   //! `jackin-console`, `jackin-capsule`, or `jackin`).
   //!
   //! The shared Elm runtime contract lives in [`runtime`]: one
   //! [`runtime::UpdateResult`] return per `update` call,
   //! [`runtime::Component`] for event→message translation, and
   //! [`runtime::View`] for model→frame rendering. Surface crates
   //! implement these traits; `jackin-tui` only defines them.
   ```

3. Open `docs/content/docs/roadmap/codebase-health-enforcement.mdx`. Find the exact line (line 261):

   ```
   - [ ] **G0 — design + build the shared contract in `jackin-tui` (D5).** `Model`/`Message`/`update` + a `Component`/`View` contract; one central `update` per app. No stack migrated yet — just the runtime + a doc'd contract.
   ```

   Replace `- [ ]` with `- [x]` on that line (leave the rest of the line unchanged). Exact old string:

   ```
   - [ ] **G0 — design + build the shared contract in `jackin-tui` (D5).** `Model`/`Message`/`update` + a `Component`/`View` contract; one central `update` per app. No stack migrated yet — just the runtime + a doc'd contract.
   ```

   Exact new string:

   ```
   - [x] **G0 — design + build the shared contract in `jackin-tui` (D5).** `Model`/`Message`/`update` + a `Component`/`View` contract; one central `update` per app. No stack migrated yet — just the runtime + a doc'd contract.
   ```

4. Open `docs/content/docs/reference/getting-oriented/codebase-map.mdx`. Find line 231, which currently reads (exact string):

   ```
   - <RepoFile path="crates/jackin-tui/src/runtime.rs">crates/jackin-tui/src/runtime.rs</RepoFile> — small shared runtime contracts such as `Dirty` and `UpdateResult`, used by host and launch update functions.
   ```

   Replace it with (exact new string):

   ```
   - <RepoFile path="crates/jackin-tui/src/runtime.rs">crates/jackin-tui/src/runtime.rs</RepoFile> — shared TEA runtime contract: `Dirty`, `UpdateResult`, `Subscription`, `Component` (event→message translator), and `View` (model→frame renderer) traits; all TUI surface crates depend on these definitions.
   ```

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → exits 0, no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exits 0, zero warnings
- `cargo nextest run -p jackin-tui` → all tests pass (the existing `runtime/tests.rs` tests are unmodified; no new tests added in this slice)
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all OK (both new traits are well under the 2000L production cap; `runtime.rs` grows from 238 to ~280 lines; `lib.rs` from 519 to ~531 lines; neither is in the budget allowlist)
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED (no production logic was altered; only new public trait definitions added to `jackin-tui`)

**Done when:**
- `pub trait Component<Ev, Msg>` and `pub trait View<Model>` are exported from `crates/jackin-tui::runtime` and the crate compiles cleanly
- `crates/jackin-tui/src/lib.rs` `//!` header contains the Architecture Invariant section
- `docs/content/docs/roadmap/codebase-health-enforcement.mdx` shows `[x]` for G0
- `docs/content/docs/reference/getting-oriented/codebase-map.mdx` line for `runtime.rs` mentions `Component` and `View`

**Rollback:** `git restore crates/jackin-tui/src/runtime.rs crates/jackin-tui/src/lib.rs docs/content/docs/roadmap/codebase-health-enforcement.mdx docs/content/docs/reference/getting-oriented/codebase-map.mdx`

**Open questions:** none

---

### G1 — Migrate jackin-launch-tui onto the shared runtime

- **Goal:** Apply the G0 shared Elm contract from `jackin-tui` to the launch cockpit stack (the smallest of the four TUI stacks) and rename the non-canonical `tui/app.rs` stem to `tui/model.rs` (D7), proving the shared runtime in the simplest consumer first.
- **Preconditions:** B1 (crate renamed `jackin-launch` → `jackin-launch-tui`), G0 (shared `Model`/`Message`/`update` + `Component`/`View` contract built in `jackin-tui`)
- **Pattern:** Parallel Change (rename module path; all callers updated in the same commit; no external API break because all public symbols are re-exported from `lib.rs`) + Branch by Abstraction (implement G0 traits on existing types without altering logic)
- **Touches:**
  - `crates/jackin-launch-tui/src/tui/app.rs` — renamed to `model.rs` (git mv; file body unchanged)
  - `crates/jackin-launch-tui/src/tui.rs` — mod declaration updated
  - `crates/jackin-launch-tui/src/lib.rs` — re-export path updated; G0 trait impls or wiring added (TODO: see Open questions)
  - `crates/jackin-launch-tui/src/tui/message.rs` — import path updated
  - `crates/jackin-launch-tui/src/tui/update.rs` — import path updated
  - `crates/jackin-launch-tui/src/tui/view.rs` — import path updated (inline test block)
  - `crates/jackin-launch-tui/src/tui/components/build_log_dialog/tests.rs` — import path updated
  - `crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs` — import path updated (two lines)
  - `crates/jackin-launch-tui/src/tui/components/container_info_dialog/tests.rs` — import path updated
  - `docs/content/docs/reference/getting-oriented/codebase-map.mdx` — file path references updated
  - `docs/content/docs/roadmap/codebase-health-enforcement.mdx` — G1 checkbox ticked
  - `crates/jackin-tui/src/...` — TODO(investigate): files added/modified by G0 that G1 implements

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

> **Note on paths:** B1 must have already renamed the crate directory from `crates/jackin-launch/` to `crates/jackin-launch-tui/` and updated `Cargo.toml`, `Cargo.lock`, workspace `members`, `jackin-runtime/Cargo.toml`, and all external `use jackin_launch::` → `use jackin_launch_tui::`. Steps below assume that has landed. If B1 has not yet run, all paths below start with `crates/jackin-launch/` instead, and the crate name is still `jackin-launch`; adjust accordingly.

**Part A — D7 stem normalization (`app.rs` → `model.rs`)**

1. Run `git mv crates/jackin-launch-tui/src/tui/app.rs crates/jackin-launch-tui/src/tui/model.rs`. File body is unchanged; only the filename moves.

2. In `crates/jackin-launch-tui/src/tui.rs`, replace the single line:
   ```rust
   pub mod app;
   ```
   with:
   ```rust
   pub mod model;
   ```
   All other `pub mod` lines in `tui.rs` remain unchanged.

3. In `crates/jackin-launch-tui/src/lib.rs` at the `pub use tui::app::{` block (lines 12–17 before this PR), replace:
   ```rust
   pub use tui::app::{
   ```
   with:
   ```rust
   pub use tui::model::{
   ```
   The list of re-exported symbols (`FailureCopyTarget`, `LaunchFailure`, `LaunchIdentity`, `LaunchStage`, `LaunchTargetKind`, `LaunchView`, `PromptContextLine`, `PromptResult`, `StageLabelTransition`, `StageStatus`, `StageView`) is unchanged.

4. In `crates/jackin-launch-tui/src/tui/message.rs` line 6, replace:
   ```rust
   use crate::tui::app::{FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, StageStatus};
   ```
   with:
   ```rust
   use crate::tui::model::{FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, StageStatus};
   ```

5. In `crates/jackin-launch-tui/src/tui/update.rs` line 6, replace:
   ```rust
   use crate::tui::app::{LaunchStage, LaunchView, StageLabelTransition, StageStatus, StageView};
   ```
   with:
   ```rust
   use crate::tui::model::{LaunchStage, LaunchView, StageLabelTransition, StageStatus, StageView};
   ```

6. In `crates/jackin-launch-tui/src/tui/view.rs` (inside the inline `mod tests {` block at approximately line 190), replace:
   ```rust
   use crate::tui::app::{LaunchFailure, LaunchIdentity, LaunchTargetKind};
   ```
   with:
   ```rust
   use crate::tui::model::{LaunchFailure, LaunchIdentity, LaunchTargetKind};
   ```
   Do not move the tests out of `view.rs`; that file is grandfathered in `test-layout-allowlist.toml` (path entry will have been updated by B1 to `crates/jackin-launch-tui/src/tui/view.rs`).

7. In `crates/jackin-launch-tui/src/tui/components/build_log_dialog/tests.rs` line 2, replace:
   ```rust
   use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
   ```
   with:
   ```rust
   use crate::tui::model::{LaunchIdentity, LaunchTargetKind};
   ```

8. In `crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs` line 1, replace:
   ```rust
   use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
   ```
   with:
   ```rust
   use crate::tui::model::{LaunchIdentity, LaunchTargetKind};
   ```

9. In `crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs` line 4, replace:
   ```rust
   use crate::{LaunchStage, tui::app::LaunchFailure};
   ```
   with:
   ```rust
   use crate::{LaunchStage, tui::model::LaunchFailure};
   ```

10. In `crates/jackin-launch-tui/src/tui/components/container_info_dialog/tests.rs` line 4, replace:
    ```rust
    use crate::tui::app::{LaunchIdentity, LaunchTargetKind, LaunchView};
    ```
    with:
    ```rust
    use crate::tui::model::{LaunchIdentity, LaunchTargetKind, LaunchView};
    ```

**Part B — G0 contract implementation**

> **TODO(investigate): The exact steps below cannot be determined until G0 lands.** See Open questions. The executor must read the G0 output in `crates/jackin-tui/src/runtime.rs` (and any new files G0 added to `crates/jackin-tui/src/`) to learn the trait names, method signatures, and any new types before proceeding.

11. TODO(investigate): Identify every trait that G0 added to `crates/jackin-tui/src/runtime.rs` (or to new files under `crates/jackin-tui/src/`) that forms the "app contract" (likely something like `TuiApp`, `TuiModel`, or `AppContract`). Confirm whether G0 provides a shared run loop function (and whether `run.rs` in `jackin-launch-tui` needs to be replaced or kept).

12. TODO(investigate): In `crates/jackin-launch-tui/src/tui/model.rs` (formerly `app.rs`), implement every G0 trait on the appropriate type (`LaunchView` or a new wrapper struct). Method bodies must contain zero logic that is not already present in the existing functions in `update.rs` / `view.rs`; they delegate to those functions only.

13. TODO(investigate): If G0 provides a shared run loop, assess whether `crates/jackin-launch-tui/src/tui/run.rs` is replaced (deleted and replaced with a thin adapter calling the G0 loop) or kept as-is and the G0 trait impl is added alongside it. This is structure-only: no behavior change in the render/input path.

14. TODO(investigate): If G0 defines a `Component`/`View` contract, assess whether any of the launch-local components under `crates/jackin-launch-tui/src/tui/components/` need to implement those traits. Likely none in G1 (G1 is the minimal proof), but confirm.

**Part C — Docs and housekeeping**

15. In `docs/content/docs/reference/getting-oriented/codebase-map.mdx` lines 116–122, update the two inline references to `tui/app.rs` (if B1 did not already do this as part of the crate rename). After G1 the canonical stem for the model is `tui/model.rs`; update `<RepoFile>` tags and surrounding prose accordingly. Do not add prose about G0 contract details; just fix the file-path references.

16. In `docs/content/docs/roadmap/codebase-health-enforcement.mdx`, locate the `- [ ] **G1 — migrate `jackin-launch-tui`**` line and change `- [ ]` to `- [x]`.

17. Run `cargo xtask lint files --print-budget` and `cargo xtask lint tests --print-allowlist`. If any file dropped under its cap or any allowlist violation was resolved by this PR, delete the entry from `file-size-budget.toml` or `test-layout-allowlist.toml` respectively. (No jackin-launch-tui production file is currently in the budget; no change expected unless Part B introduced new files. The `test-layout-allowlist.toml` entry for `crates/jackin-launch-tui/src/tui/view.rs` stays because the inline tests in `view.rs` remain.)

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → exits 0, no reformatting needed
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-launch-tui` → all tests pass unmodified
- `cargo nextest run -p jackin-runtime` → all tests pass unmodified (jackin-runtime depends on jackin-launch-tui and its re-exported symbols must still resolve after the `tui::app` → `tui::model` rename)
- `cargo nextest run --workspace` → all tests pass unmodified; behavioral specs for `runtime-launch` and `op-picker` pass unmodified
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all green

**Done when:** `cargo nextest run --workspace` is green; the file `crates/jackin-launch-tui/src/tui/app.rs` no longer exists; `crates/jackin-launch-tui/src/tui/model.rs` exists with identical byte content; all ten import sites read `tui::model` not `tui::app`; and G0 traits are implemented on the launch model (Part B complete per the G0 output).

**Rollback:** `git restore crates/jackin-launch-tui/src/tui.rs crates/jackin-launch-tui/src/lib.rs crates/jackin-launch-tui/src/tui/message.rs crates/jackin-launch-tui/src/tui/update.rs crates/jackin-launch-tui/src/tui/view.rs crates/jackin-launch-tui/src/tui/components/build_log_dialog/tests.rs crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs crates/jackin-launch-tui/src/tui/components/container_info_dialog/tests.rs docs/content/docs/reference/getting-oriented/codebase-map.mdx docs/content/docs/roadmap/codebase-health-enforcement.mdx` then `git mv crates/jackin-launch-tui/src/tui/model.rs crates/jackin-launch-tui/src/tui/app.rs` and undo any G0 trait impls added in Part B.

**Open questions:**

1. **G0 contract (blocking Part B):** What exact traits and types does G0 add to `crates/jackin-tui/src/runtime.rs` (or new files)? Trait names, method signatures, whether a shared run-loop function is included, and whether `Component`/`View` are separate traits or combined. The executor MUST read the G0 output before attempting Part B steps.
2. **Does G0 replace `run.rs`?** The launch TUI has a 41 KB hand-rolled `run.rs` containing `RichRenderer` and `RichDriver` (the render task, input polling loop, and cockpit rendering). If G0's shared runtime replaces this loop, Part B must assess whether `run.rs` is deleted, kept, or reduced to a thin adapter. If G0 is purely a trait contract with no shared run loop (most likely for a "no stack migrated yet" G0), `run.rs` stays completely unchanged in G1.
3. **`app.rs` in `tui/app.rs` body:** `app.rs` contains `pub use jackin_core::PromptResult;` (a re-export, not a definition). Confirm this re-export line moves verbatim into `model.rs` — no logic change.
4. **B1 path:** Confirm B1 has already updated `test-layout-allowlist.toml` from `crates/jackin-launch/src/tui/view.rs` to `crates/jackin-launch-tui/src/tui/view.rs`. If B1 did not do this, G1 must do it (replace the old path string with the new one in `test-layout-allowlist.toml`).

---

### G2 — Migrate the capsule TUI onto the shared runtime

> **Not yet detailed.** The detailing agent for this slice failed mid-run. Re-detail it before executing (re-run the codebase-health-playbook workflow). The strategy page describes the intended shape; do **not** improvise from the summary.

---

### G3 — Migrate jackin-console onto the shared runtime (+ unify-settings)

- **Goal:** Migrate the `jackin-console` Elm loop onto the G0 shared runtime contract and, in the same PR, collapse the settings surface's three modal enums into the editor's unified `ConsoleModal`, making editor + settings one implementation per the `unify-settings-editor-surfaces` design.
- **Preconditions:** G0 (shared contract in `jackin-tui` — `Model`/`Message`/`update`/`Component`/`View` traits defined and published), G1 (`jackin-launch-tui` migrated), G2 (capsule TUI migrated). None of G0–G2 are shipped as of the current codebase; G3 is blocked until they are.
- **Pattern:** Parallel Change + Branch by Abstraction — introduce the unified modal type and `ConfigSurfaceHost` trait, migrate settings callers, delete the three settings-only modal enums; then wire the console `run.rs` event loop to the G0 runtime contract in a second inner step.
- **Touches:**
  - `crates/jackin-tui/src/runtime.rs` — TODO(investigate): G0 may add `Model`/`Component`/`View` traits here
  - `crates/jackin-console/src/tui/app.rs` (5095 L, grandfathered in `file-size-budget.toml` at L39)
  - `crates/jackin-console/src/tui/screens/editor/model.rs` (4176 L, grandfathered at L43)
  - `crates/jackin-console/src/tui/screens/settings/model.rs` (3852 L, grandfathered at L47)
  - `crates/jackin-console/src/tui/screens/editor/update.rs` (892 L)
  - `crates/jackin-console/src/tui/screens/editor/view.rs` (2389 L, grandfathered at L59)
  - `crates/jackin-console/src/tui/screens/settings/update.rs` (1717 L)
  - `crates/jackin-console/src/tui/screens/settings/view.rs` (1635 L)
  - `crates/jackin-console/src/tui/screens/workspaces/update.rs` (1430 L)
  - `crates/jackin-console/src/tui/screens/workspaces/view.rs`
  - `crates/jackin-console/src/tui/input/editor.rs` (1138 L)
  - `crates/jackin-console/src/tui/input/global_mounts.rs` (1336 L)
  - `crates/jackin-console/src/tui/state.rs` (446 L)
  - `crates/jackin-console/src/tui/console.rs` (63 L)
  - `crates/jackin-console/src/tui/run.rs` (611 L)
  - `crates/jackin-console/src/tui/update.rs` (918 L)
  - `crates/jackin-console/src/tui/view.rs` (754 L)
  - `crates/jackin-console/src/tui/message.rs` (519 L)
  - `crates/jackin-console/src/tui/subscriptions.rs` (248 L)
  - `crates/jackin/src/console/tui/run.rs` (827 L)
  - `crates/jackin/src/console/tui.rs` (162 L)
  - `crates/jackin/src/console/effects.rs` (1365 L)
  - `file-size-budget.toml`
  - `docs/content/docs/roadmap/codebase-health-enforcement.mdx`
  - `docs/content/docs/roadmap/unify-settings-editor-surfaces.mdx`

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

**Phase 3a — Resolve open design decisions (MUST precede any code change)**

1. TODO(investigate): Confirm the exact G0 contract traits published by `crates/jackin-tui/src/runtime.rs` after G0 ships — specifically the names and signatures of any `Model`, `Component`, `View`, or event-loop helper traits added there. Every subsequent wiring step depends on these exact names.

2. TODO(investigate): Settle the three open decisions from `unify-settings-editor-surfaces.mdx` ("Open (raise before the relevant slice)"):
   - (a) Whether the shared `ConfigModal` / `AuthModal` types live in `jackin-console` beside the shared widgets, or fold directly into the existing `ConsoleModal` enum in `crates/jackin-console/src/tui/app.rs` at line 762.
   - (b) Whether the per-domain shared dispatch/apply helpers (`handle_auth_modal<H>`, etc.) live in a new `jackin-console/src/tui/config_surface/` module versus the existing `input/editor.rs` + `input/global_mounts.rs`.
   - (c) How to reconcile the trust asymmetry (settings has a first-class `Trust` tab via `SettingsTab::Trust` and `SettingsTrustState`; the editor only has a confirm flow via `Modal::Confirm { target: ConfirmTarget::TrustRoleSource { .. }, .. }`) — promote editor to a Trust tab, or model both through one trust-confirm flow.

**Phase 3b — Collapse the modal taxonomy (step 7 of unify-settings sequencing)**

3. In `crates/jackin-console/src/tui/screens/settings/model.rs`, study the three modal enums:
   - `pub enum SettingsEnvModal<TextInputState, SourcePickerState, OpPickerState, RolePickerState, ScopePickerState, ConfirmState>` at line 576 — variants: `Text`, `SourcePicker`, `OpPicker`, `RolePicker`, `ScopePicker`, `Confirm`.
   - `pub enum GlobalMountModal<TextInputState, FileBrowserState, MountDstChoiceState, ScopePickerState, RolePickerState, ConfirmState, ConfirmSaveState>` at line 1073.
   - `pub enum SettingsAuthModal<TextInputState, SourcePickerState, OpPickerState, FileBrowserState, AuthFormTarget, AuthForm, AuthFormFocus>` at line 1613.
   All variants have exact counterparts in `ConsoleModal` at `crates/jackin-console/src/tui/app.rs` line 762. Confirm the variant-by-variant match using the table in `unify-settings-editor-surfaces.mdx` (section "Modal taxonomy — same concepts, 1 enum vs 3") before making any change.

4. TODO(investigate): Determine the concrete plan for collapsing the per-panel modal slots. Currently `SettingsEnvState<EnvValue, Modal>`, `SettingsAuthState<EnvValue, Modal, PendingOpCommit>`, and `GlobalMountsState<Row, Modal>` each carry their own `Modal` type parameter (confirmed from `state.rs` lines 107–170). After the collapse, all three panels share `ConsoleModal` (or the resolved `ConfigModal`) as their modal type, OR the per-panel modal slots are removed in favor of ONE `modal: Option<ConsoleModal>` + `modal_parents: Vec<ConsoleModal>` on `SettingsState` itself (matching `EditorState.modal` + `EditorState.modal_parents` at `editor/model.rs` lines 263–264). The choice must be made before step 5.

5. **After decisions in steps 2 and 4 are resolved**: In `crates/jackin-console/src/tui/screens/settings/model.rs`, replace the `ErrorPopup` type parameter in `SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>` with a unified modal slot per the agreed design. Remove the `pub error_popup: Option<ErrorPopup>` field (line 73) from `SettingsState`. All callers at `crates/jackin-console/src/tui/state.rs` (line 112) and in `crates/jackin/src/console/effects.rs` must be updated in the same step.

6. **After step 5**: Update all `SettingsState.dismiss_error_popup` and `SettingsState.open_error_popup` call sites (currently `settings/model.rs` lines 120–296 and callers in `settings/update.rs`, `input/global_mounts.rs`) to use the unified modal variant (`ConsoleModal::ErrorPopup { .. }`) via the single agreed modal slot. Confirm via `grep -rn "dismiss_error_popup\|open_error_popup" crates/jackin-console/src/` for the exhaustive call-site list.

7. **After step 6**: In `crates/jackin-console/src/tui/state.rs`, delete the three type aliases:
   - `pub type SettingsEnvModal<'a> = crate::tui::screens::settings::model::SettingsEnvModal<...>` (lines 137–144)
   - `pub type SettingsAuthModal<'a> = crate::tui::screens::settings::model::SettingsAuthModal<...>` (lines 152–160)
   - `pub type GlobalMountModal<'a> = crate::tui::screens::settings::model::GlobalMountModal<...>` (lines 162–170)
   Update the downstream type aliases `GlobalMountsState<'a>` (line 107), `SettingsEnvState<'a>` (line 134), and `SettingsAuthState` (line 146) to use the unified modal type. These are the only callers; confirm with `grep -rn "SettingsEnvModal\|SettingsAuthModal\|GlobalMountModal" crates/`.

8. **After step 7**: Delete `pub enum SettingsEnvModal<...>` (line 576), `pub enum GlobalMountModal<...>` (line 1073), and `pub enum SettingsAuthModal<...>` (line 1613) from `crates/jackin-console/src/tui/screens/settings/model.rs`. Confirm no remaining usages: `grep -rn "SettingsEnvModal\|GlobalMountModal\|SettingsAuthModal" crates/` must return zero results before this step is committed.

**Phase 3c — Per-domain edit-flow unification (steps 8–12 of unify-settings sequencing)**

9. TODO(investigate): Specify the exact `ConfigSurfaceHost` trait definition and placement (decision 2b above). The design doc gives the shape at `unify-settings-editor-surfaces.mdx` lines 98–105:
   ```rust
   pub trait ConfigSurfaceHost {
       fn scopes(&self) -> &[ConfigScope];
       fn stash_modal(&mut self, child: ConfigModal);
       fn restore_modal(&mut self);
       fn show_error(&mut self, reason: String);
   }
   ```
   but the concrete `ConfigScope` and `ConfigModal` types must be resolved from decision 2a.

10. TODO(investigate): For each domain (mounts, env/secrets, trust, general), specify the exact twin-function pairs from the duplication map in `unify-settings-editor-surfaces.mdx` (section "Largest twinned function families") that will be collapsed. Each twin pair becomes one generic function over `H: ConfigSurfaceHost`. The pairs involve symbols in `input/editor.rs` (editor side) and `input/global_mounts.rs` (settings side). Enumerate every `fn open_*`, `fn apply_*`, `fn restore_*`, `fn persist_*`, `fn clear_*` twin pair — there are at least 20 such pairs documented in the roadmap — and map each to the file and line range in the current code.

**Phase 3d — Wire console event loop to G0 runtime contract**

11. TODO(investigate): After G0 ships, read the G0 shared runtime contract (the `Model`/`Component`/`View`/event-loop helper traits in `crates/jackin-tui/src/runtime.rs`). Map the existing `crates/jackin-console/src/tui/run.rs` `ConsoleScreenStage`, `ConsoleApp`, `ManagerState` onto those traits. The concrete wiring points are:
    - `crates/jackin-console/src/tui/console.rs` line 15: `pub type ConsoleState = ConsoleApp<ManagerState<'static>, LoadWorkspaceInput, RoleSelector, Rc<RefCell<OpCache>>>`
    - `crates/jackin-console/src/tui/update.rs` line 12: `pub type ConsoleUpdate<E> = UpdateResult<E>` (already uses `jackin_tui::runtime::UpdateResult`)
    - `crates/jackin/src/console/tui/run.rs` (827 L) — the full event loop; identify which portions the G0 shared runtime absorbs vs which must remain as console-specific glue.

12. TODO(investigate): After G1 and G2 ship, diff `jackin-launch-tui`'s wiring to G0 and the capsule TUI's wiring to G0. Use those two proven patterns as the template for the console wiring in step 11. Do not guess the pattern from G0 alone; the two prior migrations are the proof of the approach.

**Phase 3e — W5 file splits on the unified surface**

13. After phases 3b–3d are complete and the two duplicated surfaces are ONE implementation, apply the W5 split pattern to `crates/jackin-console/src/tui/app.rs` (5095 L), `crates/jackin-console/src/tui/screens/editor/model.rs` (now the unified surface), and `crates/jackin-console/src/tui/screens/settings/model.rs` (now smaller after the modal collapse). The roadmap's instruction: "split the unified surface, not the duplicated pair." Follow the project split pattern (crates/AGENTS.md): keep the original file as coordinator; move clusters to sibling `<module>/<name>.rs`; use `pub(super)` for moved items; re-export with `pub(crate) use self::<name>::...`; no `mod.rs`; no wildcard imports. Specific sibling targets for `app.rs`:
    - TODO(investigate): The exact cluster boundaries within `app.rs` (5095 L) — `ConsoleModal` (lines 762–851), `ConsoleManagerStage` (lines 255–268), dispatch helpers (`console_input_dispatch_plan`, lines 432–471), `LaunchAgentPromptState`/`LaunchRolePromptState`/`LaunchProviderPickerState` trait families (lines 33–250) — must be confirmed against the final state of `app.rs` after the modal collapse in phase 3b.

14. After all splits in step 13, update `file-size-budget.toml`: run `cargo run -p jackin-xtask --locked -- lint files --print-budget` and delete the entries for any file that now sits under the 2000 L production cap. Commit the refreshed budget in the same PR.

**Phase 3f — Docs and roadmap update**

15. In `docs/content/docs/roadmap/codebase-health-enforcement.mdx`, check the box for slice G3 and update the W5 ongoing-decompositions list (lines 165–173) to reflect which files are now under cap. Update the W6 Phase G checklist (lines 260–264) marking G3 done. No hard-wrap in prose paragraphs.

16. In `docs/content/docs/roadmap/unify-settings-editor-surfaces.mdx`, advance the **Status** line from "Partially implemented" to "Implemented" if all steps 1–12 of the sequencing (lines 169–185) are completed in this PR; otherwise mark which steps landed. Update the **Definition of done** checklist (lines 195–199).

17. Update `PROJECT_STRUCTURE.md` and the [Codebase Map](/reference/getting-oriented/codebase-map/) page if any new sibling modules were created in step 13.

**Verify** (run in order; STOP and revert on the first failure):
- `cargo fmt --check` → zero diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-console` → all tests pass
- `cargo nextest run -p jackin` → all tests pass (includes `jackin/src/console/tui/input/editor/tests.rs` 2636 L and `state/tests.rs` 893 L)
- `cargo nextest run --workspace --all-features` → entire workspace green
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout + dependency-direction all OK
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → run and compare; delete any grandfathered entry now under the 2000 L cap
- behavioral specs `runtime-launch` and `op-picker` pass **unmodified** — no test source lines changed

**Done when:**
- `SettingsEnvModal`, `GlobalMountModal`, and `SettingsAuthModal` enums are deleted from `crates/jackin-console/src/tui/screens/settings/model.rs`; `grep -rn "SettingsEnvModal\|GlobalMountModal\|SettingsAuthModal" crates/` returns zero hits
- `SettingsState.error_popup` field is gone; settings errors surface through `ConsoleModal::ErrorPopup`
- One `open_sub_modal`/`pop_modal_chain`/`clear_modal_chain` implementation serves both editor and settings
- The console's `run.rs` event loop passes through the G0 shared runtime contract (confirmed by G0 trait name)
- `file-size-budget.toml` entries for `app.rs`, `editor/model.rs`, and `settings/model.rs` are either deleted (if under 2000 L) or reduced to their new line counts
- Full workspace CI green; specs pass unmodified

**Rollback:** `git restore crates/jackin-console/src/tui/ crates/jackin/src/console/ crates/jackin-tui/src/runtime.rs file-size-budget.toml` — or, if this is a PR branch, `git checkout main` and delete the branch.

**Open questions:**
1. What are the exact trait names and signatures added to `crates/jackin-tui/src/runtime.rs` by G0? (G0 has not shipped; G3 is blocked until this is known.)
2. Does the unified modal type stay as `ConsoleModal` in `app.rs`, or does a new `ConfigModal` type get introduced in a shared location? The design doc says "leaning `jackin-console`, beside the shared widgets" but this is not locked.
3. Do the per-panel modal type params on `GlobalMountsState`, `SettingsEnvState`, `SettingsAuthState` get removed entirely (replaced by one `modal: Option<ConsoleModal>` on `SettingsState`), or do the per-panel slots stay but change type to `ConsoleModal`?
4. How is the trust asymmetry reconciled: promote editor to a `Trust` tab matching `SettingsTab::Trust`, or keep editor's `ConfirmTarget::TrustRoleSource` flow and add the same pattern to settings?
5. What are the exact file-size boundaries for splitting `app.rs` (5095 L) after the modal collapse — what clusters move to sibling files and what stays as coordinator?
6. Which portions of `crates/jackin/src/console/tui/run.rs` (827 L) are absorbed by the G0 shared runtime loop vs which remain as console-specific glue?
7. The auth mouse-row-click asymmetry (`editor.auth_expanded` set + row-click vs settings `selected_kind` detail-rows with no row-click) — is this an intentional per-surface affordance to preserve, or a behavior gap to reconcile in G3?

---

### W5-usage — Decompose jackin-capsule usage.rs into provider modules

> **Not yet detailed.** The detailing agent for this slice failed mid-run. Re-detail it before executing (re-run the codebase-health-playbook workflow). The strategy page describes the intended shape; do **not** improvise from the summary.

---

### W5-console — Decompose console tui/app.rs + editor/settings model.rs

- **Goal:** Shrink three over-cap files in `jackin-console` by extracting cohesive clusters to sibling modules, bringing every production file under the 2000-line ratchet cap and relocating inline test blocks to `tests.rs` siblings per the test-layout rule.
- **Preconditions:** none for Phase A (app.rs); Phase B (editor/settings) requires unify-settings (roadmap G3) to be merged first — see Open questions.
- **Pattern:** file-split (coordinator + sibling files, proven by the existing `tui/input/editor.rs` split)
- **Touches:** `crates/jackin-console/src/tui/app.rs` (split, Phase A), `crates/jackin-console/src/tui/app/launch_prompt.rs` (create, Phase A), `crates/jackin-console/src/tui/app/manager_stage.rs` (create, Phase A), `crates/jackin-console/src/tui/app/modal.rs` (create, Phase A), `crates/jackin-console/src/tui/app/create_prelude.rs` (create, Phase A), `crates/jackin-console/src/tui/app/tests.rs` (create, Phase A); `file-size-budget.toml` (update); `test-layout-allowlist.toml` (update). Phase B touches `screens/editor/model.rs`, `screens/settings/model.rs` and their new sibling dirs — **deferred to post-G3**.

---

## Phase A — Split `tui/app.rs` (5095 lines) into coordinator + four siblings

**Current cluster map** (exact line ranges in the 5095-line file):

| Lines | Cluster | Destination |
|---|---|---|
| 1–24 | File header + `use` imports | Replaced by coordinator header |
| 27–31 | `ConsoleAppStage` enum | Stays in coordinator |
| 33–251 | Launch-prompt traits, plan fns, `ConsoleApp` impls | `app/launch_prompt.rs` |
| 253–682 | `ConsoleManagerStage`, dispatch types, traits, fn, `ConsoleManagerStage` impls | `app/manager_stage.rs` |
| 683–2321 | `ConsoleModal` enum + all impl blocks + private `footer_items_for_mode` fn | `app/modal.rs` |
| 2323–2336 | `ConsoleApp` struct | Stays in coordinator |
| 2338–2725 | `ConsoleCreatePreludeState` struct + `impl` blocks + `CreatePrelude*` plan types and fns | `app/create_prelude.rs` |
| 2727–2789 | `ConsoleApp` simple `impl` blocks (`new`, `quit_confirm_*`, `base_surface_unblocked`) | Stays in coordinator |
| 2791–5095 | Inline `#[cfg(test)] mod tests { … }` body | `app/tests.rs` |

**Steps** (each step is one mechanical action — exact paths, exact symbol names, exact edit):

1. Create directory `crates/jackin-console/src/tui/app/` (it does not exist yet).

2. Create `crates/jackin-console/src/tui/app/launch_prompt.rs` with the content of **lines 33–251** of the current `app.rs`, prepended with the imports those lines need:
   ```rust
   //! Launch-prompt traits, plan functions, and their `ConsoleApp` impls.

   use super::{ConsoleApp, ConsoleAppStage};

   // All `crate::tui::components::*` paths used in those lines stay as-is
   // (they are already written as `crate::tui::components::agent_choice::AgentChoice`
   //  etc. in the source — no import rewriting required).
   ```
   The symbols this file defines (all must remain `pub`):
   `LaunchAgentPromptManagerState`, `LaunchAgentPromptState`, `LaunchRolePromptManagerState`, `LaunchProviderPickerManagerState`, `LaunchRolePromptState`, `LaunchProviderPickerState`, `open_launch_agent_prompt_plan`, `open_launch_role_prompt_plan`, `clear_pending_launch_plan`, `clear_pending_launch_role_plan`, `take_pending_launch_plan`, `take_pending_launch_and_role_plan`, `store_pending_launch_plan`, `open_launch_provider_picker_plan`.

3. Create `crates/jackin-console/src/tui/app/manager_stage.rs` with the content of **lines 253–682** of the current `app.rs`, prepended with:
   ```rust
   //! ConsoleManagerStage and all dispatch/trait definitions.

   use crate::tui::debug::{
       ConsoleCreatePreludeDebugFacts, ConsoleEditorDebugFacts, ConsoleSettingsDebugFacts,
       ConsoleStageDebug,
   };
   use crate::tui::view::StageFooterHeightFacts;
   ```
   The symbols this file defines (all remain `pub`):
   `ConsoleManagerStage`, `ConsoleManagerStageRoute`, `ConsoleManagerStageState`, `ConsoleInputDispatchPlan`, `ConsoleInputDispatchFacts`, `ConsoleStageModalFacts`, `ConsoleEditorModalPresence`, `ConsoleEditorFooterHeight`, `ConsoleSettingsModalPresence`, `ConsoleSettingsFooterHeight`, `ConsolePendingTokenGenerate`, `ConsolePendingRoleLoad`, `ConsolePendingDriftCheck`, `ConsolePendingIsolationCleanup`, `ConsolePendingOpCommit`, `ConsolePendingOpCommitOrigin`, `ConsolePendingOpCommitResolution`, `ConsoleAnimationTick`, `ConsoleCreatePreludeModalPresence`, `ConsoleManagerModalBlockPresence`, `apply_manager_stage`, `console_input_dispatch_plan`.

4. Create `crates/jackin-console/src/tui/app/modal.rs` with the content of **lines 683–2321** of the current `app.rs` (includes the `ConsoleModal` enum, all its impl blocks, and the private `footer_items_for_mode` fn), prepended with:
   ```rust
   //! ConsoleModal enum and all trait implementations.

   use std::path::PathBuf;

   use ratatui::layout::Rect;

   use super::ConsoleAnimationTick;
   use crate::tui::components::footer_hints::{
       ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
       ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
   };
   use crate::tui::components::modal_rects::{
       ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
       ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
       ModalRectMode, ModalRolePickerState,
   };
   use crate::tui::debug::ConsoleModalDebugKind;
   use crate::tui::screens::editor::model::{
       CreateStep, EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
       EditorStatusPopupModal,
   };
   ```
   (`use super::ConsoleAnimationTick` resolves via coordinator re-export of `manager_stage::ConsoleAnimationTick`.)
   Remove the top-level `use crate::tui::debug::ConsoleModalDebugKind` line that exists in `app.rs` (line 17 in original) since `modal.rs` declares its own; the coordinator will no longer import this.
   The symbol this file defines (remains `pub`): `ConsoleModal`.

5. Create `crates/jackin-console/src/tui/app/create_prelude.rs` with the content of **lines 2338–2725** of the current `app.rs`, prepended with:
   ```rust
   //! ConsoleCreatePreludeState and CreatePrelude plan types and functions.

   use std::path::PathBuf;

   use super::{ConsoleCreatePreludeModalPresence};
   use crate::tui::debug::{ConsoleCreatePreludeDebugFacts, ConsoleModalDebugKind, ConsoleStageDebug};
   use crate::tui::screens::editor::model::{CreateStep, FileBrowserTarget, TextInputTarget};
   ```
   (`ConsoleCreatePreludeModalPresence` resolves via coordinator re-export of `manager_stage::ConsoleCreatePreludeModalPresence`.)
   The symbols this file defines (all remain `pub`):
   `ConsoleCreatePreludeState`, `CreatePreludeCompletionStatus`, `CreatePreludeKeyPlan`, `CreatePreludeModalStep`, `CreatePreludeFileBrowserPlan`, `CreatePreludeWorkdirCancelPlan`, `CreatePreludeWorkdirPickPlan`, `CreatePreludeMountDstChoicePlan`, `CreatePreludeTextInputDstPlan`, `CreatePreludeTextInputNamePlan`, `CreatePreludeFileBrowserTarget`, `CreatePreludeTextInputTarget`, `create_prelude_modal_step`, `create_prelude_text_input_dst_plan`, `create_prelude_text_input_name_plan`, `create_prelude_workdir_pick_plan`, `create_prelude_file_browser_plan`, `create_prelude_mount_dst_choice_plan`, `create_prelude_workdir_cancel_plan`, `create_prelude_key_plan`, `create_prelude_completion_status`.

6. Create `crates/jackin-console/src/tui/app/tests.rs` by extracting the **body** of the inline `#[cfg(test)] mod tests { … }` block (lines 2793–5094 inclusive — the content between the outer braces, not including the `mod tests {` and closing `}`). The file begins directly with the imports and helper structs. The existing `use super::{ … }` import in the test block remains unchanged; `super::` will now resolve to the coordinator `app.rs` which re-exports all needed symbols.

7. Replace the entire content of `crates/jackin-console/src/tui/app.rs` with the new coordinator:
   ```rust
   //! Top-level console TUI app model.

   pub(super) mod launch_prompt;
   pub(super) mod manager_stage;
   pub(super) mod modal;
   pub(super) mod create_prelude;
   #[cfg(test)]
   mod tests;

   pub use self::launch_prompt::{
       LaunchAgentPromptManagerState, LaunchAgentPromptState, LaunchRolePromptManagerState,
       LaunchProviderPickerManagerState, LaunchRolePromptState, LaunchProviderPickerState,
       open_launch_agent_prompt_plan, open_launch_role_prompt_plan, clear_pending_launch_plan,
       clear_pending_launch_role_plan, take_pending_launch_plan, take_pending_launch_and_role_plan,
       store_pending_launch_plan, open_launch_provider_picker_plan,
   };

   pub use self::manager_stage::{
       ConsoleManagerStage, ConsoleManagerStageRoute, ConsoleManagerStageState,
       ConsoleInputDispatchPlan, ConsoleInputDispatchFacts, ConsoleStageModalFacts,
       ConsolePendingOpCommitOrigin, ConsolePendingOpCommitResolution,
       ConsoleEditorModalPresence, ConsoleEditorFooterHeight,
       ConsoleSettingsModalPresence, ConsoleSettingsFooterHeight,
       ConsolePendingTokenGenerate, ConsolePendingRoleLoad, ConsolePendingDriftCheck,
       ConsolePendingIsolationCleanup, ConsolePendingOpCommit,
       ConsoleAnimationTick, ConsoleCreatePreludeModalPresence, ConsoleManagerModalBlockPresence,
       apply_manager_stage, console_input_dispatch_plan,
   };

   pub use self::modal::ConsoleModal;

   pub use self::create_prelude::{
       ConsoleCreatePreludeState,
       CreatePreludeCompletionStatus, CreatePreludeKeyPlan, CreatePreludeModalStep,
       CreatePreludeFileBrowserPlan, CreatePreludeWorkdirCancelPlan, CreatePreludeWorkdirPickPlan,
       CreatePreludeMountDstChoicePlan, CreatePreludeTextInputDstPlan,
       CreatePreludeTextInputNamePlan, CreatePreludeFileBrowserTarget, CreatePreludeTextInputTarget,
       create_prelude_modal_step, create_prelude_text_input_dst_plan,
       create_prelude_text_input_name_plan, create_prelude_workdir_pick_plan,
       create_prelude_file_browser_plan, create_prelude_mount_dst_choice_plan,
       create_prelude_workdir_cancel_plan, create_prelude_key_plan, create_prelude_completion_status,
   };

   /// Single-variant today; kept as `enum` so future stages can land without
   /// churning every match site.
   #[derive(Debug)]
   #[allow(clippy::large_enum_variant)]
   pub enum ConsoleAppStage<Manager> {
       Manager(Manager),
   }

   // ConsoleApp struct (lines 2323–2336 of original app.rs) — paste verbatim.
   // ConsoleApp impl blocks (lines 2727–2789 of original app.rs) — paste verbatim.
   // These impl blocks reference `ConsoleManagerModalBlockPresence` which is
   // available as a re-export from `manager_stage` above.
   ```
   No caller files need editing: all `crate::tui::app::X` import paths continue to resolve through this coordinator's `pub use` re-exports.

8. In `file-size-budget.toml`: delete the three entries for `crates/jackin-console/src/tui/app.rs` (lines = 5095). The coordinator after the split is ~175 lines — under the 2000-line production cap, so no entry is needed. Alternatively run `cargo xtask lint files --print-budget` after the split and paste the refreshed budget.

9. In `test-layout-allowlist.toml`: delete the entry `"crates/jackin-console/src/tui/app.rs"` — the inline test block is gone, replaced by `app/tests.rs`.

---

## Phase B — Split `screens/editor/model.rs` and `screens/settings/model.rs` (DEFERRED — precondition: unify-settings G3)

> **Sequencing constraint (from roadmap W5):** "decompose with G3 / unify-settings (split the unified surface, not the duplicated pair)." Do NOT execute the steps below until after the unify-settings PR (roadmap G3) merges `editor/model.rs` and `settings/model.rs` into a single unified surface. The boundaries below describe the CURRENT per-file structure for planning purposes only.

#### Current `screens/editor/model.rs` boundaries (4176 lines)

| Lines | Content | Proposed destination (post-G3) |
|---|---|---|
| 1–5 | File header | Coordinator header |
| 7–10 | `use` imports | Each sibling declares its own |
| 13–239 | Key-plan enums, plan fn, modal traits: `EditorTab`, `EditorFocusTarget`, `EditorHoverTarget`, `RoleHeaderExpansionPlan`, `EditorRoleHeaderExpansionKeyPlan`, `AuthEnterPlan`, `EditorEnterKeyPlan`, `EditorEscapeKeyPlan`, `EditorSaveKeyPlan`, `EditorMountGithubOpenPlan`, `EditorHorizontalScrollKeyPlan`, `EditorFieldSelectionKeyPlan`, `EditorNavigationKeyPlan`, `EditorTopLevelKeyPlan`, `EditorImmediateActionKeyPlan`, `EditorRoleActionKeyPlan`, `EditorMountActionKeyPlan`, `EditorSecretsActionKeyPlan`, `EditorAuthActionKeyPlan`, `EditorTabActionKeyPlan`, `EditorMode`, `EditorSaveModePlan`, `editor_save_mode_plan`, `EditorStatusPopupModal`, `EditorRoleOverridePickerModal`, `EditorSaveDiscardModal`, `EditorErrorPopupModal` | `model/key_plans.rs` (~239 L) |
| 241–2136 | `EditorState` struct + all its `impl` blocks | `model/state.rs` (~1896 L) |
| 2137–2291 | Row model types: `FieldFocus`, `SecretsScopeTag`, `SecretsRow`, `SecretsEnterPlan`, `AuthRow`, `PendingSaveCommit`, `EditorSaveFlow`, `ConfirmTarget`, `TextInputTarget`, `FileBrowserTarget`, `ExitIntent`, `CreateStep` | `model/rows.rs` (~155 L) |
| 2292–4176 | Inline `#[cfg(test)] mod tests` body | `model/tests.rs` (~1885 L) |
| — | Coordinator | `model.rs` (~60 L with `mod` decls + explicit `pub use` re-exports) |

Note: `model/state.rs` at ~1896 L is near the 2000 L cap. TODO(investigate): verify that `EditorState` impl blocks cannot be sub-split across `state/general_impls.rs` etc. without creating logic cross-dependencies; if the file lands at 1896 L it is still under cap.

#### Current `screens/settings/model.rs` boundaries (3852 lines)

| Lines | Content | Proposed destination (post-G3) |
|---|---|---|
| 1–5 | File header | Coordinator header |
| 7–17 | `use` imports | Each sibling declares its own |
| 21–475 | `SettingsTab` + `SettingsState` struct + all its `impl` blocks + `SettingsAfterEventOutcome` + `SettingsHoverTarget` + panel traits (`SettingsPanelDirty`, `SettingsPanelChangeCount`, `SettingsPanelDiscard`, `SettingsPanelMarkSaved`) + `SettingsPanelTakeError`, `SettingsAuthRestorePendingForm`, `SettingsMountsTakeExit`, `SettingsModalSlot`, `SettingsAuthModalSlot` | `model/state.rs` (~455 L) |
| 494–705 | Auth-form types + `SettingsAuthModal` enum + its impls + private `footer_items_for_mode` fn: `AuthFormFocus`, `AuthFormTarget`, `SettingsAuthRow`, `SettingsEnvScope`, `SettingsEnvConfirm`, `SettingsEnvTextTarget`, `SettingsEnvEnterPlan`, `SettingsEnvRow`, `SettingsAuthModal`, `AuthFormTarget` impl | `model/auth_form.rs` (~212 L) |
| 707–1051 | `SettingsEnvConfig` + fn + `SettingsEnvState` + `SettingsEnvSaveRefs` + all `SettingsEnvState` impls | `model/env.rs` (~345 L) |
| 1053–1604 | `GlobalMountConfirm` + `GlobalMountTextTarget` + `GlobalMountModal` + its impls + `GlobalMountsState` + `GlobalMountsSaveRefs` + impls + `GlobalMountDraft` | `model/mounts.rs` (~552 L) |
| 1613–2257 | `SettingsAuthModal` (re-check line range) + `SettingsAuthState` + `SettingsAuthSaveRefs` + all `SettingsAuthState` impls | `model/auth.rs` (~645 L) |
| 2258–2353 | `SettingsGeneralState` + `SettingsGeneralSaveRefs` + `SettingsGeneralState` impls | `model/general.rs` (~96 L) |
| 2355–3852 | Inline `#[cfg(test)] mod tests` body | `model/tests.rs` (~1498 L) |
| — | Coordinator | `model.rs` (~60 L with `mod` decls + explicit `pub use` re-exports) |

TODO(investigate): After G3 merges the two model files into one unified surface, the above two tables become obsolete; the merged file's cluster boundaries must be re-derived from the actual merged content before executing Phase B steps.

---

**Steps** (Phase B, template only — do NOT execute until G3 merges):

B1. After G3 merge, derive the exact cluster map from the unified `editor/model.rs` (settings model will be folded in or eliminated by G3).
B2. Create `screens/editor/model/` directory.
B3. Create each sibling `*.rs` file with the appropriate cluster content from the unified file, each prepended with a `//!` doc comment and the exact `use` imports those lines require.
B4. Replace `screens/editor/model.rs` with the coordinator: `mod` declarations + explicit `pub use` re-exports for every public symbol.
B5. Create `screens/editor/model/tests.rs` from the extracted inline test body.
B6. Delete entries for `screens/editor/model.rs` and `screens/settings/model.rs` from `file-size-budget.toml`; run `cargo xtask lint files --print-budget` to refresh.
B7. Delete entries for `screens/editor/model.rs` and `screens/settings/model.rs` from `test-layout-allowlist.toml`.

---

**Verify** (run in order; STOP and revert on the first failure):

- `cargo fmt --check` → no diff
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → no warnings
- `cargo nextest run -p jackin-console` → all pass
- `cargo run -p jackin-xtask --locked -- lint` → file-size + test-layout OK (after budget/allowlist edits)
- `cargo run -p jackin-xtask --locked -- lint files --print-budget` → paste output, delete the `app.rs` entry from budget if it appears
- behavioral specs `runtime-launch` and `op-picker` pass UNMODIFIED

**Done when:**
- Phase A: `crates/jackin-console/src/tui/app.rs` is ≤ 200 lines (coordinator only); four sibling files exist each under 2000 lines; `app/tests.rs` exists; no inline `#[cfg(test)] mod tests` remains in `app.rs`; `cargo nextest run -p jackin-console` is green; `app.rs` is removed from both `file-size-budget.toml` and `test-layout-allowlist.toml`.
- Phase B: deferred to post-G3.

**Rollback:** `git restore crates/jackin-console/src/tui/app.rs file-size-budget.toml test-layout-allowlist.toml && git rm -r crates/jackin-console/src/tui/app/`

**Open questions:**
1. The exact cluster boundaries for Phase B (editor + settings model splits) cannot be determined until G3 (unify-settings) merges; executor must re-derive them from the actual post-G3 unified file before writing any Phase B step. Do not attempt to split the current duplicated pair — the roadmap explicitly says to split the unified surface.
2. `modal.rs` imports: the exact set of `use crate::tui::auth_config::*` and `use crate::tui::update::*` items needed by `ConsoleModal`'s impl blocks should be derived from compiler errors after the mechanical move (the original `app.rs` used `crate::tui::auth_config::X` inline paths, not top-level imports, so `modal.rs` may need additional explicit `use` lines that are not listed above).
3. `manager_stage.rs` line 682 references `ConsoleStageDebug` from `crate::tui::debug`. Confirm `crate::tui::debug::ConsoleStageDebug` is still the correct import path (not moved by any prior slice).
4. The `#[allow(clippy::large_enum_variant)]` attributes on `ConsoleManagerStage` (line 254) and `ConsoleModal` (line 760) must be preserved verbatim when their definitions move to their respective sibling files.
