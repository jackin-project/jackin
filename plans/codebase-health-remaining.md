# Codebase health — remaining work to reach ZERO exceptions

Executor-grade worklist of **everything still open** under
[Codebase health: structure & reviewability](/roadmap/codebase-health-enforcement/).
Written so an agent can run one slice with **zero improvisation**: exact files, exact
`file:line`, exact symbols, exact done-when.

**The finish line is two files at empty.** The whole roadmap item closes when both
machine-checked exception ledgers hold nothing:

- `test-layout-allowlist.toml` → `files = [ ]` (already emptied in the working tree; the
  gate is **RED** until 7 remaining violators are fixed — see Ledger 1).
- `file-size-budget.toml` → `[[production]]` **and** `[[test]]` both empty (13 files still
  over the 1500L cap + 4 stale entries to prune — see Ledger 2).

Then Ledger 3 lands the last threshold notch (fns ≤ 150 lines).

**Verification basis.** Every fact below was checked against the live tree (branch
`refactor/codebase-health-decomposition`, working tree **dirty** with an in-progress
test-layout sweep) on 2026-07-01. Line numbers were accurate at investigation time — an
executor MUST re-confirm each against the file before editing (any landed slice shifts them).

**The invariant (non-negotiable).** **Structure only — never behavior.** No logic,
control-flow, signature, or performance change. The existing test suite + the
`runtime-launch` / `op-picker` behavioral specs must pass **unmodified**. A move that forces
a production-test edit changed behavior → **stop, back out, report.** (Flattening a
`tests.rs`'s internal `mod` wrappers in Ledger 1 is test-file *layout*, not behavior — the
`#[test]` bodies are byte-identical; that is allowed.)

---

## Executor contract (read once, obey every slice)

1. **One slice = one PR.** Do exactly the slice; do not bundle the next one.
2. **Structure only.** See above. If a step seems to need a production-test edit to pass — stop.
3. **Run Verify in order; stop on first red.** Never force past a gate.
4. **Do not guess.** Re-confirm line numbers; if code doesn't match the step, stop and report.
5. **Conventions:** no `mod.rs`; tests in a sibling `tests.rs`; a `tests.rs` is one **flat**
   file with **no** child `mod` declarations; every crate `[lints] workspace = true`; **no
   wildcard imports** (`clippy::wildcard_imports` denied).
6. **Commit:** Conventional Commit `refactor(<scope>): <slice>`, sign off (`-s`), push immediately.

### Standard Verify (every slice)

```
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --all-features
cargo run -p jackin-xtask --locked -- lint        # file-size + test-layout + arch
# behavioral specs runtime-launch + op-picker pass UNMODIFIED
```

After a file drops under its cap, refresh the ratchet and prune the entry:
`cargo run -p jackin-xtask --locked -- lint files --print-budget` (over `file-size-budget.toml`).

### The decomposition recipe (Ledger 2 references it)

Keep the file as a **coordinator** (module doc, public types, dispatch, re-exports); move
each cohesive cluster to a sibling `<module>/<name>.rs`; the child reads ancestor privates
via explicit `use super::…` (**never** a wildcard); items the coordinator/siblings still
call get `pub(super)`; tests stay in `<module>/tests.rs`; no `mod.rs`.

**⚠ CRITICAL — re-export to preserve call sites.** A coordinator's siblings and its
`tests.rs` reach items via `super::X` or `use super::*`. **Every item that moves OUT of a
coordinator MUST be re-exported from it** (`pub(crate) use <sibling>::{A, B};` — explicit
name list, never a wildcard). Do the moves, then add one re-export line per moved item; then
no call site or test glob changes. Skipping this breaks every glob.

---

## Snapshot — already DONE (no action; here so the open set is unambiguous)

R1 broke `jackin-runtime → jackin-tui`; R2 flipped the arch gate to `--strict` in CI; R3
finished the `jackin-isolation` carve (E1); R4 decomposed the four >2000L production files;
R6 cleared all 58 clippy `#[expect]` grandfathers (zero `tracked in
codebase-health-enforcement` remain); R7 steps 1–8 ratcheted the caps down
(`production_cap` 2000→**1500**, `too-many-lines` 450→**200**, `cognitive-complexity`
100→**60**, `excessive-nesting` 8→**5**, `too-many-arguments` 9→**7**) with honest per-item
`#[allow]`s; R8/R9/R10/R11 hygiene + decision + bookkeeping landed. The in-progress working
tree has additionally decomposed `editor/model.rs` (4176→2291), `settings/model.rs`
(3852→2354) and `diagnostics.rs` (1964→1341, now **under** cap) and cleared 20 of 27
test-layout entries. **None of that is committed yet.**

---

# The open set — three ledgers

---

## Ledger 1 — `test-layout-allowlist.toml` → empty (gate currently RED)

The working-tree sweep already emptied the allowlist (`files = [ ]`) and moved inline tests
to sibling `tests.rs` for 20 of the original 27 files. `cargo xtask lint tests` now reports
**7 remaining violations**. Two shapes, both mechanical, both test-file-only.

### Shape A — inline `#[cfg(test)]` module with a body → move to a sibling file (2 files)

- [ ] **`crates/jackin-console/src/tui/input.rs`** — inline `#[cfg(test)] pub mod test_support`
      (body at `:26–48`: `key()`, `mount()` helpers). Move the body to a new
      `crates/jackin-console/src/tui/input/test_support.rs`; replace the inline block with
      `#[cfg(test)] pub mod test_support;`. (`input.rs` already owns an `input/` dir — the
      sibling file is legal.) Callers use `crate::tui::input::test_support::{key, mount}` —
      the path is unchanged, so no call-site edits.
- [ ] **`crates/jackin/src/console/tui.rs`** — contains an inline `#[cfg(test)]` module with a
      body (the gate flags it; the file also has `#[cfg(test)] use` items + a nested
      `mod state { … #[cfg(test)] mod tests; }` at `:84–130`). Run `cargo xtask lint tests`
      to see the exact block, move that inline module body into the matching sibling
      `tests.rs`, and leave only a `#[cfg(test)] mod tests;` declaration. Re-confirm the
      `#[cfg(test)] use` re-exports still reach the moved tests via `use super::*`.

### Shape B — `tests.rs` declares child `mod`s → flatten to one file, no child modules (5 files)

Fix = delete each `mod NAME {` opener and its matching `}`, promoting the inner `#[test]`
fns to the file's top level. Hoist any per-mod `use`/helper up to the file scope (dedupe
against existing ones); rename on the rare `#[test]` name collision across two former mods.
Bodies stay byte-identical.

- [ ] **`crates/jackin-capsule/src/tui/selection/tests.rs`** (319L) — 4 child mods:
      `word_fixtures` (:88), `word_bounds` (:126), `word_bounds_herdr_parity` (:238),
      `word_bounds_terminal_conventions` (:287). Note `word_fixtures` holds shared
      fixtures — hoist it to file scope first, then flatten the three consumers.
- [ ] **`crates/jackin-console/src/tui/console/tests.rs`** (301L) — 1 child mod:
      `quit_confirm` (:29).
- [ ] **`crates/jackin-console/src/tui/op_picker/tests.rs`** (2074L) — 1 child mod:
      `cache_invalidation` (:2014). (Under the 10000L test cap — flatten only.)
- [ ] **`crates/jackin-console/src/tui/screens/workspaces/view/list/tests.rs`** (1888L) —
      4 child mods: `list_name_scroll` (:18), `mount_table` (:310), `mount_block_height`
      (:517), `subpanel_padding` (:591).
- [ ] **`crates/jackin-instance/src/auth/tests.rs`** (2846L) — 1 child mod:
      `source_validation` (:36).

### Ledger 1 done-when

- [ ] `cargo run -p jackin-xtask --locked -- lint tests` reports **0 violations**.
- [ ] `test-layout-allowlist.toml` is `files = [ ]` (already true; keep it so).
- [ ] The full in-progress sweep (the 20 already-moved files + these 7) compiles and commits
      green — Standard Verify passes. (If splitting into more than one PR: each PR keeps the
      allowlist honest — re-add a temporary entry only if a file cannot be fixed that PR, and
      delete it in the PR that fixes it. Target end state is empty.)

---

## Ledger 2 — `file-size-budget.toml` → empty

### Part A — prune stale entries NOW (no code move; verify each with `wc -l` first)

- [ ] Remove the **`crates/jackin/src/cli/diagnostics.rs`** entry (`lines = 1964`) — live tree
      is **1341 < 1500** (already decomposed in the working tree). Commit alongside the
      diagnostics split.
- [ ] Remove the **`crates/jackin-capsule/src/tui/components/dialog_widgets.rs`** entry
      (`lines = 716`) — live tree is **614 < 1500**.
- [ ] Remove **both `[[test]]` entries** — `launch/tests.rs` (8611) and `daemon/tests.rs`
      (7612) are **under** the 10000L `test_cap`, so they need no grandfather. (The test cap
      itself stays 10000 and is owned by the `test-infra-behavioral-specs` roadmap item — out
      of scope here.)
- [ ] After committing the in-progress model splits, refresh the **`editor/model.rs`** entry
      to 2291 and **`settings/model.rs`** to 2354 (they moved but are still > 1500; the
      entries survive only until Part B decomposes them below).

### Part B — decompose the 13 over-cap files < 1500 (one PR each)

Each row = one PR using the decomposition recipe. Coordinator target LOC verified achievable
by the split-map. **Hot-path files (image, grid, session) additionally run the E0
launch/attach benchmark — no regression.** After each: refresh the budget and delete the
row's entry.

#### Production files (11)

- [ ] **`crates/jackin-console/src/tui/screens/settings/model.rs`** (2354 → coord ~950).
      Extract under `settings/model/`: `general.rs` (`SettingsGeneralState` 2256–2353),
      `trust.rs` (`SettingsTrustRow`/`SettingsTrustState`/rows 1457–1602), `mounts.rs`
      (`GlobalMount*`/`GlobalMountsState` 1052–1455 + `GlobalMountDraft` 1604), `env.rs`
      (`SettingsEnvConfig`/`SettingsEnvState`/`SettingsEnvModal` 576–1050), `auth.rs`
      (`AuthFormFocus`/`AuthFormTarget`/`SettingsAuthState` 494–522, 1743–2254). Stays:
      `SettingsTab`, panel traits, `SettingsState` struct + impls (~950L).
      **Hazard:** `AuthFormFocus`/`AuthFormTarget` (494–515) are also used by the editor
      module — extract to `auth.rs` but **re-export publicly** from the coordinator.
- [ ] **`crates/jackin-console/src/tui/screens/editor/model.rs`** (2291 → coord ~1200).
      Extract under `editor/model/`: `key_plans.rs` (the `Editor*KeyPlan` cluster 88–200),
      `mode.rs` (`EditorMode`/`EditorSaveModePlan` + modal traits 203–238), `secrets.rs`
      (`SecretsScopeTag`/`SecretsRow`/`SecretsEnterPlan` 2141–2172), `auth.rs` (`AuthRow`
      2175–2197), `save_flow.rs` (`PendingSaveCommit`/`EditorSaveFlow`/`ConfirmTarget`/…
      2199–2290). Stays: `EditorTab`, `EditorState` struct + the big method/impl blocks.
      **Hazard:** the ~1800-line method impl (291–2074) is a state machine — it stays in the
      coordinator (it fits under 1500 once the type clusters leave); do NOT split mid-impl.
- [ ] **`crates/jackin-runtime/src/runtime/image.rs`** (1973 → coord ~160). **HOT PATH.**
      Extract under `image/`: `decision.rs` (`decide_role_image` 164–411 +
      `published_image_*` 1763–1844), `builder.rs` (`build_agent_image` 1351–1690 +
      `ensure_local_role_base` 1165–1333), `prewarming.rs` (`spawn_sibling_*_prewarm`/
      `prewarm_*` cluster 481–1070), `binaries.rs` (`prepare_*binaries*` 57–61, 413–479,
      1072–1142), `version.rs` (`git_head_sha`/`role_git_sha_for_recipe`/`extract_agent_version`/
      `record_built_agent_version`/`resolve_github_token` 1694–1970). Stays:
      `prewarm_role_images` + public enums + re-exports (~160L).
      **Hazard:** `decide_role_image` (`too_many_lines` allow) and `build_agent_image`
      (`too_many_arguments` + `too_many_lines` allows) move **whole** — do not split their
      inlined multi-phase state. E0 bench after.
- [ ] **`crates/jackin-term/src/grid.rs`** (1908 → coord ~695). **HOT PATH.** Extract under
      `grid/` (joins existing `perform.rs`): `storage.rs` (`RowStore`/`RowArena`/`*_grid`
      216–376, 1863–1885), `scroll.rs` (`scroll_up`/`preserve_visible_rows_to_scrollback`/
      `newline_action*` 1115–1267), `write.rs` (`write_char_at_cursor`/
      `append_to_previous_cluster`/`cell_width` 940–1103, 1831–1859), `modes.rs`
      (`apply_sgr*`/`set_dec_mode`/`set_alt_screen`/`reset_modes` 840–855, 1514–1735),
      `parse.rs` (`reconstruct_csi`/`parse_sgr_color*`/`underline_style_from_sgr` 1746–1905),
      `snapshot.rs` (`dump*`/`scrollback_*` 505–723). Stays: `DamageGrid` struct, enums,
      constructor, core I/O + accessors (~695L).
      **Hazard:** `scroll_up` (`excessive_nesting` allow) stays whole — DECSTBM scroll-region
      semantics. E0 bench after.
- [ ] **`crates/jackin-capsule/src/session.rs`** (1749 → coord ~714). New `session/` dir.
      Extract: `osc_policy.rs` (`OscPolicy`/`osc8_uri_is_safe`/`parse_osc7` + OSC consts
      50–208), `git_types.rs` (`GitContext`/`Oid`/`BranchName`/`PullRequestLookupOutcome`
      357–496), `spawn.rs` (`Session::spawn` 523–777), `terminal_protocol.rs`
      (`feed_pty`/`apply_passthrough_policy`/`handle_unhandled_csi`/… 1199–1478),
      `evidence.rs` (`sample_process_evidence`/`advance_status`/`apply_runtime_event`/…
      920–1073), `commands.rs` (`build_agent_command`/`build_shell_command`/
      `validate_agent_slug`/… 1625–1746). Stays: `Session` struct, scrollback/render/input/
      lifecycle methods (~714L). **Hazard:** hot path (attach) — E0 bench after.
- [ ] **`crates/jackin-console/src/tui/screens/settings/update.rs`** (1727 → coord ~250).
      Extract under `settings/update/`: `key_plans.rs` (the `Settings*KeyPlan` enums + `*key_plan`
      fns 43–282), `global_mount_plans.rs` (`GlobalMount*` plans + fns 484–952),
      `env_plans.rs` (`SettingsEnv*PickerCommitPlan`/`role_picker_*` 950–1071),
      `selection_scroll.rs` (`Settings*ScrollPlan`/`*SelectionPlan`/`settings_scroll_focus_plan`/
      `settings_modal_open` 1073–1322), `env_helpers.rs` (`settings_env_flat_rows`/
      `set_settings_env_value`/`toggle_*`/`remove_*`/`step_cursor_*`/`settings_*_change_count`
      1381–1724). Stays: tab nav + trust/hover/detail-row helpers + dispatch (~250L).
      **Hazard:** `settings_env_flat_rows`/`settings_env_value` have 10+ call sites — re-export.
- [ ] **`crates/jackin-capsule/src/daemon.rs`** (1688 → coord ~1342). Joins the existing
      `daemon/` dir (10 siblings). Extract: `startup.rs` (`Multiplexer::new` 470–585),
      `clipboard.rs` (`request_clipboard_image_*`/`stage_clipboard_image_*`/
      `ClipboardImageInsertMode`/classify helpers 604–740), `control.rs`
      (`control_reply_for_request`/`handle_client_frame`/`write_status_capture`/
      `build_exit_inspect_rows`/`handle_last_session_exit` 747–811, 1487–1685). Stays:
      `Multiplexer` struct + `run_daemon` event loop (~1342L).
      **Hazard:** `run_daemon` (654L event loop) stays whole — it is the daemon entry point.
- [ ] **`crates/jackin-console/src/tui/screens/settings/view.rs`** (1639 → coord ~260).
      Extract under `settings/view/`: `render_tabs.rs` (`render_{general,mounts,env,auth,trust}_tab`
      211–377), `render_modals.rs` (`render_{global_mount,settings_env,settings_auth}_modal`
      567–683), `line_renderers.rs` (`*_lines`/`*_state_lines`/`render_auth_*`/`truncate`
      685–731, 1114–1560), `footer_composer.rs` (`settings_footer_items`/
      `settings_context_footer_mode`/`tab_bar_footer_items`/`content_footer_items` 379–488,
      617–1636), `text_helpers.rs` (the `*_text_input_state`/`*_text_plan`/label/prompt family
      757–1087), `content_geometry.rs` (`settings_frame_areas`/`settings_modal_render_plan`/
      `*_content_height`/`clamp_mounts_scroll_x_for_frame` 64–143, 1088–1111, 1571–1598).
      Stays: `render_settings_screen` orchestrator + routing (~260L).
- [ ] **`crates/jackin-console/src/tui/model/modal.rs`** (1587 → coord ~250). New `modal/` dir.
      Extract: `auth_impls.rs` (all the auth-related trait impls on `ConsoleModal` 282–1346),
      `display.rs` (`rect_mode`/`rect`/`prepare_for_render`/`footer_items*`/
      `footer_items_for_mode` 1349–1587). Stays: `ConsoleModal` enum + core impl methods
      (~250L). **Hazard:** each trait impl carries the full 22-type-param list + where clauses
      — move each impl block **whole**.
- [ ] **`crates/jackin-console/src/tui/input/mouse.rs`** (1558 → coord ~290). Extract under
      `input/mouse/`: `modal_scroll.rs` (`try_scroll_*_modal`/`scroll_*_modal_selection`
      401–636), `scroll_bars.rs` (`try_drag_{horizontal,vertical}_scrollbar` 861–1236),
      `scroll_pan.rs` (`scroll_active_panel*`/`update_scroll_focus` 987–1496), `selection.rs`
      (`try_select_*`/`update_tab_hover` 707–858), `hover.rs` (`update_*_hover`/
      `*_row_at`/`file_browser_*` 321–399, 638–705). Stays: `handle_mouse_with_config`
      dispatch + `clickable_at` + `ScrollArea`/`*_scroll_areas` helpers (~290L).
- [ ] **`crates/jackin-console/src/tui/components/footer_hints.rs`** (1537 → coord ~110).
      Extract under `footer_hints/`: `workspace.rs` (`WorkspaceListFooter*` facts + fns
      27–521), `editor.rs` (`Editor*FooterFacts`/`AuthRowFooterMode`/editor+auth footer fns
      241–697), `settings.rs` (`SettingsContextFooterMode` + settings/secret footer fns
      763–1418), `modals.rs` (`ModalFooterMode` + the `Modal*FooterState` traits + modal
      footer fns 818–1206), `common.rs` (`append_*`/`tab_bar_footer_items`/
      `content_footer_items`/`*_row_footer_items` 819–835, 1208–1534). Stays: the
      `WorkspaceScreenFooter*`/`SettingsScreenFooter*` orchestrators + re-exports (~110L).

#### Integration-test files (2) — counted as production (not named `tests.rs`)

Cargo compiles only top-level `tests/*.rs` as test binaries; files under a `tests/<name>/`
subdir are modules included via `mod X;` (resolves to `tests/<name>/X.rs`) — **no `mod.rs`**.
The gate still counts each submodule `.rs` as production, so keep every one < 1500 (all land
< 500). Keep the top file as the crate root; move helper/fixture/test blocks into submodules.

- [ ] **`crates/jackin/tests/dind_e2e.rs`** (1669 → top ~300). Add `mod common; mod pty_runner;
      mod transcript; mod diagnostics; mod fixtures; mod util;` and move: `common.rs`
      (prereq/docker checks, serial lock 456–591), `pty_runner.rs` (the PTY runner family +
      `Pty*` structs 593–1002), `transcript.rs` (pipe/transcript helpers 1004–1063),
      `diagnostics.rs` (failure context + tail helpers 1065–1181), `fixtures.rs` (role/stub
      seeding + Dockerfile + fake-claude scripts 1189–1620), `util.rs` (`cleanup_role`/`run`/
      sentinel asserts 364–450, 1622–1669). Keep the 4 `#[test]` fns + shared consts +
      `E2eRoleCleanup` in `dind_e2e.rs`. **Hazard:** module-level consts used across siblings
      — keep in `dind_e2e.rs`, `use super::*` in siblings.
- [ ] **`crates/jackin/tests/manager_flow.rs`** (1523 → top ~500). Already uses the
      `#[path = "manager_flow/secrets.rs"] mod secrets;` pattern — extend it. Add
      `auth_common.rs`, `auth_claude.rs`, `auth_github.rs` and move: `auth_common.rs` (the
      auth-kind/role-edit tests + `auth_row_idx` 524–1123), `auth_claude.rs` (the Claude auth
      form tests 551–768, 1129–1246), `auth_github.rs` (the GitHub auth tests 1258–1523).
      Keep the workspace CRUD/launch tests + shared fixtures (`seed_config*`, `editor*`,
      `render_to_dump`, `key`) in `manager_flow.rs`. **Hazard:** `seed_config_with_agents`
      used across siblings — keep in the top file, `use super::*`.

### Ledger 2 done-when

- [ ] `file-size-budget.toml` `[[production]]` list is **empty**.
- [ ] `file-size-budget.toml` `[[test]]` list is **empty**.
- [ ] `cargo run -p jackin-xtask --locked -- lint files` green with `production_cap = 1500`.
- [ ] E0 launch/attach benchmark shows no regression after the image / grid / session slices.

---

## Ledger 3 — final threshold notch (ONLY after Ledgers 1 + 2)

`clippy.toml` is already at target for file size (1500), nesting (5), arguments (7), and
cognitive complexity (60). One notch remains to hit the stated target **fns ≤ 150 lines**:

- [ ] Lower `clippy.toml` `too-many-lines-threshold` **200 → 150**. Add honest per-fn
      `#[allow(clippy::too_many_lines, reason = "…")]` **only** to the genuinely-irreducible
      launch/build orchestrators that already carry a deferred-extraction note
      (`launch_role_runtime`, `run_launch_core`, `load_role_with`, `decide_role_image`,
      `build_agent_image`, `run_daemon`) — never a blanket allow. Every other newly-firing fn
      is extracted, not suppressed.
- [ ] Confirm the other three thresholds are at target and no gate regresses.

### Ledger 3 done-when

- [ ] `clippy.toml` `too-many-lines-threshold = 150`; all four thresholds at target.
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` green.

---

## Note — the `#[allow]` residue (accepted, not an exception ledger)

R6 converted 58 clippy `#[expect]`s; many mutually-exclusive bool clusters became enums, and
the genuinely-orthogonal flag sets / large launch fns became documented `#[allow(…, reason =
"…")]`. Those `#[allow]`s are **intentional, justified suppressions**, not machine-tracked
exceptions — they carry no `tracked in codebase-health-enforcement` marker and do not gate
the item. Reducing them further (e.g. bundling `too_many_arguments` build fns behind config
structs) is **opportunistic follow-up**, out of scope for closing this item. Do not reopen
the item for them.

---

## Blocked / external

- **`editor/model.rs` + `settings/model.rs` and the unify-settings item.** The prior plan
  parked these two behind [unify-settings-editor-surfaces](/roadmap/unify-settings-editor-surfaces/).
  The working tree has **already decomposed both toward the cap** (4176→2291, 3852→2354)
  along their current cluster boundaries, independent of unification. Ledger 2 finishes the
  job on the current structure. If unify-settings later merges the two surfaces, re-split the
  unified successor along its real seams and refresh the budget — but that is no longer a
  blocker for reaching zero file-size exceptions on today's tree.

---

## Ordering / critical path

```
Ledger 1  finish 7 test-layout violators + commit the sweep   → allowlist empty, gate green
Ledger 2A prune 4 stale budget entries                        → (rides Ledger-2B commits)
Ledger 2B decompose 13 files < 1500 (one PR each)             → budget empty
          hot-path files (image, grid, session): E0 bench
Ledger 3  too-many-lines 200 → 150 (after 2B)                 → thresholds at target
```

- **Item closes when:** Ledger 1 empty + green, Ledger 2 both lists empty, Ledger 3 at target.
- **Biggest grind:** Ledger 2B (13 file splits) — mechanical, one PR each, split-maps above.
- **Nothing here is blocked.** All 13 splits + 7 test-layout fixes proceed today.

## Per-PR checklist (every slice)

- [ ] Scope = exactly one slice; structure-only (no logic / behavior / perf change).
- [ ] `cargo fmt --check` · clippy `-D warnings` · `cargo nextest run --all-features` green.
- [ ] Behavioral specs `runtime-launch` + `op-picker` pass **unmodified**.
- [ ] `cargo xtask lint` green; refresh the relevant ratchet + prune fixed entries.
- [ ] Docs synced same PR: `PROJECT_STRUCTURE.md` + Codebase Map + this file's box + roadmap box.
- [ ] (hot-path slices: image / grid / session) E0 launch/attach benchmark shows no regression.
- [ ] DCO sign-off (`-s`); push immediately.
