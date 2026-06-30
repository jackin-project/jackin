# Codebase health — remaining work to close the roadmap item

Executor-grade worklist of **everything still open** under
[Codebase health: structure & reviewability](/roadmap/codebase-health-enforcement/).
Written so an agent can run one slice with **zero improvisation**: exact files, exact
`file:line`, exact symbols, exact repoints, exact commands, exact done-when.

This file is the live worklist. Slices A1–G3 of the original execution playbook have all
shipped (their per-slice log lives in git history); only the open set below remains.

**Verification basis.** Every fact below was checked against the live tree (branch
`refactor/codebase-health-decomposition`, HEAD at writing), **not** against roadmap
checkboxes. Line numbers were accurate at investigation time — an executor must
re-confirm each against the file before editing (any prior landed slice shifts them);
where a step says "verify before editing," that is mandatory, not optional.

**The invariant (non-negotiable).** **Structure only — never behavior.** No logic,
control-flow, signature, or performance change. The existing test suite + the
`runtime-launch` / `op-picker` behavioral specs must pass **unmodified**. A move that
forces a test edit changed behavior → **stop, back out, report.**

---

## Executor contract (read once, obey every slice)

1. **One slice = one PR.** Do exactly the slice; do not bundle the next one.
2. **Structure only.** See above. If a step seems to need a test edit to pass — stop.
3. **Respect preconditions.** R2 needs R1. R5 needs the external unify-settings item.
   R7 needs R4 + R6. Confirm prerequisites are merged before starting.
4. **Run Verify in order; stop on first red.** Never force past a gate.
5. **Do not guess.** Re-confirm line numbers; if code doesn't match the step, stop and report.
6. **Conventions:** no `mod.rs`; tests in a sibling `tests.rs`; every crate
   `[lints] workspace = true`; **no wildcard imports** (`clippy::wildcard_imports` denied).
7. **Commit:** Conventional Commit `refactor(<scope>): <slice>`, sign off (`-s`), push immediately.

### Standard Verify (every slice unless overridden)

```
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --all-features
cargo run -p jackin-xtask --locked -- lint        # file-size + test-layout + arch
# behavioral specs runtime-launch + op-picker pass UNMODIFIED
```
After a file drops under its cap, refresh the ratchet and prune the entry:
`cargo run -p jackin-xtask --locked -- lint files --print-budget` (over `file-size-budget.toml`)
/ `lint tests --print-allowlist` (over `test-layout-allowlist.toml`).

### Crate-carve recipe (referenced by R3)

1. Target crate exists or create `crates/<new>/` (`[lints] workspace = true` + `//!`
   Architecture-Invariant header). 2. `git mv` modules verbatim (byte-identical bodies).
3. Visibility-only: public surface `pub`, rest `pub(crate)` — no signature/logic edits.
4. Repoint importers (Parallel Change: add new path, migrate, delete old). 5. Wire
   `Cargo.toml` members/deps. 6. Add arch-gate / `cargo-deny` entries for new edges.
7. Update `PROJECT_STRUCTURE.md` + Codebase Map. 8. Verify; hot-path carve → E0 benchmark.

---

## Definition of done (the item closes when ALL hold)

1. R1 breaks `jackin-runtime → jackin-tui`; R2 runs the arch gate `--strict` in CI.
2. R3 finishes the isolation carve (E1 complete).
3. R4 + R5 clear the file-size backlog (no production `.rs` over 2000L).
4. R6 burns down the 58 clippy grandfathers (zero remain).
5. **Then** R7 tightens thresholds to target (files ≤ 1500L, fns ≤ 150 lines).
6. R8–R10 (hygiene/decision) land alongside; R11 bookkeeping done.

---

## Snapshot — DONE (no action; listed so the open set is unambiguous)

A0–A5 boundary fixes + `cargo-deny` hygiene + 19/19 Architecture-Invariant headers
(`FORBIDDEN_EDGES` 3 → 1) · B1 rename · B2 19 shims deleted · C1/C2 + E2 carves
(`jackin-host`/`jackin-usage`/`jackin-instance`) · D1–D4 dedups · E0 `lto="thin"` +
baseline · E1 **partial** (4 of 6 isolation modules; **finalize/git_inspect open → R3**)
· F1/F2 naming · G0–G3 shared TUI runtime + all four stacks migrated · W5 `usage.rs` +
`tui/model.rs` decomposed · W2 clippy thresholds + W3 file-size gate + W4 arch/deny/shear trio.

---

# The open set

---

## R1 — Break the last inverted dependency: `jackin-runtime → jackin-tui`  ⟵ gate-keeper

**Goal.** Remove every **production** use of `jackin_tui::*` from `jackin-runtime` so the
L1→L3 edge disappears. E0 LTO baseline shipped → the perf gate that parked this is lifted.

**Verified scope correction.** Only **8 production call sites** (not 11). The
`components::*State` structs + `theme::DANGER_RED` in `progress.rs` are **`#[cfg(test)]`-only**
— they need no relocation; they fall out when `jackin-tui` moves to `[dev-dependencies]`.

**Allowed-edge fact.** `jackin-runtime → jackin-launch-tui` is **permitted** (the arch gate
bans only `jackin-runtime → jackin-tui`). So the port impl lives in `jackin-launch-tui` and
runtime injects it directly — **mirror the existing `BuildLogSink`** (injected from
`runtime/image.rs:1306,1639` via `Arc::new(DiagnosticsBuildLogSink)`) and the existing
self-owned `host_terminal()` accessor at `progress.rs:132`.

### The 8 production sites + exact fix

| # | site (verify before edit) | symbol | fix |
|---|---|---|---|
| 1 | `runtime/host_attach.rs:272` | `jackin_tui::url_text::redact_url_for_log(&url)` | **swap** → `jackin_core::redact_url_for_log(&url)`. Symbol already exists: `jackin-core/src/url_text.rs:29` `pub fn redact_url_for_log(url:&str)->String`, re-exported `jackin-core/src/lib.rs:85`. Identical signature, zero behavior change. |
| 2 | `runtime/progress.rs:87` | `jackin_tui::ansi::POINTER_HAND` | **relocate const to `jackin-core`** then repoint. Source: `jackin-tui/src/lib.rs:269` `pub const POINTER_HAND:&str="\x1b]22;pointer\x1b\\";` (no ratatui). |
| 3 | `runtime/progress.rs:89` | `jackin_tui::ansi::POINTER_DEFAULT` | relocate `jackin-tui/src/lib.rs:270` `"\x1b]22;default\x1b\\"` → core; repoint. |
| 4 | `runtime/progress.rs:98` | `jackin_tui::ansi::encode_osc52_clipboard_write(payload)` | relocate `jackin-tui/src/lib.rs:465` `pub fn encode_osc52_clipboard_write(payload:&str)->Vec<u8>` (body: BASE64 + `\x1b]52;c;`…`\x07`, no ratatui) → core; repoint. |
| 5 | `runtime/launch.rs:735` | `jackin_tui::output::print_deploying(name).await` | **port** (prints directly; async). |
| 6 | `runtime/launch.rs:2093` | `jackin_tui::animation::warp_out(host_owned)` | **port** — `animation.rs:295`; calls `warp()` which uses `crossterm::terminal::size()` → **must stay in jackin-tui**. |
| 7 | `runtime/launch.rs:2094` | `jackin_tui::animation::warp_end_caption(elapsed, host_owned)` | **port** — `animation.rs:302`, crossterm-backed → stay in jackin-tui. |
| 8 | `runtime/launch.rs:2809,2812,2815,2818,2824` (5 calls) | `jackin_tui::output::step_fail(&format!(...))` | **port** — `output.rs:35` owo_colors stderr helper. |

### Steps

1. **Quick win (site 1):** repoint `host_attach.rs:272` to `jackin_core::redact_url_for_log`; drop the `jackin_tui::url_text` reference. Build.
2. **Relocate the pure ansi items (sites 2–4)** into `jackin-core` (e.g. a new
   `jackin-core/src/ansi_tokens.rs`, `pub mod` + root re-export). Leave a
   `pub use jackin_core::… as …` re-export in `jackin-tui`'s `ansi` so jackin-tui's own
   callers compile unchanged (Parallel Change). Repoint `progress.rs:87,89,98` to the core path.
3. **Define the launch-output port** in `jackin-core/src/launch_progress.rs` (beside the
   existing `LaunchDiagnostics`/`LaunchHostTerminal` at `:227`/`:235`):
   ```rust
   pub trait LaunchOutputSink: Send + Sync {
       fn print_deploying<'a>(&'a self, role_name: &'a str)
           -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>; // async via boxed future
       fn step_fail(&self, msg: &str);
       fn warp_out(&self, host_screen_owned: bool);
       fn warp_end_caption(&self, elapsed: Option<std::time::Duration>, host_screen_owned: bool);
   }
   ```
   Re-export from `jackin-core/src/lib.rs` (mirror `:66-67`).
4. **Impl the port in `jackin-launch-tui`** (it may depend on `jackin-tui`): a unit struct
   whose methods call `jackin_tui::output::{print_deploying,step_fail}` /
   `jackin_tui::animation::{warp_out,warp_end_caption}`. Re-export it from the crate root
   (mirror `jackin-launch-tui/src/build_log.rs:14` `DiagnosticsBuildLogSink`).
5. **Consume in runtime via a self-owned accessor** — mirror `progress.rs:132`
   `host_terminal() -> &'static dyn LaunchHostTerminal`. Add a `launch_output() ->
   &'static dyn LaunchOutputSink` backed by a `static` of the `jackin-launch-tui` impl, and
   rewrite `launch.rs:735,2093,2094,2809–2824` to call through it. **No `jackin/src/app/load_cmd.rs`
   edit needed** — runtime owns the accessor, exactly as it owns `host_terminal()`.
6. **Move the dep:** in `crates/jackin-runtime/Cargo.toml` delete line 30
   `jackin-tui = { … path = "../jackin-tui" }` from `[dependencies]`. The `#[cfg(test)]`
   refs at `progress.rs:48-52` + `progress/tests.rs` still need jackin-tui → add
   `jackin-tui = { path = "../jackin-tui" }` under `[dev-dependencies]` (minimal change;
   keeps tests compiling, removes the production-dependency inversion). Also fix the
   `lib.rs:15-16` doc comment that names the removed `jackin_tui::*` uses.

### Verify

Standard Verify **+** `cargo run -p jackin-xtask --locked -- lint arch --strict` passes
(edge gone). Launch cockpit / progress / clipboard / warp-out render identically; run the
`runtime-launch` spec. Re-run the E0 launch/attach benchmark (hot path) — no regression.

### Done-when

`grep -rn 'jackin_tui' crates/jackin-runtime/src --include='*.rs'` returns **zero**
production hits (test-only refs may remain, served by the dev-dep); `jackin-tui` is gone
from `jackin-runtime/Cargo.toml [dependencies]`.

---

## R2 — Flip the dependency-direction gate to `--strict` in CI

**Goal.** Make the architecture machine-enforced once R1 lands. **Precondition: R1 merged.**

**Verified facts.** `FORBIDDEN_EDGES` (`crates/jackin-xtask/src/arch.rs:48`) holds the
single entry `("jackin-runtime","jackin-tui")`. The arch synthetic test
`crates/jackin-xtask/src/arch/tests.rs:13` (`synthetic_graph_flags_only_listed_forbidden_edges`)
asserts at `:37` `assert_eq!(problems, vec!["jackin-runtime → jackin-tui"]);`; a clean-graph
test exists at `:41`. CI runs the gate **non-strict**: `.github/workflows/ci.yml:441`
`cargo run -p jackin-xtask --locked -- lint` (comment at `:438` flags strict as the eventual flip).

### Steps

1. Set `FORBIDDEN_EDGES = &[]` in `arch.rs:48` (drop the runtime→tui tuple).
2. Update `arch/tests.rs`: with the list empty, `synthetic_graph_flags_only_listed_forbidden_edges`
   has nothing to inject — adjust its assertion to expect no problems (or fold it into the
   existing clean-graph test). Read the test body (`:13-63`) before editing; keep it meaningful.
3. Change `.github/workflows/ci.yml:441` to `cargo run -p jackin-xtask --locked -- lint --strict`.
4. Update the `arch.rs:40-56` doc comment (it narrates the now-removed edge).

### Verify

Standard Verify; `cargo xtask lint --strict` green. Sanity: locally add a throwaway
forbidden edge → confirm the gate now **fails** → revert.

### Done-when

CI invocation carries `--strict`; `FORBIDDEN_EDGES` empty; no production inversion remains.

---

## R3 — Finish E1: move `finalize.rs` + `git_inspect.rs` into `jackin-isolation`

**Goal.** Complete the isolation carve so `jackin-runtime` owns no isolation code.
**Verified: NO blockers** — every production dep of both files already lives in
`jackin-core`/`jackin-config`/`jackin-diagnostics` (the three crates `jackin-isolation`
already depends on). (Correction to an earlier draft: `parse_session_count`/`JACKIN_STATUS_CMD`
are **already in `jackin-core`** — `jackin-core/src/status.rs:15,21`, re-export `lib.rs:80` —
and `finalize.rs` already consumes them from there; nothing to relocate.)

### Files (verify with `ls`/`wc -l` first)

```
crates/jackin-runtime/src/isolation/finalize.rs          586  MOVE
crates/jackin-runtime/src/isolation/finalize/tests.rs   1683  MOVE
crates/jackin-runtime/src/isolation/git_inspect.rs        87  MOVE
crates/jackin-runtime/src/isolation/git_inspect/tests.rs  79  MOVE
crates/jackin-runtime/src/isolation/tests.rs              50  DO NOT MOVE (tests MountIsolation via super::*)
```

### Steps

1. `git mv` the four files into `crates/jackin-isolation/src/` (preserve the
   `finalize/tests.rs`, `git_inspect/tests.rs` subdir layout). Leave `isolation/tests.rs`.
2. Add to `crates/jackin-isolation/src/lib.rs`: `pub mod finalize;` `pub mod git_inspect;`.
   Update the stale doc-comment (lib.rs ~`:19-26`) that says they "remain under jackin_runtime::isolation."
3. In moved `finalize.rs` repoint (3+ sites): `crate::isolation::cleanup`→`crate::cleanup`;
   `crate::isolation::state`→`crate::state`; `crate::isolation::git_inspect`→`crate::git_inspect`
   (body `:212`); `crate::instance::naming::instance_id_from_container_base`→
   `jackin_core::constants::instance_id_from_container_base` at **`:417,454,475`**.
   `git_inspect.rs` needs **no** path edits (self-contained on jackin-core + std).
4. In moved `finalize/tests.rs`: `crate::runtime::test_support::{FakeRunner,FakeDockerClient}`
   → `jackin_runtime::runtime::test_support::{…}` (mirrors existing
   `jackin-isolation/src/cleanup/tests.rs:5` + `materialize/tests.rs:189`);
   `crate::isolation::MountIsolation`→`crate::MountIsolation`; `crate::isolation::state`→`crate::state`.
   `git_inspect/tests.rs` needs no edits (uses `super::*`).
5. In `crates/jackin-runtime/src/isolation.rs` replace bare `pub mod finalize;` /
   `pub mod git_inspect;` (`:21-22`) with re-export shims:
   `pub mod finalize { pub use jackin_isolation::finalize::*; }` (same for git_inspect) — so
   all ~30 `crate::isolation::finalize::*` call sites across `runtime/{attach,launch/restore,
   launch/launch_pipeline,launch,apple_container}.rs` compile **unchanged**.
6. **No Cargo.toml dependency edits** — the dev-dep cycle (`jackin-isolation`
   `[dev-dependencies] jackin-runtime{features=["test-support"]}`) + `test-support` feature
   are already present on both sides.

### Verify

Standard Verify + `cargo nextest run -p jackin-isolation -p jackin-runtime`; E0 benchmark
(isolation is launch hot path) — no regression.

### Done-when

`crates/jackin-runtime/src/isolation/finalize.rs` + `git_inspect.rs` gone;
`grep -rn 'mod finalize\|mod git_inspect' crates/jackin-runtime/src/isolation.rs` shows only
the re-export shims.

---

## R4 — W5 production decompositions (every prod `.rs` under 2000L)

**Goal.** Clear the file-size grandfather list. Split pattern: keep the file as the
**coordinator** (public surface, constants, dispatch); move each cohesive cluster to a
sibling `<module>/<name>.rs`; child reads ancestor privates via explicit `use super::…`
(**no wildcard**); items the coordinator/siblings call get `pub(super)`; tests in
`<module>/tests.rs`; no `mod.rs`. One file = one PR; refresh the ratchet after each.

**Over-cap production files (verified `wc -l`):**

- [ ] `crates/jackin-runtime/src/runtime/launch.rs` — **2834L** (siblings go under the
      existing `runtime/launch/` dir, beside `launch_pipeline.rs`).
- [ ] `crates/jackin-console/src/tui/screens/editor/view.rs` — **2389L**.
- [ ] `crates/jackin-capsule/src/tui/components/dialog.rs` — **2265L**.
- [ ] `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` — **2213L**.
- [ ] **Bookkeeping:** prune the `runtime/image.rs` `[[production]]` block from
      `file-size-budget.toml` — it is **1952L < 2000 cap** (stale grandfather; ratchet rule
      says delete when under cap). No code move.

> **Concrete per-file split-maps (which fn/struct → which sibling) are being finalized by a
> dedicated investigation and will be appended here before execution.** Until then, an
> executor must read the target file and derive cluster boundaries using the split pattern
> above — do **not** start a R4 file split until its split-map row is filled in below.
>
> _Split-map: launch.rs — PENDING._
> _Split-map: editor/view.rs — PENDING._
> _Split-map: dialog.rs — PENDING._
> _Split-map: launch_pipeline.rs — PENDING._

The two grandfathered `tests.rs` (`launch/tests.rs` 8603L, `daemon/tests.rs` 7612L) are
**under** the 10000L test cap — not this item's blocker; their fix is owned by
[test-infra-behavioral-specs](/roadmap/test-infra-behavioral-specs/). Leave grandfathered.

**Done-when.** `file-size-budget.toml` `[[production]]` list is empty.

---

## R5 — editor + settings model collapse (blocked on unify-settings)

**Goal.** Bring `editor/model.rs` (4176L) + `settings/model.rs` (3852L) under cap by
splitting the **unified** surface, not the duplicated pair.
**Blocked** on [unify-settings-editor-surfaces](/roadmap/unify-settings-editor-surfaces/)
("Partially implemented — the structural unification is open"). **Long pole** — everything
else proceeds in parallel. After unify lands: re-read the unified model, split along its
real cluster boundaries, refresh the budget. **Done-when** both files (or their unified
successor) sit under 2000L and W6 closes.

---

## R6 — Burn down the 58 clippy `#[expect]` grandfathers

**Goal.** Zero `#[expect(clippy::…, reason = "tracked in codebase-health-enforcement")]`.
All 58 are **item-level** (each on one struct/fn); none module-level. Verified totals:
`struct_excessive_bools` 40 · `fn_params_excessive_bools` 10 · `too_many_arguments` 4 ·
`too_many_lines` 4.

**Fix strategy (important).** Many `struct_excessive_bools` sites are `*Facts`/`*Plan`
parameter-objects that were *created* to dodge `fn_params_excessive_bools` — naively
"bundle bools into a struct" just **moves the lint**. Correct fix = model mutually-exclusive
bool clusters as **enums** (state machines). Example: `ModalOverlayState` (9 modal-open
bools, mutually exclusive) → one `enum OpenModal { None, List, Editor, … }`. For genuinely
independent flags (`SupportedSgr`), a single bitflags/flags type is fine. Each refactor is
structure-only; delete the `#[expect]` once the lint passes. Cluster PRs by crate.

**Risk order:** do low-fan-out first, high-fan-out last. `SupportedSgr` (1 construction
site) mechanical; `ModalOverlayState` (7, all-bool) clean enum candidate;
`EvidenceSummary` (31 sites) + `ConsoleInputDispatchFacts` (16 sites) last.

### `struct_excessive_bools` (40)

| `#[expect(` file:line | struct (item line) | bools |
|---|---|---|
| jackin-capsule/src/agent_status/evidence.rs:62 | ProcessEvidence (66) | 6 |
| jackin-capsule/src/agent_status/evidence.rs:89 | EvidenceSummary (93) | 10 |
| jackin-capsule/src/daemon.rs:162 | Multiplexer (166) | 4 |
| jackin-capsule/src/session.rs:137 | OscPolicy (141) | 4 |
| jackin-capsule/src/tui/model.rs:32 | MuxModeState (36) | 4 |
| jackin-capsule/src/tui/model.rs:58 | PointerShapeState (62) | 6 |
| jackin-capsule/src/tui/model.rs:159 | CursorVisibilityState (163) | 5 |
| jackin-capsule/src/tui/view.rs:31 | CapsuleRatatuiFrame<'a> (36) | 6 |
| jackin-console/src/tui/components/footer_hints.rs:51 | WorkspaceListFooterFacts (56) | 12 |
| jackin-console/src/tui/components/footer_hints.rs:72 | WorkspaceListFooterInputFacts (77) | 8 |
| jackin-console/src/tui/components/footer_hints.rs:157 | WorkspaceFooterScrollFacts (162) | 5 |
| jackin-console/src/tui/components/op_picker.rs:463 | FieldStageBackPlan (468) | 5 |
| jackin-console/src/tui/components/op_picker.rs:499 | FieldStageRefreshPlan (504) | 4 |
| jackin-console/src/tui/components/op_picker.rs:520 | SectionStageBackPlan (525) | 4 |
| jackin-console/src/tui/components/save_preview.rs:22 | WorkspaceSavePreview (27) | 4 |
| jackin-console/src/tui/components/save_preview.rs:645 | SettingsGeneralPreview (650) | 4 |
| jackin-console/src/tui/model/stage.rs:61 | ConsoleInputDispatchFacts (66) | 12 |
| jackin-console/src/tui/model/stage.rs:82 | ConsoleStageModalFacts (87) | 7 |
| jackin-console/src/tui/screens/editor/update.rs:160 | EditorTabSelectPlan (165) | 4 |
| jackin-console/src/tui/screens/settings/update.rs:1145 | SettingsScrollFocusPlan (1150) | 4 |
| jackin-console/src/tui/screens/workspaces/update.rs:780 | WorkspaceListSelectionPlan (785) | 6 |
| jackin-console/src/tui/screens/workspaces/update/tests.rs:217 | TestListSelection (222) | 6 |
| jackin-console/src/tui/screens/workspaces/view.rs:51 | WorkspaceListDisplayRow (56) | 4 |
| jackin-console/src/tui/screens/workspaces/view.rs:65 | WorkspaceListDisplayRowFacts (70) | 4 |
| jackin-console/src/tui/screens/workspaces/view.rs:108 | WorkspaceSidebarFacts (113) | 5 |
| jackin-console/src/tui/update.rs:251 | ListPreRenderScrollResetPlan (256) | 4 |
| jackin-console/src/tui/update.rs:263 | ListPreRenderFacts (268) | 6 |
| jackin-console/src/tui/view.rs:18 | ModalOverlayState (23) | 9 |
| jackin-launch-tui/src/tui/model.rs:12 | LaunchView (16) | 5 |
| jackin-protocol/src/agent_status.rs:59 | AgentStatusReport (63) | 6 |
| jackin-protocol/src/attach.rs:165 | AttachCapabilities (169) | 5 |
| jackin-protocol/src/attach.rs:180 | AttachCapabilitySources (184) | 5 |
| jackin-term/tests/conformance.rs:55 | CellSnapshot (57) | 6 |
| jackin-term/src/cell.rs:24 | Attrs (28) | 9 |
| jackin-term/src/grid.rs:89 | DamageGrid (93) | 3 |
| jackin-term/src/snapshot.rs:29 | SnapCell (31) | 12 |
| jackin-term/src/width.rs:75 | SupportedSgr (77) | 13 |
| jackin-tui/src/components/status_footer.rs:15 | StatusFooterHover (17) | 4 |
| jackin/src/cli/prewarm.rs:20 | PrewarmArgs (23) | 8 (clap `#[arg]`) |
| jackin-xtask/src/pr.rs:39 | Categories (41) | 4 |

### `fn_params_excessive_bools` (10)

| `#[expect(` file:line | fn (item line) | bools |
|---|---|---|
| jackin-console/src/tui/model/create_prelude.rs:94 | create_prelude_modal_step (99) | 5 |
| jackin-console/src/tui/screens/settings/update.rs:229 | settings_env_key_plan (234) | 4 |
| jackin-console/src/tui/screens/settings/update.rs:1189 | settings_modal_open (1194) | 4 |
| jackin-console/src/tui/screens/settings/view.rs:115 | settings_modal_render_plan (120) | 4 |
| jackin-console/src/tui/screens/workspaces/update.rs:980 | workspace_list_scroll_focus_plan (985) | 6 |
| jackin-console/src/tui/screens/workspaces/view.rs:181 | current_directory_display_row (186) | 4 |
| jackin-console/src/tui/update.rs:331 | list_modal_key_target (336) | 4 |
| jackin-console/src/tui/update.rs:372 | shared_modal_scroll_target (377) | 5 |
| jackin-console/src/tui/update.rs:463 | list_pre_render_focus_plan (468) | 4 |
| jackin-host/src/host_clipboard.rs:619 | validate_linux_clipboard_backend (623) | 4 |

### `too_many_arguments` (4)

| `#[expect(` file:line | fn (item line) |
|---|---|
| jackin-runtime/src/runtime/attach.rs:567 | spawn_agent_session (571) |
| jackin-runtime/src/runtime/image.rs:866 | prewarm_agent_image_from_validated_repo (870) |
| jackin-runtime/src/runtime/image.rs:1156 | ensure_local_role_base (1160) |
| jackin-runtime/src/runtime/image.rs:1331 | build_agent_image (1335) |

### `too_many_lines` (4) — overlaps R4 file splits

| `#[expect(` file:line | fn (item line) |
|---|---|
| jackin/src/app/workspace_cmd.rs:14 | handle (18) |
| jackin/src/console/tui/run.rs:167 | run_console (171) |
| jackin-runtime/src/runtime/launch.rs:636 | launch_role_runtime (640) |
| jackin-runtime/src/runtime/launch/launch_pipeline.rs:160 | load_role_with (164) |

**Done-when.** `grep -rn 'tracked in codebase-health-enforcement' crates --include='*.rs'`
returns zero.

---

## R7 — Ratchet thresholds to target (ONLY after R4 + R6)

**Goal.** Enforce the *target* shape: files ≤ 1500L, fns ≤ 150 logical lines.
**Verified band (1500–2000L production, the next decomposition wave):**
`jackin/src/cli/diagnostics.rs` 1964 · `jackin-term/src/grid.rs` 1889 ·
`jackin-capsule/src/session.rs` 1733 · `jackin-console/src/tui/screens/settings/update.rs` 1717 ·
`jackin-capsule/src/daemon.rs` 1670 · `jackin-console/src/tui/screens/settings/view.rs` 1635 ·
`jackin-console/src/tui/model/modal.rs` 1587 · `jackin-console/src/tui/input/mouse.rs` 1558 ·
`jackin-console/src/tui/components/footer_hints.rs` 1526 ·
`jackin-capsule/src/tui/components/dialog_widgets.rs` 1510.
**Caveat:** `jackin/tests/dind_e2e.rs` (1669) + `manager_flow.rs` (1523) are NOT named
`tests.rs`, so the gate counts them as **production** — they hit the 1500 cap too.

**Steps (sequence after R4/R6 so nothing breaks on flip day):** lower
`file-size-budget.toml production_cap` 2000 → 1500 (decompose the band above);
tighten `clippy.toml` `too-many-lines-threshold` 450 → toward 150, `cognitive-complexity` 100 →,
`excessive-nesting` 8 → 5, `too-many-arguments` 9 →. Never blanket-`allow`; ratchet in notches.

---

## R8 — Duplicate-version debt (`deny.toml`)

`multiple-versions = "warn"` (`deny.toml:111`) + **35** grandfathered `[bans] skip` entries.
Pay down transitive dupes (`cargo tree -d`), delete each skip as it resolves, then flip to
`deny`. Lower priority (hygiene, not navigability) — acceptable to land last.

---

## R9 — W4 optional: dependency-graph CI artifact

No `cargo-modules`/graph step in `.github/`. Add a **non-blocking** CI job publishing a
`cargo-modules` / `cargo-depgraph` layering graph per PR (informational; no `deny.toml`
license burden). **Done-when** CI uploads the artifact, or the box is closed as "won't do."

---

## R10 — D7 deferred decision: `jackin-config` persistence IO edge

`crates/jackin-config/src/persist.rs` is only **103L** — pure atomic-write + filename
validation, with a tight docstring already scoping it. Decide: (a) keep the thin documented
IO edge in the schema crate, or (b) split it to an L2 adapter for a strictly IO-free schema
crate. Given its size + clear boundary, (a) is the pragmatic default — but it is an operator
ruling. **Done-when** the decision is recorded on the roadmap; if (b), the move shipped.
(The other two D7 deferrals — tool overlap, `FakeOpWriter` dedup — are already resolved.)

---

## R11 — Doc & bookkeeping cleanup

- [x] Retire the execution playbook (deleted; replaced by this file; detail in git history).
- [x] Fix the stale roadmap W5 line count (`image.rs` 2811 → 1952, under cap).
- [x] Reconcile the W4 `deny.toml` bullet wording (teeth = arch gate, not `deny.toml`).
- [ ] Prune the under-cap `runtime/image.rs` entry from `file-size-budget.toml` (covered by R4).
- [ ] Add a one-line "revisit on next sigstore/oci-client bump" note beside the two
      `deny.toml` `RUSTSEC-2023-0071` / `RUSTSEC-2026-0173` advisory ignores.

---

## Ordering / critical path

```
R1 ──▶ R2          break runtime→tui, then flip arch --strict   (gate-keeper)
R3                 finish E1 (no blockers; hot path → E0 bench)
R4 ──▶ R7          clear 2000L backlog, THEN tighten caps to 1500L
R6 ──▶ R7          burn 58 clippy expects, THEN tighten thresholds
R5                 LONG POLE — blocked on external unify-settings
R8, R9, R10, R11   independent hygiene/decision/docs — anytime
```

- **Closes the item:** R1+R2 · R3 · R4+R5 · R6 → **then** R7; R8–R10 alongside.
- **Biggest lever:** R1 — the one edge the umbrella is held open for; R1+R2 converts the
  architecture from reviewer-upheld to CI-enforced.
- **Biggest grind:** R6 (58 sites, 40 struct_bools) + R4/R7 (file splits) — mechanical, one PR each.
- **Only external dependency:** R5 (unify-settings). Everything else proceeds today.

## Per-PR checklist (every slice)

- [ ] Scope = exactly one slice; structure-only (no logic/behavior/perf change).
- [ ] `cargo fmt --check` · clippy `-D warnings` · `cargo nextest run --all-features` green.
- [ ] Behavioral specs `runtime-launch` + `op-picker` pass **unmodified**.
- [ ] `cargo xtask lint` green; refresh the relevant ratchet + prune fixed entries.
- [ ] New/renamed crate: `[lints] workspace = true` + Architecture-Invariant `//!` header.
- [ ] Docs synced same PR: `PROJECT_STRUCTURE.md` + Codebase Map + this file's box + roadmap box.
- [ ] (carve / hot-path slices) E0 launch/attach benchmark shows no regression.
- [ ] DCO sign-off (`-s`); push immediately.
