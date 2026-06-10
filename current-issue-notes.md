# Goal Spec: Dialog-Only Auth Editing + Source-Folder Persistence (Workspace Editor)

This file is a **complete, self-sufficient execution spec**. It was planned with a high-capability model so that a smaller model can implement it end-to-end without making any design decisions. Every decision is already made. The executor's job is mechanical: locate, edit, test, commit, push, repeat.

**Operator launch hint:** run the executing agent with a prompt like
`/goal Follow current-issue-notes.md at the repository root. Implement every phase in order, on the current branch, until PR #550 is fully updated.`

---

## 0. Execution Protocol (executor: read this first, follow it for the whole run)

1. **Role.** You are implementing a fully-specified feature in the jackin' repository. All design decisions are final. Do not redesign, do not reduce scope, do not add features or improvements beyond this spec. If you believe the spec is wrong, follow the Stop Conditions — do not improvise.
2. **Binding documents.** Repo rules apply in full: `AGENTS.md`, `crates/AGENTS.md`, `COMMITS.md`, `BRANCHING.md`, `PULL_REQUESTS.md`, `TESTING.md`, `docs/AGENTS.md`, `.github/AGENTS.md`. This spec adds task-specific rules and records explicit operator decisions (work on the existing branch, §2; spec-file deletion, Phase 9). Where this spec is silent, repo rules decide. Where both are silent, match the surrounding code's existing patterns.
3. **Work loop — repeat for each phase, in order, no skipping, no reordering:**
   a. Re-read §1 (Hard Rules) and the current phase section. You do not need to re-read the rest of the file.
   b. Locate code by **symbol name with `rg`**, not by line number. Line references below were correct at commit `74170171c` and may have drifted. Example: `rg -n "fn build_workspace_edit" crates/`.
   c. Write the phase's tests first when the phase lists a regression test (red), then implement (green).
   d. Implement exactly the listed steps.
   e. Run the phase's Verify block. Every command must pass.
   f. Update the Progress Log (§9) — one line, same commit.
   g. Commit with the phase's given commit message (Conventional Commits, `git commit -s`), then `git push` immediately.
4. **Never continue past a red verify.** Fix it. After 3 distinct failed fix attempts on the same error, trigger Stop Conditions.
5. **Stop Conditions.** Stop work, leave the repo compiling (revert uncommitted breakage if needed), append a `BLOCKED:` line to the Progress Log describing the exact blocker (symbol not found, conflicting instruction, unfixable test, etc.), commit, push, and end the run with a report. Do not guess your way past a blocker.
6. **No interactive commands.** Never launch `jackin console`, any TUI, or any command that waits for input. All verification in this run is via tests, `cargo` checks, and docs build commands. Manual TUI smoke-testing is the operator's job after the PR opens.
7. **Context discipline.** Read files in targeted ranges around symbols. Do not bulk-read large files end-to-end when a 60-line window around the symbol suffices.
8. **Reporting.** When all phases are done, end with: branch name, PR #550 URL, list of commits, and the full final verify output summary.

---

## 1. Hard Rules (binding for every phase; a violation = the phase is wrong even if tests pass)

- **R1 — Scope.** Modify only the files named in the phases (plus their test modules and the docs files in Phase 8). If a change seems to require touching another file, check §8 Symbol Map first; if still unclear, Stop Conditions.
- **R2 — No schema bump.** `AgentAuthConfig.sync_source_dir` already exists in the persisted schema (`crates/jackin-config/src/auth.rs`, field `sync_source_dir`). Do **not** bump `CURRENT_WORKSPACE_VERSION` or `CURRENT_CONFIG_VERSION`, do **not** add migration steps or fixtures. This task is writer/UI plumbing only.
- **R3 — Preview rows stay visible.** Demoting a row to preview-only must never remove it from rendering. "Non-focusable" ≠ "hidden".
- **R4 — One focusability predicate.** All surfaces (cursor stepping, Enter dispatch, `D` dispatch, footer mode selection, mouse hit-testing) must consume the same single `auth_row_is_focusable` predicate. No per-surface row lists. This repo has a history of exactly that drift bug.
- **R5 — Kind-generic.** No `if kind == Claude`-style branches. Capability is `auth_mode_supports_source_folder(kind, mode)`; default paths come from `agent.runtime().state_paths().credential_dir`. A Claude-only code path is an automatic failure.
- **R6 — Never print secret values.** The confirm-save preview may say a credential was set/changed/cleared; it must never render the credential value.
- **R7 — Display format.** Default: `default: <path>`. Inherited: `inherited: <path>`. Explicit: bare `<path>` with no prefix. Never render env-var suffixes like `(CLAUDE_CONFIG_DIR)` / `(CODEX_HOME)` anywhere (panel, dialog, Settings screen, confirm preview).
- **R8 — Key semantics.** Enter is the only key that opens a chooser (folder browser, credential source picker). Space cycles enumerated values (`Mode`). Footers lead with the focused row's primary action (`␣ cycle` on Mode, `↵ browse` on Source folder, `↵ set` on credential row); `⇥ button row` is a trailing hint only.
- **R9 — Staging discipline.** The dialog's Save commits staged values into `editor.pending` only. Disk writes happen exclusively through the workspace save flow (`S` → Confirm changes → Save).
- **R10 — Code hygiene.** Workspace lints stay green (`clippy --workspace --all-targets --all-features -- -D warnings`). No `mod.rs` files in `crates/`. Comments only for non-obvious constraints (see AGENTS.md "Code comments"); never narrate the diff. Match surrounding naming and style.
- **R11 — Commits.** Conventional Commits, `git commit -s` (DCO sign-off), push immediately after every commit. Use the commit messages given per phase verbatim (subject line; you may extend the body factually).
- **R12 — Do not merge.** Update PR #550 (Phase 9) and stop. Merge authorization is not granted for this run.
- **R13 — Cleanup stays in its phase.** The `jackin-pr-trailers` / docs-example cleanup happens only in Phase 7, exactly as listed there. Do not interleave it with the auth phases and do not expand it beyond the listed items.
- **R14 — Brand spelling.** In docs/PR prose the project is `jackin'` (lowercase, trailing apostrophe); bare `jackin` only for commands, crates, paths, identifiers.
- **R15 — Docs land in the same PR** (Phase 8): tui-design-decisions rules, user-facing guide text, and internals updates. A feature without its docs is incomplete.

---

## 2. Branch and PR (operator decision: everything lands on the current branch)

- **All work happens on `chore/update-opentelemetry-to-0.32` — the branch this spec ships on, with open PR #550. Do not create any new branch, local or remote.** This also satisfies the AGENTS.md branch-discipline hard rule: an open PR is in scope for the session, so all work goes to its branch.
- Phase 0 verifies you are on that branch and up to date. If `git branch --show-current` prints anything else, `git switch chore/update-opentelemetry-to-0.32` — never branch off it.
- Push with plain `git push` after every commit (R11). Never force-push; if a push is rejected, Stop Conditions.
- PR #550 stays the single PR for the whole branch. Phase 9 updates its title and body to cover everything on the branch. Final title (verbatim): `feat(console,tools): dialog-only auth editing and PR-trailer tooling`.
- Final body: rebuild from `.github/PULL_REQUEST_TEMPLATE.md`, covering (a) the auth feature per this spec, (b) the `jackin-pr-trailers` crate including the Phase 7 cleanup, (c) the agent-attribution policy removal, (d) the `cargo xtask` entrypoint docs. The Verify-locally block must use `--debug` on every `jackin` invocation and follow `AGENTS.md` "Walking the operator through local validation" (suggest `cargo run --bin jackin -- console --debug`; do not add `--no-intro`).

---

## 3. Background: What Is Broken Today (verified against `74170171c`)

The workspace editor's Auth tab (per agent: Claude Code, Codex, Amp, OpenCode; Kimi is Settings-only) shows rows `Mode`, `Source folder`, credential `Source` (when the mode needs one), and `+ Override for a role`, plus per-role override sections.

Four defects:

1. **Persistence is broken (top priority).** Picking a source folder updates only the in-memory `editor.pending`. The save path applies three diff passes — `build_workspace_edit` (general fields), `apply_auth_forward_diff` (auth *mode* only), `apply_env_diff` (credential env values) — inside `save_workspace` (`crates/jackin/src/console/services/config.rs`, ~line 188). **None of them writes `sync_source_dir`** at the workspace or role layer, and `ConfigEditor` (`crates/jackin-config/src/editor.rs`) has only a global-layer setter (`set_global_sync_source_dir`, ~line 278, with the reusable field helper `set_sync_source_dir_field`, ~line 745). Result: footer counts a pending change, the write drops it, reopening shows the default again.
2. **Editing happens in the wrong place.** `Source folder` rows are focusable, and Enter on them opens a folder picker directly from the panel: Enter dispatch in `crates/jackin/src/console/tui/input/editor.rs` (~line 389) routes `WorkspaceSourceFolder | RoleSourceFolder` to `open_auth_source_folder_picker` (`crates/jackin/src/console/tui/input/auth.rs`, ~line 65), and the picker result lands via `apply_file_browser_to_editor` (`input/editor.rs`, ~line 1176). `D` on those rows clears the value from the panel (`handle_d_on_auth_row`, `input/auth.rs` ~line 144). The decision: the panel edits nothing; the edit-auth dialog is the only editing surface.
3. **The dialog cannot edit the source folder.** `AuthForm` (`crates/jackin-console/src/tui/components/auth_panel.rs`, ~line 149) has only `kind`, `mode`, `credential`; `AuthFormFocus` (`crates/jackin-console/src/tui/screens/settings/model.rs`, ~line 140) has only `Mode`, `CredentialSource`, `Save`, `Cancel`, `Reset`.
4. **Display and preview are wrong.** Explicit values render as `explicit: /path`, env-var suffixes like `(CLAUDE_CONFIG_DIR)` are appended (renderer: `crates/jackin-console/src/tui/screens/editor/view.rs`, ~line 837, format `{status}: {path}{env}`), and the `Confirm changes` dialog (`workspace_save_lines`, `crates/jackin-console/src/tui/components/save_preview.rs` ~line 157; populated by `workspace_save_preview`, `crates/jackin/src/console/tui/components/save_preview.rs` ~line 34) lists **no auth changes at all** — neither mode nor credential nor source folder.

Current wrong panel state (cursor reaches `Source folder`, footer offers `↵ edit source`):

```text
┌ Claude Code ───────────────────────────────────────────────────────────────────────┐
│  Mode          sync (inherited)                                                    │
│▸ Source folder default: ~/.claude (CLAUDE_CONFIG_DIR)                              │
│                                                                                    │
│  + Override for a role                                                             │
└────────────────────────────────────────────────────────────────────────────────────┘

         ↑↓ navigate   ↵ edit source   ⇧ tab bar   S save workspace   Esc back
```

Useful structural facts:

- Row model: `AuthRow<K>` (`crates/jackin-console/src/tui/screens/editor/model.rs`, ~line 329) — variants `AuthKindRow`, `WorkspaceMode`, `WorkspaceSource`, `WorkspaceSourceFolder`, `RoleHeader`, `RoleMode`, `RoleSource`, `RoleSourceFolder`, `AddSentinel`, `Spacer`. Flattened by `auth_flat_rows` (`crates/jackin-console/src/tui/screens/editor/update.rs`, ~line 567). The host crate re-exports the alias `AuthRow` in `crates/jackin/src/console/tui/state.rs`.
- Focusability today: `editor_selection_bounds` (`crates/jackin/src/console/tui/input/editor.rs`, ~line 496) skips only `AuthRow::Spacer`.
- The dialog already has a side-modal round-trip pattern to copy: stash form on `editor.modal_parents` → mount side modal → re-mount form with the value staged and focus on `Save`. See `open_auth_source_picker_from_form` (~line 412), `apply_plain_text_to_auth_form` (~line 502), `restore_auth_form_after_op_picker_cancel` (~line 619), all in `crates/jackin/src/console/tui/input/auth.rs`.
- Key routing for the dialog is the pure function `auth_form_key_plan` (`crates/jackin-console/src/tui/components/auth_panel.rs`, ~line 88) returning plans (`Stay`/`Focus`/`CycleMode`/`OpenCredentialSource`/`Save`/`Cancel`/`Reset`), driven by `handle_auth_form_key` (`crates/jackin/src/console/tui/input/auth.rs`, ~line 262).
- Capability gate: `auth_mode_supports_source_folder(kind, mode)` (`crates/jackin-console/src/tui/auth.rs`, ~line 127) — true iff mode is `Sync` and kind ∈ {Claude, Codex, Amp, Kimi, Opencode}.
- `AuthKind::WORKSPACE_PANEL_KINDS` (`crates/jackin-console/src/tui/auth.rs`, ~line 21) excludes Kimi — Kimi's source folder is editable only on the Settings screen (global layer). Do not add Kimi rows to the workspace panel.
- Dialog footer: `auth_form_footer_items` (`crates/jackin-console/src/tui/components/footer_hints.rs`, ~line 909). The `CredentialSource` arm already leads with `↵ set` — keep that.
- Display builders: `editor_source_folder_display` and `settings_source_folder_display` (`crates/jackin/src/console/tui/components/auth_panel.rs`, ~lines 208 / 182) produce `AuthSourceFolderDisplay { kind, path, env_var }` (defined in `crates/jackin-console/src/tui/components/editor_rows.rs`). Layer precedence implemented there: role explicit → workspace explicit → global inherited → built-in default (`~/<state_paths().credential_dir>`).
- In-memory mutators already exist and are correct: `set_workspace_sync_source_dir` / `set_role_sync_source_dir` (`crates/jackin/src/console/domain.rs`, ~lines 348 / 356).

---

## 4. Target Behavior (acceptance mocks — these are the contract)

### Design rules

- The Auth panel is a **preview and navigation surface**; it edits nothing.
- Panel focusable rows: `AuthKindRow` (kind list), `WorkspaceMode`, `RoleMode`, `RoleHeader` (expand/collapse), `AddSentinel` (`+ Override for a role`). Preview-only rows: `WorkspaceSource`, `RoleSource`, `WorkspaceSourceFolder`, `RoleSourceFolder`, `Spacer`.
- Enter on a `Mode` row is that row's only capability: it opens the edit-auth dialog for that layer (workspace or role).
- The dialog edits mode (Space cycles), credential (Enter sets), and source folder (Enter browses; row present only when `auth_mode_supports_source_folder` holds for the *currently selected* mode — it appears/disappears live as Space cycles).
- Dialog Save stages into `editor.pending`; Cancel discards; Reset clears the layer (mode **and** source folder).
- Confirm-changes lists every pending auth change; workspace save persists them; reopen shows them; zero pending count after save+reload.

### Panel: default source folder (no cursor on the folder row, no env suffix, no `edit source` hint)

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder default: ~/.claude                                                                 │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

                 ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace   Esc back
```

### Panel: inherited

```text
│  Source folder inherited: /Users/donbeave/.codex-work                                             │
```

### Panel: explicit (bare path, no `explicit:` prefix)

```text
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
```

### Dialog: sync mode, folder row focused

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Mode                    sync                                                                 │
  │▸ Source folder           default: ~/.claude                                                   │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                            ↵ browse · ↑↓ navigate   ⇥ button row   Esc cancel
```

Enter opens the file browser; picking a folder returns to this dialog with the value staged:

```text
  │▸ Source folder           /Users/donbeave/.claude-scentbird                                    │
```

### Dialog: api_key mode (no folder row; credential footer leads with `↵ set`)

```text
┌ Edit auth ─────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Mode                    api_key                                                              │
  │▸ ANTHROPIC_API_KEY       required                                                             │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                            ↵ set · ↑ navigate   ⇥ button row   Esc cancel
```

Cycling back to `sync` (Space on Mode) restores the folder row with whatever was staged.

### Reset behavior

Dialog `Reset` clears the current layer's mode **and** source folder for that kind. Panel then shows the next effective layer: `inherited: <path>` if a higher layer sets one, else `default: <path>`.

### Role override flow

Role `Mode` row opens the dialog for the role layer. Role source-folder rows are preview-only:

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│▼ Role: the-architect                                                                              │
│      Mode          sync                                                                           │
│      Source folder inherited: /Users/donbeave/.claude-scentbird                                   │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Confirm changes (workspace `S` flow)

```text
┌ Confirm changes ──────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Edit workspace: scentbird                                                                    │
  │                                                                                               │
  │  Auth:                                                                                        │
  │    Claude Code source folder                                                                  │
  │      - default: ~/.claude                                                                     │
  │      + /Users/donbeave/.claude-scentbird                                                      │
  │                                                                                               │
  │                                      Save        Cancel                                       │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘
```

Variants: inherited→explicit (`- inherited: /path` / `+ /new`), explicit→default reset (`- /old` / `+ default: ~/.claude`), role scope (`Role the-architect / Claude Code source folder`). Mode changes render like `Claude Code mode` with `- sync` / `+ api_key`; credential changes render presence only (e.g. `ANTHROPIC_API_KEY set`), never the value (R6).

### Post-save reopen

Explicit path shown, no pending-change count.

---

## 5. Implementation Phases

Run in order. Each phase: Steps → Tests → Verify → Commit.

The fast per-phase verify is:

```sh
cargo fmt --check
cargo clippy -p jackin -p jackin-console -p jackin-config --all-targets -- -D warnings
cargo nextest run -p jackin -p jackin-console -p jackin-config
```

(referred to below as **Verify-fast**; the full workspace battery runs in Phase 9.)

### Phase 0 — Orientation (no code changes)

1. `git switch chore/update-opentelemetry-to-0.32 && git pull --ff-only origin chore/update-opentelemetry-to-0.32`. Then bring the branch up to date with main: `git fetch origin main && git merge origin/main` — an empty/clean merge is fine (push it); on conflicts, Stop Conditions.
2. Baseline: run Verify-fast. It must pass before you change anything; if it fails on a clean checkout, Stop Conditions.
3. Confirm the anchor symbols exist (`rg -n "fn build_workspace_edit|fn save_workspace|fn auth_flat_rows|enum AuthFormFocus|fn auth_mode_supports_source_folder|fn set_global_sync_source_dir" crates/`). Any miss → re-locate by searching the symbol name alone; still missing → Stop Conditions.
4. No commit for this phase.

### Phase 1 — Persist workspace and role source folders (the bug fix; do this first)

Files: `crates/jackin-config/src/editor.rs` (+ its tests), `crates/jackin/src/console/services/config.rs`, `crates/jackin/src/console/domain.rs` only if accessor helpers are needed.

1. **Regression test first (red):** `save_workspace` has no test module today — add `#[cfg(test)] mod tests` in `crates/jackin/src/console/services/config.rs` using the harness pattern from `crates/jackin-config/src/editor/tests.rs`: `tempfile::TempDir` + `JackinPaths::for_tests(temp.path())` (+ a file-contents reader like its `workspace_file_contents`; `tempfile` is already a dev-dependency where that harness lives — add it to the host crate's dev-dependencies if missing). The test: create a workspace via `ConfigEditor`, build `original`/`pending` `WorkspaceConfig` values where `pending` sets `sync_source_dir` for one agent at the workspace layer and one at a role layer, call `save_workspace`, reload, assert both values survive; then save again with the values cleared and assert removal. It must fail on the current tree.
2. Add to `ConfigEditor`: `set_workspace_sync_source_dir(workspace_name, agent, Option<&Path>)` and `set_workspace_role_sync_source_dir(workspace_name, role, agent, Option<&Path>)`, implemented with the existing `set_sync_source_dir_field` helper (same shape as `set_global_sync_source_dir` and the existing `set_workspace_auth_forward` / `set_workspace_role_auth_forward` path-building).
3. Add a sync-source-dir diff pass to `save_workspace`, run alongside `apply_auth_forward_diff`: for each agent at the workspace layer and for each role override (union of original+pending role keys, same as the auth-forward pass), compare `original` vs `pending` `sync_source_dir`; on difference call the new setter (passing `None` to clear).
4. **DRY requirement:** do not copy-paste another ~120-line per-agent block. Restructure the diff walking as data — a slice of `(Agent, accessor)` pairs or equivalent — and route both the existing `auth_forward` pass and the new `sync_source_dir` pass through it. Keep behavior identical for `auth_forward` (including that the workspace panel has no Kimi: iterate the agents the config schema actually stores; absent fields diff as `None` and produce no calls).
5. `ConfigEditor` unit tests: set + clear at both layers, asserting the emitted TOML contains / drops the `sync_source_dir` key under the right table (`[workspaces.<ws>.<agent>]`, `[workspaces.<ws>.roles.<role>.<agent>]`).

Verify: Verify-fast (the Phase-1 regression test now green).
Commit: `fix(config): persist workspace and role auth sync source dirs`

### Phase 2 — Source-folder row in the edit-auth dialog

Files: `crates/jackin-console/src/tui/components/auth_panel.rs` (+ tests), `crates/jackin-console/src/tui/screens/settings/model.rs`, `crates/jackin-console/src/tui/components/footer_hints.rs`, `crates/jackin/src/console/tui/input/auth.rs` (+ tests), `crates/jackin/src/console/tui/state.rs`, `crates/jackin/src/console/tui/input/editor.rs` (file-browser commit arm), `crates/jackin/src/console/tui/components/footer/modal.rs` if the footer dispatcher needs the new focus.

1. Extend `AuthForm` with staged source-folder state: `source_folder: Option<PathBuf>` (the staged explicit value) plus the captured fallback display (the default/inherited `AuthSourceFolderDisplay` computed when the form opens) so the row renders `default:` / `inherited:` values it does not own (R7 formats).
2. Populate it in `open_auth_form_modal` using the exact inputs the panel render path uses: `let synthesized = synthesize_appconfig_for_auth(editor, config)` then `editor_source_folder_display(&synthesized, &workspace_name_for_panel(editor), role_or_empty, kind)` (all in `crates/jackin/src/console/tui/components/auth_panel.rs`; role is `""` for the workspace layer, the role name for a role layer — widen visibility to `pub(crate)` if needed). The synthesized config overlays `editor.pending`, so the dialog's fallback display stays correct for unsaved edits. Do not fork the precedence logic.
3. Add `AuthFormFocus::SourceFolder`. Teach `auth_form_key_plan`: Up/Down reach the row between `Mode` and credential/buttons **only when** `auth_mode_supports_source_folder(kind, current form mode)`; Enter on it returns a new plan variant (e.g. `OpenSourceFolderBrowser`); the row drops out of the focus cycle when Space cycles to a mode without folder support (if focus sits on the row at that moment, move focus to `Mode`).
4. Render the row in the form body (same column alignment as the credential row), with the staged value as bare path, else fallback display.
5. Handle the new plan in `handle_auth_form_key`: stash the form on `editor.modal_parents`, mount `Modal::FileBrowser` with a new `FileBrowserTarget::AuthFormSourceFolder` variant, reusing `crate::console::services::file_browser::from_home_with_hidden()`. On browser commit (`apply_file_browser_to_editor`), pop the stashed form, set the staged path, re-mount with focus `Save` (mirror `apply_plain_text_to_auth_form`). On browser cancel, restore unchanged (mirror `restore_auth_form_after_op_picker_cancel`).
6. Dialog Save: extend the form's commit outcome with the staged folder; in `persist_form`, apply it via `set_workspace_sync_source_dir` / `set_role_sync_source_dir` together with mode + credential. Cancel discards. Reset (`clear_layer`) now also clears the layer's `sync_source_dir` for that kind.
7. Footer: add the `SourceFolder` arm to `auth_form_footer_items`: `↵ browse · ↑↓ navigate ⇥ button row` (+ the shared Esc segment, matching how the other arms end). Leave the `CredentialSource` arm (`↵ set …`) as is.
8. Tests (in the existing test modules of the touched files): key-plan — row reachable in sync, absent otherwise, live appear/disappear on cycle, focus evacuation rule; round-trip — browse stages path, Save lands it in `editor.pending`, Cancel leaves pending untouched, Reset clears mode+folder at the right layer.

Verify: Verify-fast.
Commit: `feat(console): add source-folder editing to the edit-auth dialog`

### Phase 3 — Demote panel rows to preview-only

Files: `crates/jackin-console/src/tui/screens/editor/update.rs` (+ tests), `crates/jackin/src/console/tui/input/editor.rs` (+ its `auth_cursor_step_tests`), `crates/jackin/src/console/tui/input/auth.rs`, `crates/jackin/src/console/tui/components/footer/editor.rs`, `crates/jackin/src/console/tui/input/mouse.rs` (+ tests), `crates/jackin/src/console/tui/state.rs`.

1. Add `pub fn auth_row_is_focusable<K>(row: &AuthRow<K>) -> bool` next to `auth_flat_rows`: `true` for `AuthKindRow`, `WorkspaceMode`, `RoleMode`, `RoleHeader`, `AddSentinel`; `false` for `Spacer`, `WorkspaceSource`, `RoleSource`, `WorkspaceSourceFolder`, `RoleSourceFolder`.
2. `editor_selection_bounds`: build the skip list from `!auth_row_is_focusable(row)` (replacing the Spacer-only match).
3. Enter dispatch: delete the `WorkspaceSourceFolder | RoleSourceFolder → open_auth_source_folder_picker` arm **and** the `WorkspaceSource | RoleSource → open_auth_form_modal` arm (Mode is the single dialog entry point). Delete `open_auth_source_folder_picker` and the now-unused `FileBrowserTarget::AuthWorkspaceSourceFolder` / `AuthRoleSourceFolder` variants plus their `apply_file_browser_to_editor` arms (Phase 2's `AuthFormSourceFolder` is the only auth file-browser target left).
4. `handle_d_on_auth_row`: remove the `WorkspaceSourceFolder`, `RoleSourceFolder`, and credential-`Source` arms (preview rows are inert; layer clearing is the dialog's Reset). Keep `RoleHeader`/`Mode` behavior as-is.
5. Footer mapping (`footer/editor.rs`): demoted rows can no longer be focused, so remove their `AuthEditSource` mappings; `↵ edit source` must be unreachable on the panel. Mode rows keep `↵ edit mode`.
6. Mouse: route Auth-tab click focus/activation through `auth_row_is_focusable`; clicking a preview row neither focuses nor activates.
7. Tests: update cursor-step tests for the new skip set (cursor from `Mode` lands on next focusable, never on folder/credential rows); Enter and `D` on a preview row index are no-ops; mouse-click no-op.

Verify: Verify-fast.
Commit: `fix(console): make auth panel rows preview-only outside the dialog`

### Phase 4 — Display format everywhere

Files: `crates/jackin-console/src/tui/components/editor_rows.rs`, `crates/jackin-console/src/tui/screens/editor/view.rs` (+ view tests), `crates/jackin/src/console/tui/components/auth_panel.rs` (+ tests), `crates/jackin-console/src/tui/screens/settings/view.rs` (+ tests).

1. Renderer: `Explicit` → bare path; `Default` → `default: <path>`; `Inherited` → `inherited: <path>`.
2. Remove the env suffix: delete the `env_var` field from `AuthSourceFolderDisplay` and every read of it (compiler finds them all; verified: `jackin-tui-lookbook` does not consume this type, so the fallout stays within the Phase 4 file list). The suffix must be unrepresentable, not just unused (R7).
3. The dialog row (Phase 2) and the Settings screen render through the same type/format — verify no second formatter exists (`rg "explicit: |CLAUDE_CONFIG_DIR" crates/ --type rust` should hit only tests you are updating).
4. Tests: three display kinds on panel + dialog + settings lines; assert output contains no `(` env suffix and no `explicit:` literal.

Verify: Verify-fast.
Commit: `fix(console): align auth source-folder display labels across surfaces`

### Phase 5 — Auth changes in the confirm-save preview

Files: `crates/jackin-console/src/tui/components/save_preview.rs` (+ tests), `crates/jackin/src/console/tui/components/save_preview.rs` (+ tests), `crates/jackin/src/console/tui/input/save.rs` only if the lines builder signature changes.

1. Extend `WorkspaceSavePreview` with an auth-changes section; render it in `workspace_save_lines` per the §4 mock (`Auth:` header, per-change `- old` / `+ new` pairs, role scope prefix `Role <name> / <Agent> …`).
2. Populate in `workspace_save_preview` by diffing `editor.original` vs `editor.pending`: mode changes, credential changes (presence only — R6), source-folder changes; workspace layer + role overrides. Reuse the same data-driven per-agent walk shape as Phase 1 so preview and write cannot disagree about what changed (shared helper if crate boundaries allow; otherwise mirror the agent list from one definition).
3. Old/new source-folder values render with the §4 labels: previous effective value (`default: …` / `inherited: …` / bare explicit) → new value (bare explicit, or `default:`/`inherited:` after a reset).
4. Tests: workspace-level change, role-level change, reset-to-default, mode change line, credential line shows no secret value, and no-auth-change ⇒ no `Auth:` section.

Verify: Verify-fast.
Commit: `feat(console): list auth changes in the workspace save confirmation`

### Phase 6 — Settings-screen parity (global layer) and cross-agent sweep

Files: `crates/jackin-console/src/tui/screens/settings/{model,view,update}.rs` (+ tests), `crates/jackin/src/console/tui/components/auth_panel.rs`, `crates/jackin/src/console/tui/input/settings*.rs` or the settings input module found via `rg -n "SettingsAuthModal" crates/jackin/src/`.

1. Apply the same rules to the Settings screen's Auth tab: `SettingsAuthLineRow::SourceFolder` (and credential `Source`) become preview-only via the same predicate approach; the Settings edit-auth dialog gains the same source-folder row; its Save writes through the existing `set_global_sync_source_dir`.
2. Kimi is editable here (it is in `SETTINGS_KINDS`) — the generic gate covers it; just ensure tests include Kimi.
3. Cross-agent sweep: `rg -n "AuthKind::Claude" crates/jackin crates/jackin-console --type rust` over your diff — any new match must be in a test or a justified generic dispatch (like the existing named-field accessors), never a behavior branch (R5).
4. Tests: settings cursor skips preview rows; settings dialog folder row works for Kimi and one other kind; global persistence round-trip (`set_global_sync_source_dir` already exists — test the dialog path stages and saves through it).

Verify: Verify-fast.
Commit: `feat(console): dialog-only auth editing on the settings screen`

### Phase 7 — Branch cleanup: `jackin-pr-trailers` and docs examples (this branch's own review findings)

Files: `crates/jackin-pr-trailers/Cargo.toml`, `crates/jackin-pr-trailers/src/main.rs`, `crates/jackin-pr-trailers/README.md`, `.github/AGENTS.md`.

All decisions below are final — implement as stated, no alternatives:

1. **Workspace lint baseline.** Add `[lints] workspace = true` to `crates/jackin-pr-trailers/Cargo.toml` (crates/AGENTS.md hard rule). Fix everything that then fires. For `clippy::print_stdout` / `clippy::print_stderr`: printing is this CLI's purpose — keep the prints and wrap each printing site in a narrowly-scoped `#[expect(clippy::print_stdout, reason = "jackin-pr-trailers is a CLI; the trailer block is its output")]` (mirror the pattern in `crates/jackin-xtask/src/main.rs`). Replace any `unwrap()`/`expect()` on runtime data with `?` + `anyhow::Context`.
2. **Scope the suppression.** Delete the crate-wide `#![expect(clippy::disallowed_methods, …)]` at the top of `src/main.rs`. Introduce one small helper (e.g. `fn run_command(cmd: &mut Command) -> anyhow::Result<std::process::Output>`) carrying a single `#[expect(clippy::disallowed_methods, reason = "short-lived CLI; blocking process calls at the git/gh boundary")]`, and route every `Command` invocation through it.
3. **Git-native trailer parsing** (prefer-libraries rule). Delete `extract_trailers`, `parse_trailer_line`, `is_valid_trailer_key`, and `parse_commit_messages_from_git_log`. Instead: obtain full commit messages (gh JSON path unchanged; local path via `git log --format=%B%x00 <merge-base>..HEAD` split on NUL), then pipe each message to `git interpret-trailers --parse --only-trailers --unfold` (message on stdin) and read `Key: value` lines from stdout. Keep deduplication (case-insensitive key + trimmed value) and output ordering (`Signed-off-by` first, then `Co-authored-by`, then others) in Rust. This also fixes two parser bugs: `Fixes #123` being rewritten to `Fixes: 123`, and trailing body lines like `Note: …` being mis-collected as trailers.
4. **Distinct sync errors.** One message helper, two cases: remote ref missing → `remote branch origin/<branch> not found — push the branch first`; local ≠ remote → `local HEAD differs from origin/<branch> — push your commits first`. Remove the duplicated "dirty things that are not extracted" text.
5. **Use the discovered PR.** In the no-`--pr` path, after the sync check passes: if `gh pr list --head <branch>` found a PR number, extract through the same `gh pr view --json commits` path as an explicit `--pr`; fall back to the merge-base `git log` path only when no PR exists.
6. **Fix the usage examples.** In `README.md` and the `.github/AGENTS.md` trailer-helper section: the built binary is not on `PATH`; show `cargo run -p jackin-pr-trailers --` (or the explicit `target/release/jackin-pr-trailers` path) and keep the documented behavior in sync with items 3–5.
7. **Tests.** Rework the trailer tests to drive the git-native path (`git` in `PATH` is an acceptable test dependency — the tool already shells out to git). Cover: dedup, ordering, `--body-file` append, `Fixes #123` passthrough unchanged, and both sync-error messages (unit-test the message helper).

Verify:

```sh
cargo fmt --check
cargo clippy -p jackin-pr-trailers --all-targets -- -D warnings
cargo nextest run -p jackin-pr-trailers
```

Commit: `fix(pr-trailers): adopt workspace lints and git-native trailer parsing`
(the `README.md` / `.github/AGENTS.md` example fixes may ride in the same commit; if split, use `docs(github): correct jackin-pr-trailers usage examples`)

### Phase 8 — Docs (same PR)

Files: `docs/content/docs/reference/tui-design-decisions.mdx`, the workspace-editing guide page under `docs/content/docs/guides/` (locate via `rg -ln "Auth" docs/content/docs/guides/`), the internals page describing workspace save (`rg -ln "save_workspace|workspace save" docs/content/docs/reference/`).

1. `tui-design-decisions.mdx` — add enforceable rules (pass/fail wording, per its conventions):
   - Preview-only rows: visible, never focusable, no cursor marker, inert to Enter/`D`/mouse; demotion never hides a row.
   - Grouped settings edit only inside their dialog; panels/screens are preview surfaces; one dialog entry point per group (the `Mode` row).
   - Enter opens choosers; Space cycles enumerated values; no other key mutates a value row.
   - Footers lead with the focused row's primary action; `⇥ button row` is never the leading hint on a value row.
2. User-facing guide: how to set a workspace or role source folder (open Auth tab → Enter on Mode → dialog → Enter on Source folder → browse → Save → `S` save workspace), with the `default:`/`inherited:`/bare-path display meanings. No internal paths/symbols (docs/AGENTS.md split).
3. Internals page: update the workspace save-flow description (diff passes incl. the new sync-source-dir pass) and the auth dialog staging model.
4. Roadmap check (AGENTS.md rule): `rg -lni "auth|source folder" docs/content/docs/reference/roadmap/` — if any roadmap item covers this work, update its status per the rule; if none, note "roadmap: no related item" in the PR body.
5. Docs verification, from `docs/`: `bun run build && bun run check:repo-links && bunx tsc --noEmit && bun test`.

Verify: docs commands above + Verify-fast (unchanged Rust still green).
Commit: `docs: document dialog-only auth editing and source-folder flow`

### Phase 9 — Final battery, spec retirement, PR update

1. Full battery, all must pass:

   ```sh
   cargo fmt --check
   cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
   cargo nextest run --workspace
   ```

2. Delete this spec: `git rm current-issue-notes.md` (operator pre-authorized; the durable rules now live in the docs from Phase 8).
   Commit: `chore: retire auth source-folder implementation notes`
3. Push, then update PR #550 per §2: `gh pr edit 550 --title "feat(console,tools): dialog-only auth editing and PR-trailer tooling" --body-file <tmp>` with the rebuilt template body — summary of everything on the branch, the §6 verification matrix results, Verify-locally block (with `--debug`), and the roadmap note. **Do not merge (R12).**
4. Final report per Protocol step 8 (include the PR #550 URL).

---

## 6. Final Verification Matrix (all rows must be demonstrably true before Phase 9 completes)

| # | Check | How proven |
|---|---|---|
| 1 | Workspace-layer source folder survives save + reload | Phase 1 regression test |
| 2 | Role-layer source folder survives save + reload; clearing removes the key | Phase 1 tests |
| 3 | No schema version bump, no migration files in the diff | `git diff origin/main --stat` shows no `versions.rs` / `tests/fixtures/migrations` changes |
| 4 | Panel cursor never lands on `WorkspaceSource`/`RoleSource`/`WorkspaceSourceFolder`/`RoleSourceFolder` | Phase 3 cursor tests |
| 5 | Enter/`D`/mouse on preview rows are no-ops; `open_auth_source_folder_picker` is gone | Phase 3 tests + `rg` finds no symbol |
| 6 | Dialog shows folder row iff current mode supports it; live on Space cycle | Phase 2 key-plan tests |
| 7 | Browse → stage → Save lands in `editor.pending`; Cancel discards; Reset clears mode+folder | Phase 2 tests |
| 8 | Display: `default:`/`inherited:` prefixes, bare explicit, zero env suffixes anywhere | Phase 4 tests + `rg "CLAUDE_CONFIG_DIR" crates/jackin-console/src crates/jackin/src/console` hits no render path |
| 9 | Confirm dialog lists mode/credential/source-folder diffs both layers; no secrets | Phase 5 tests |
| 10 | Settings screen follows the same rules incl. Kimi | Phase 6 tests |
| 11 | No kind-specific behavior branches | Phase 6 sweep |
| 12 | Footer hints: `␣ cycle` / `↵ browse` / `↵ set` lead their rows; panel has no `↵ edit source` | Phase 2/3 tests or footer unit tests |
| 13 | Docs updated (tui rules, guide, internals) and docs build green | Phase 8 commands |
| 14 | fmt + clippy + full nextest green | Phase 9 battery |
| 15 | Every commit conventional + signed-off + pushed to `chore/update-opentelemetry-to-0.32`; no new branch exists; PR #550 title/body updated, not merged | `git log`, `git branch -a`, PR #550 |
| 16 | `jackin-pr-trailers` on the workspace lint baseline; trailer parsing via `git interpret-trailers`; hand-rolled parser gone | Phase 7 verify + `rg "extract_trailers|parse_trailer_line" crates/` finds nothing |
| 17 | Sync-error messages distinct; discovered PR number actually used | Phase 7 tests |
| 18 | `current-issue-notes.md` deleted from the branch | Phase 9 diff |

---

## 7. Operator Priorities (context for judgment calls — not a license to change scope)

1. Make auth settings actually work (persistence) — Phase 1 ships even if later phases stall.
2. Dialog-only editing — the panel must stop editing; the dialog must fully edit.
3. Honest UI — preview shows effective values with the exact §4 labels; confirm dialog tells the whole truth before writing.

---

## 8. Symbol Map (quick reference; locate by name, lines are hints @ `74170171c`)

| Symbol | File | Role |
|---|---|---|
| `build_workspace_edit` | `crates/jackin/src/console/domain.rs` ~1197 | general-fields diff pass (leave as-is) |
| `save_workspace` | `crates/jackin/src/console/services/config.rs` ~188 | save orchestrator — add sync-source-dir pass here |
| `apply_auth_forward_diff` | `crates/jackin/src/console/services/config.rs` ~242 | mode diff pass — restructure data-driven (Phase 1.4) |
| `set_global_sync_source_dir` / `set_sync_source_dir_field` | `crates/jackin-config/src/editor.rs` ~278 / ~745 | existing global setter + field helper to reuse |
| `AgentAuthConfig.sync_source_dir` | `crates/jackin-config/src/auth.rs` ~33 | already-persisted field (R2) |
| `set_workspace_sync_source_dir` / `set_role_sync_source_dir` | `crates/jackin/src/console/domain.rs` ~348/~356 | in-memory pending mutators (reuse in dialog Save) |
| `AuthRow<K>` / `auth_flat_rows` | `crates/jackin-console/src/tui/screens/editor/{model,update}.rs` ~329/~567 | row model / flattener — add `auth_row_is_focusable` beside |
| `editor_selection_bounds` | `crates/jackin/src/console/tui/input/editor.rs` ~496 | focus skip list (Phase 3.2) |
| Enter dispatch (Auth tab) | `crates/jackin/src/console/tui/input/editor.rs` ~389 | arms to delete/keep (Phase 3.3) |
| `open_auth_source_folder_picker` | `crates/jackin/src/console/tui/input/auth.rs` ~65 | panel picker — delete (Phase 3.3) |
| `apply_file_browser_to_editor` | `crates/jackin/src/console/tui/input/editor.rs` ~1176 | file-browser commit arms (rewire Phase 2.5, prune Phase 3.3) |
| `handle_d_on_auth_row` | `crates/jackin/src/console/tui/input/auth.rs` ~144 | `D` arms to prune (Phase 3.4) |
| `AuthForm` / `auth_form_key_plan` | `crates/jackin-console/src/tui/components/auth_panel.rs` ~149/~88 | dialog state + pure key router (Phase 2) |
| `AuthFormFocus` | `crates/jackin-console/src/tui/screens/settings/model.rs` ~140 | add `SourceFolder` (Phase 2.3) |
| `open_auth_form_modal` / `handle_auth_form_key` / `persist_form` / `clear_layer` | `crates/jackin/src/console/tui/input/auth.rs` ~41/~262/~733/~764 | dialog lifecycle (Phase 2) |
| form↔picker stash pattern | `crates/jackin/src/console/tui/input/auth.rs` ~412/~502/~619 | copy for the folder browser (Phase 2.5) |
| `auth_mode_supports_source_folder` | `crates/jackin-console/src/tui/auth.rs` ~127 | the capability gate (R5) |
| `AuthKind::WORKSPACE_PANEL_KINDS` / `SETTINGS_KINDS` | `crates/jackin-console/src/tui/auth.rs` ~21/~32 | Kimi caveat |
| `editor_source_folder_display` / `settings_source_folder_display` | `crates/jackin/src/console/tui/components/auth_panel.rs` ~208/~182 | layer precedence + display builders (Phase 2.2, 4) |
| `synthesize_appconfig_for_auth` / `workspace_name_for_panel` | `crates/jackin/src/console/tui/components/auth_panel.rs` (near `editor_auth_lines_for_state`, ~line 76) | pending-overlay config + name for the display builders (Phase 2.2) |
| `JackinPaths::for_tests` harness | `crates/jackin-config/src/editor/tests.rs` | TempDir test pattern to copy (Phase 1.1) |
| `AuthSourceFolderDisplay` | `crates/jackin-console/src/tui/components/editor_rows.rs` | display struct — drop `env_var` (Phase 4.2) |
| source-folder renderer | `crates/jackin-console/src/tui/screens/editor/view.rs` ~837 | `{status}: {path}{env}` → R7 formats (Phase 4.1) |
| `auth_form_footer_items` | `crates/jackin-console/src/tui/components/footer_hints.rs` ~909 | dialog footer arms (Phase 2.7) |
| panel footer mapping | `crates/jackin/src/console/tui/components/footer/editor.rs` ~122 | drop `EditSource` mappings (Phase 3.5) |
| `workspace_save_preview` / `workspace_save_lines` | `crates/jackin/src/console/tui/components/save_preview.rs` ~34 / `crates/jackin-console/src/tui/components/save_preview.rs` ~157 | confirm preview (Phase 5) |
| `SettingsAuthLineRow` / `settings_auth_lines_for_state` | `crates/jackin-console/src/tui/screens/settings/view.rs` ~30 / `crates/jackin/src/console/tui/components/auth_panel.rs` ~97 | Settings parity (Phase 6) |
| Mouse hit-testing (Auth tab) | `crates/jackin/src/console/tui/input/mouse.rs` | route via predicate (Phase 3.6) |

---

## 9. Progress Log (executor appends one line per phase, same commit as the phase)

Format: `YYYY-MM-DD <phase> — <done|BLOCKED: reason> — <commit subject>`

- 2026-06-10 Phase 1 — done — fix(config): persist workspace and role auth sync source dirs
- 2026-06-10 Phase 2 — done — feat(console): add source-folder editing to the edit-auth dialog
- 2026-06-10 Phase 3 — done — fix(console): make auth panel rows preview-only outside the dialog
- 2026-06-10 Phase 4 — done — fix(console): align auth source-folder display labels across surfaces
- 2026-06-10 Phase 5 — done — feat(console): list auth changes in the workspace save confirmation

---

## Appendix A — Why no schema bump (pre-answered question)

`sync_source_dir` is an existing optional serde field on `AgentAuthConfig`, already written/read for the global layer and already representable in workspace files at `[workspaces.<ws>.<agent>]` and `[workspaces.<ws>.roles.<role>.<agent>]`. This task adds *writers* for layers the schema already models. The AGENTS.md five-artifact migration rule triggers only on schema-shape changes (rename/remove/type/variant/restructure); an unchanged shape needs none of it. Therefore: touching `CURRENT_WORKSPACE_VERSION`, the migration registries, or `tests/fixtures/migrations/` is **wrong** for this task (R2).

## Appendix B — Where the 2026-06-10 PR #550 review findings went

All review findings recorded for this branch are now **in scope for this run** (operator decision: everything finishes on this branch, in this PR):

- pr-trailers lint baseline, suppression scope, git-native parsing, sync-error messages, discovered-PR usage, README/`.github/AGENTS.md` examples → **Phase 7** (decisions pre-made there).
- Stale PR #550 title/body (the OpenTelemetry 0.32 bump and criterion `black_box` fix already sit on `main`) → **Phase 9** title/body update per §2.
- This file's repo-root location → self-retiring; **Phase 9** deletes it after Phase 8 lands the durable rules in docs.
