# Current jackin' Issue Notes

## Context

- Branch: `chore/update-opentelemetry-to-0.32`
- Open PR: #550
- Date captured: 2026-06-09
- Last updated: 2026-06-10 (root-cause analysis, dialog-only editing decision, implementation plan, PR #550 review findings)

## Operator Priorities

1. **Make auth settings actually work.** Today a source-folder selection silently fails to persist: the footer counts a pending change, the confirm dialog does not show it, and reopening the workspace shows the default value again. The persistence bug is the first thing to fix — everything else is UX on top of a working save path.
2. **The edit-auth dialog is the only editing surface.** The current implementation put source-folder editing on the Auth panel (outside the dialog), which is the wrong place. The panel is a preview/navigation surface only. Every auth change — mode, credential, source folder — happens inside the edit-auth dialog opened by pressing Enter on `Mode`. After the dialog saves, the operator lands back on the Auth panel and *sees* the staged values there, but cannot change anything from that screen.

## Issue Summary

In the workspace editor's `Auth` tab, agent credential panels show both `Mode` and `Source folder`. `Mode` is an editable setting, while `Source folder` is derived preview information in the panel. The current list presentation makes both rows feel like selectable/editable menu items, and the `Source folder` row is currently wired as editable from the panel (Enter opens a folder picker directly).

This must be fixed consistently for every auth kind that supports sync source folders, not just Claude Code. The same interaction model applies to all supported agents with source-folder configuration: Claude Code, Codex, Amp, Kimi, and OpenCode.

## Reproduction

Open a workspace editor and navigate to:

`Auth` -> `Claude Code`

Example screen (current, wrong: cursor sits on `Source folder`, footer offers `↵ edit source`):

```text
jackin'  · edit workspace · jackin


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Claude Code ───────────────────────────────────────────────────────────────────────┐
│  Mode          sync (inherited)                                                    │
│▸ Source folder default: ~/.claude (CLAUDE_CONFIG_DIR)                              │
│                                                                                    │
│  + Override for a role                                                             │
└────────────────────────────────────────────────────────────────────────────────────┘

         ↑↓ navigate   ↵ edit source   ⇧ tab bar   S save workspace   Esc back
```

Also confirmed with:

`Auth` -> `Codex`

```text
┌ Codex ────────────────────────────────────────────────────────────────────────────┐
│  Mode          sync (inherited)                                                    │
│▸ Source folder default: ~/.codex (CODEX_HOME)                                      │
│                                                                                    │
│  + Override for a role                                                             │
└────────────────────────────────────────────────────────────────────────────────────┘
```

## Actual Behavior

- `Source folder` appears visually similar to the editable `Mode` row, is focusable, and pressing Enter on it opens a folder picker directly from the panel. The whole editing flow lives outside the dialog.
- The edit-auth dialog opened from `Mode` only shows `Mode`, the credential row, `Save`, `Cancel`, and `Reset` — there is no source-folder row, so the correct editing surface cannot set it.
- The panel shows `explicit: /path` for explicitly configured folders and appends env-var suffixes such as `(CLAUDE_CONFIG_DIR)` / `(CODEX_HOME)`.
- After picking a source folder from the panel, the footer shows one pending change, but the `Confirm changes` dialog only shows `Edit workspace: <name>` and does not list the auth/source-folder change.
- After confirming the save, reopening the workspace editor shows the source folder reverted to the default. The selected value is dropped during the write (root cause below).

## Root Cause Analysis

All paths verified against the current tree on 2026-06-10.

### Persistence: where the picked folder is dropped

The in-memory edit works; the on-disk write loses it.

1. Picker commit mutates `editor.pending` correctly:
   - `apply_file_browser_to_editor` — `crates/jackin/src/console/tui/input/editor.rs:1176` routes `FileBrowserTarget::AuthWorkspaceSourceFolder` / `AuthRoleSourceFolder` to
   - `set_workspace_source_folder` / `set_role_source_folder` — `crates/jackin/src/console/tui/input/auth.rs:169` / `:177`, which call
   - `set_workspace_sync_source_dir` / `set_role_sync_source_dir` — `crates/jackin/src/console/domain.rs:348` / `:356`.
   - Because `editor.pending` now differs from `editor.original`, the footer's pending-change counter shows `(1 changes)` — that part is honest.
2. Save flow: `commit_editor_save_with_runner` — `crates/jackin/src/console/tui/input/save.rs:233` emits `WorkspaceSaveEffect::WriteWorkspace { original, pending, … }`, handled by `save_workspace` — `crates/jackin/src/console/services/config.rs:188`. In Edit mode it applies **three diff passes**, and `sync_source_dir` is in none of them:
   - `build_workspace_edit` — `crates/jackin/src/console/domain.rs:1197` → `ConfigEditor::edit_workspace`. Carries only `workdir`, mount upserts/removals, `allowed_roles`, `default_role`, `keep_awake`, `git_pull_on_entry`. The `WorkspaceEdit` struct (`crates/jackin-config/src/schema.rs:439`) has no auth fields at all.
   - `apply_auth_forward_diff` — `crates/jackin/src/console/services/config.rs:242`. Carries only `auth_forward` (the mode), workspace layer and role layer, per agent.
   - `apply_env_diff` — same file. Carries credential env values.
3. `ConfigEditor` (`crates/jackin-config/src/editor.rs`) has `set_global_sync_source_dir` (`:278`, used by the Settings screen via `services/config.rs:129`) and the generic field helper `set_sync_source_dir_field` (`:745`) — but **no workspace-level or role-level sync-source-dir setter**. So the workspace/role value picked in the editor is never written, and the reload after save (`editor_doc.save()` → fresh `AppConfig`) shows the old/default value.

The fix is writer plumbing only. `AgentAuthConfig.sync_source_dir` already exists in the persisted schema (`crates/jackin-config/src/auth.rs:33`) for the workspace file's per-agent tables and role overrides. **No `CURRENT_WORKSPACE_VERSION` bump and no migration artifacts are needed** (`v1alpha6` stays, `crates/jackin-config/src/versions.rs:8`).

### Confirm dialog: why the change is not listed

`begin_editor_save` — `crates/jackin/src/console/tui/input/save.rs:120` builds the confirm modal lines via `build_confirm_save_lines` (`:192`) → `workspace_save_preview` — `crates/jackin/src/console/tui/components/save_preview.rs:34` → `workspace_save_lines` — `crates/jackin-console/src/tui/components/save_preview.rs:157`. The preview struct has no auth fields: it diffs name/workdir/mounts/roles/general toggles only. Auth mode, credential, and source-folder changes are invisible in `Confirm changes` even when they do persist (mode/credential) — the preview is incomplete for the whole auth family, not just source folders.

### Panel: why rows are editable today

- Row model: `AuthRow<K>` — `crates/jackin-console/src/tui/screens/editor/model.rs:329` with variants `AuthKindRow`, `WorkspaceMode`, `WorkspaceSource` (credential), `WorkspaceSourceFolder`, `RoleHeader`, `RoleMode`, `RoleSource`, `RoleSourceFolder`, `AddSentinel`, `Spacer`. Flattened by `auth_flat_rows` — `crates/jackin-console/src/tui/screens/editor/update.rs:567` (`Source` rows appear when the effective mode needs a credential, `SourceFolder` rows when it supports a source folder).
- Focusability: `editor_selection_bounds` — `crates/jackin/src/console/tui/input/editor.rs:496` skips **only `AuthRow::Spacer`**. Every other row, including both `SourceFolder` variants, is focusable.
- Enter dispatch — `crates/jackin/src/console/tui/input/editor.rs:389`:
  - `WorkspaceMode` / `WorkspaceSource` / `RoleMode` / `RoleSource` → `open_auth_form_modal` (the dialog).
  - `WorkspaceSourceFolder` / `RoleSourceFolder` → `open_auth_source_folder_picker` — `crates/jackin/src/console/tui/input/auth.rs:65` → mounts `Modal::FileBrowser` straight from the panel. **This is the panel-side editing path to remove.**
- `D` key — `handle_d_on_auth_row` — `crates/jackin/src/console/tui/input/auth.rs:144` clears the source folder directly from the panel (`RoleSourceFolder`/`WorkspaceSourceFolder` arms). Also panel-side editing; dies with the rule. Layer reset moves to the dialog's `Reset`.
- Footer: `crates/jackin/src/console/tui/components/footer/editor.rs:122` maps `WorkspaceSource`/`RoleSource`/`WorkspaceSourceFolder`/`RoleSourceFolder` to `AuthRowFooterMode::EditSource` → `↵ edit source` (`crates/jackin-console/src/tui/components/footer_hints.rs:336`).
- Mouse: `crates/jackin/src/console/tui/input/mouse.rs` mirrors the Enter dispatch for clicks; it must consume the same focusability predicate or clicks will keep editing preview rows.
- Display: `editor_source_folder_display` — `crates/jackin/src/console/tui/components/auth_panel.rs:208` computes layer precedence (role explicit → workspace explicit → global inherited → built-in default from `agent.runtime().state_paths().credential_dir`) and returns `AuthSourceFolderDisplay { kind, path, env_var }`; the renderer — `crates/jackin-console/src/tui/screens/editor/view.rs:837` prints `{status}: {path}{env}`, producing `explicit: /path` and the `(CLAUDE_CONFIG_DIR)` suffix. `settings_source_folder_display` (`auth_panel.rs:182`) is the Settings-screen sibling with the same format.

### Dialog: what exists to build on

- Modal: `Modal::AuthForm { target, state: Box<AuthForm>, focus, literal_buffer }`; form struct `AuthForm` — `crates/jackin-console/src/tui/components/auth_panel.rs:149` (fields: `kind`, `mode`, `credential`); focus enum `AuthFormFocus` — `crates/jackin-console/src/tui/screens/settings/model.rs:140` (`Mode`, `CredentialSource`, `Save`, `Cancel`, `Reset`).
- Key routing: `handle_auth_form_key` — `crates/jackin/src/console/tui/input/auth.rs:262` via the pure `auth_form_key_plan` — `crates/jackin-console/src/tui/components/auth_panel.rs:88` (plans: `Stay`/`Focus`/`CycleMode`/`OpenCredentialSource`/`Save`/`Cancel`/`Reset`).
- The form already has a modal round-trip pattern for side pickers: stash the form on `editor.modal_parents`, mount the side modal, re-mount the form with the picked value staged and focus on `Save` (`open_auth_source_picker_from_form` `:412`, `apply_plain_text_to_auth_form` `:502`, `restore_auth_form_after_op_picker_cancel` `:619`). The folder browser uses the **same pattern** — no new modal-stack machinery.
- Save commit: `commit_auth_form_save` (`:702`) → `persist_form` (`:733`) → `apply_workspace_auth_commit` / `apply_role_auth_commit` (`crates/jackin/src/console/domain.rs:364`+) mutate `editor.pending`. Reset: `reset_auth_form_layer` (`:714`) → `clear_layer`.
- Capability gate: `auth_mode_supports_source_folder` — `crates/jackin-console/src/tui/auth.rs:127` — `Sync` mode and kind ∈ {Claude, Codex, Amp, Kimi, Opencode}.
- Dialog footer: `auth_form_footer_items` — `crates/jackin-console/src/tui/components/footer_hints.rs:909`, per-focus arms; `CredentialSource` already leads with `↵ set`.

## Expected Behavior

Only editable/action rows are selectable from the Auth panel. For a selected agent, `Mode` is selectable. Pressing Enter on `Mode` opens the edit-auth dialog. The dialog includes the source-folder information, editable there when the selected mode supports a source folder.

`Source folder` in the panel stays visible — it is **not** removed from the display — but it reads as non-interactive preview/configuration context: no cursor marker, never focusable, Enter/`D`/mouse-click are no-ops on it.

Every source-folder change happens only inside the edit-auth dialog. No source-folder value is changed directly from the Auth panel. After the dialog saves, the operator returns to the Auth panel and sees the staged values reflected in the preview rows; the workspace-level `S save workspace` flow then confirms and persists them.

## Design Rules

### Surfaces

- The Auth panel is a **preview and navigation surface**. It shows the effective configuration; it edits nothing.
- The edit-auth dialog is the **only editing surface**. Mode, credential, and source folder all change there, together.

### Row interaction (panel)

- `Mode` (workspace and role layers) is selectable; Enter on it opens the edit-auth dialog for that layer. That is `Mode`'s only capability.
- `Source folder` and the credential `Source` row are preview-only: visible, never focusable, skipped by cursor movement (like `Spacer` today), inert to Enter, `D`, and mouse clicks.
- `RoleHeader` (expand/collapse) and `+ Override for a role` remain selectable — they are navigation/action rows, not value rows.

### Key semantics (dialog)

- **Enter is the only way to change a value that needs a chooser.** Enter on `Source folder` opens the folder browser; Enter on the credential row opens the credential source picker. The chooser returns to the dialog with the value staged.
- **Space cycles enumerated values.** Space on `Mode` cycles to the next supported mode. No chooser pops for enum rows.
- The footer must lead with the focused row's primary action — `␣ cycle` on `Mode`, `↵ browse` on `Source folder`, `↵ set` on the credential row. `⇥ button row` stays as a trailing hint but is never presented as the way to change a value.

### Display format (panel, dialog, and Settings screen alike)

- Default path in use: `default: <path>`
- Inherited from a higher layer: `inherited: <path>`
- Explicitly configured at this layer: `<path>` — bare, no `explicit:` prefix.
- Never show the environment-variable suffix (`(CLAUDE_CONFIG_DIR)`, `(CODEX_HOME)`, …) anywhere.

### Staging and persistence

- The dialog's Save commits staged values into the workspace editor's pending state only. Nothing touches disk until the workspace save flow (`S` → `Confirm changes` → Save) is confirmed.
- The `Confirm changes` dialog lists **every** pending auth change — mode, credential, and source folder, workspace-level and role-level.
- The workspace save writes source-folder changes to the workspace config, and reopening the editor shows the saved value. No pending-change count after a successful save and reload.

## Supported Auth Kinds

Source-folder editing uses the same flow for every auth kind whose sync mode supports a host-side source folder (`auth_mode_supports_source_folder`):

- Claude Code: default `~/.claude`
- Codex: default `~/.codex`
- Amp: default `~/.local/share/amp`
- Kimi: default from the Kimi agent state path
- OpenCode: default from the OpenCode agent state path

Defaults come from `agent.runtime().state_paths().credential_dir` — do not hard-code paths in the TUI layer.

**Kimi caveat:** `AuthKind::WORKSPACE_PANEL_KINDS` (`crates/jackin-console/src/tui/auth.rs:21`) deliberately excludes Kimi, so the workspace editor panel covers Claude Code, Codex, Amp, and OpenCode; Kimi's source folder is reachable only through the Settings screen (global layer), which gets the same dialog-only treatment. Auth kinds without source-folder support (Grok, GitHub CLI, Z.AI, MiniMax) never show a source-folder row in the dialog.

## Proposed Flow

### Panel Preview: Default Source Folder

The source-folder row has no cursor marker and cannot receive focus.

```text
jackin'  · edit workspace · scentbird


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder default: ~/.claude                                                                 │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

                 ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace   Esc back
```

Codex uses the same shape (`default: ~/.codex`). Important details:

- No `(CLAUDE_CONFIG_DIR)`, `(CODEX_HOME)`, or other environment-variable suffix.
- No cursor on `Source folder`; cursor movement skips it.
- No `↵ edit source` footer hint anywhere on the panel.

### Panel Preview: Inherited Source Folder

```text
┌ Codex ────────────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync (inherited)                                                                   │
│  Source folder inherited: /Users/donbeave/.codex-work                                             │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

`inherited:` explains that the visible path is not owned by this workspace or role layer.

### Panel Preview: Explicit Source Folder

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

        ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace (1 changes)   Esc discard
```

The row remains preview-only. To change the value, press Enter on `Mode`.

### Edit Dialog: Sync Mode With Default Source Folder

Pressing Enter on `Mode` opens the edit-auth dialog. In `sync` mode the dialog includes a source-folder row.

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    sync                                                                 │
  │  Source folder           default: ~/.claude                                                   │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                          ␣ cycle · ↓ navigate   ⇥ button row   Esc cancel
```

Pressing Down moves from `Mode` to `Source folder`:

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

Pressing Enter on `Source folder` opens the file browser. Picking a folder returns to the same dialog with the selected value staged:

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Mode                    sync                                                                 │
  │▸ Source folder           /Users/donbeave/.claude-scentbird                                    │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                            ↵ browse · ↑↓ navigate   ⇥ button row   Esc cancel
```

Pressing Save commits the staged source folder into the workspace editor's pending state. It does not write the config file until the workspace save flow is confirmed.

### Edit Dialog: API Key Mode

When the mode requires a credential, the credential row stays visible and required, and Enter is the way to set it — the footer leads with `↵ set`, not with the button row. The source-folder row does not appear unless the selected mode supports one.

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

This keeps the existing credential flow intact.

### Edit Dialog: Back To Sync

If the operator cycles back to `sync` (Space on `Mode`), the source-folder row appears again with whatever was staged:

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    sync                                                                 │
  │  Source folder           /Users/donbeave/.claude-scentbird                                    │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                          ␣ cycle · ↓ navigate   ⇥ button row   Esc cancel
```

### Reset Behavior

`Reset` in the edit-auth dialog resets the current auth layer. For source folders:

- Workspace-layer reset removes the workspace source-folder override for that auth kind.
- Role-layer reset removes the role source-folder override for that auth kind.
- The panel then shows `inherited: <path>` or `default: <path>`, depending on the next effective layer.

Preview after reset to default:

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder default: ~/.claude                                                                 │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Preview after reset to inherited:

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync (inherited)                                                                   │
│  Source folder inherited: /Users/donbeave/.claude-work                                            │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Role Override Flow

Role overrides follow the same pattern. The role header and role `Mode` remain selectable. Role source-folder rows are preview-only.

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

To change the role source folder, focus the role `Mode` row and press Enter. The dialog opens for that role layer:

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    sync                                                                 │
  │  Source folder           inherited: /Users/donbeave/.claude-scentbird                         │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘
```

After selecting a role-specific folder and saving the dialog:

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│▼ Role: the-architect                                                                              │
│      Mode          sync                                                                           │
│      Source folder /Users/donbeave/.claude-architect                                              │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Save Confirmation Flow

After the dialog saves the pending source-folder value, pressing `S` from the workspace editor shows the auth change in the confirm dialog.

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

                                        S save   C/Esc cancel
```

For inherited-to-explicit:

```text
  │  Auth:                                                                                        │
  │    Codex source folder                                                                        │
  │      - inherited: /Users/donbeave/.codex-work                                                 │
  │      + /Users/donbeave/.codex-scentbird                                                       │
```

For explicit-to-default reset:

```text
  │  Auth:                                                                                        │
  │    Claude Code source folder                                                                  │
  │      - /Users/donbeave/.claude-scentbird                                                      │
  │      + default: ~/.claude                                                                     │
```

For a role override:

```text
  │  Auth:                                                                                        │
  │    Role the-architect / Claude Code source folder                                             │
  │      - inherited: /Users/donbeave/.claude-scentbird                                           │
  │      + /Users/donbeave/.claude-architect                                                      │
```

Mode and credential changes get the same treatment (e.g. `Claude Code mode: - sync / + api_key`, `Claude Code credential: ANTHROPIC_API_KEY set`); today the confirm dialog lists none of the auth family.

### Post-Save Reopen Flow

After confirming Save, reopening the workspace editor shows the persisted value:

```text
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

No pending-change count after a successful save and reload.

## Implementation Plan

Ordered by operator priority: persistence first, then the dialog, then the panel demotion, then display, then preview, then parity and docs. Each phase is independently testable.

### Phase 1 — Persist workspace and role source folders (make it work)

1. Add `ConfigEditor::set_workspace_sync_source_dir(workspace, agent, Option<&Path>)` and `ConfigEditor::set_workspace_role_sync_source_dir(workspace, role, agent, Option<&Path>)` in `crates/jackin-config/src/editor.rs`, reusing the existing `set_sync_source_dir_field` helper (`:745`) that the global setter (`:278`) already uses.
2. Add a sync-source-dir diff pass to `save_workspace` (`crates/jackin/src/console/services/config.rs:188`), alongside `apply_auth_forward_diff`: compare `original` vs `pending` per agent at the workspace layer and per role override, call the new setters on change (including change-to-`None` for reset).
3. DRY note while in there: `apply_auth_forward_diff` (`services/config.rs:242`) is ~120 lines of per-agent copy-paste (and the workspace-layer block silently has no Kimi arm — currently correct only because Kimi is absent from `WORKSPACE_PANEL_KINDS`). Restructure both passes as a loop over an agent list with field accessors so the source-folder pass does not become a second copy-paste block; per AGENTS.md, symmetric variants should be data, not control flow.
4. No schema bump: `sync_source_dir` is already part of `AgentAuthConfig` (`crates/jackin-config/src/auth.rs:33`). Writers only.
5. Tests: `ConfigEditor` unit tests for both new setters (set + clear, workspace + role); a `save_workspace` round-trip test asserting a pending source-folder change survives save + reload (this is the regression test for the reported bug).

### Phase 2 — Source-folder row in the edit-auth dialog

1. Extend `AuthForm` (`crates/jackin-console/src/tui/components/auth_panel.rs:149`) with staged source-folder state: the staged `Option<PathBuf>` plus the read-only fallback display (default/inherited path and label) captured when the form opens, so the row can render `default:` / `inherited:` values it does not own.
2. Pre-populate it in `open_auth_form_modal` (`crates/jackin/src/console/tui/input/auth.rs:41`) from the same layer-resolution logic the panel display uses (share the helper, do not fork it — see Phase 4).
3. Add `AuthFormFocus::SourceFolder` (`crates/jackin-console/src/tui/screens/settings/model.rs:140`) and teach `auth_form_key_plan` (`crates/jackin-console/src/tui/components/auth_panel.rs:88`) the row: reachable by Up/Down between `Mode` and the credential/button rows, **only when** `auth_mode_supports_source_folder(kind, form.mode)` — the row appears/disappears live as Space cycles `Mode`.
4. Render the row in `build_form_lines` / `render_form` (`crates/jackin-console/src/tui/components/auth_panel.rs:292`+) using the shared display formatting (Phase 4 rules).
5. Enter on the row opens the folder browser via the existing form round-trip pattern: stash the form on `editor.modal_parents`, mount `Modal::FileBrowser` with a new target (e.g. `FileBrowserTarget::AuthFormSourceFolder`), and on commit re-mount the form with the path staged and focus on `Save` — exactly like `open_auth_source_picker_from_form` / `apply_plain_text_to_auth_form` (`input/auth.rs:412` / `:502`). Browser cancel restores the form unchanged (`restore_auth_form_after_op_picker_cancel` pattern, `:619`).
6. Dialog `Save` (`persist_form`, `input/auth.rs:733`): extend the commit outcome with the staged source folder and apply it to `editor.pending` through `set_workspace_sync_source_dir` / `set_role_sync_source_dir` (`domain.rs:348` / `:356`) in the same commit as mode + credential. `Cancel` discards the staged value. `Reset` (`clear_layer`, `:764`) also clears the layer's `sync_source_dir`.
7. Footer: add an `AuthFormFocus::SourceFolder` arm to `auth_form_footer_items` (`crates/jackin-console/src/tui/components/footer_hints.rs:909`): `↵ browse · ↑↓ navigate   ⇥ button row   Esc cancel`. The `CredentialSource` arm already leads with `↵ set` — keep that; the rule is that every value row's footer leads with its Enter action.
8. Tests: key-plan tests for the new focus state (row reachable only in `sync`, skipped otherwise); round-trip test staging a browsed path; Save-commits/Cancel-discards/Reset-clears tests against `editor.pending`.

### Phase 3 — Demote panel rows to preview-only

1. Introduce a single focusability predicate for Auth rows (e.g. `auth_row_is_focusable(&AuthRow<K>) -> bool` next to `auth_flat_rows` in `crates/jackin-console/src/tui/screens/editor/update.rs:567`): `false` for `Spacer`, `WorkspaceSource`, `RoleSource`, `WorkspaceSourceFolder`, `RoleSourceFolder`; `true` for `AuthKindRow`, `WorkspaceMode`, `RoleMode`, `RoleHeader`, `AddSentinel`. One predicate, consumed everywhere — cursor stepping, Enter dispatch, `D` dispatch, footer mode selection, and mouse hit-testing — so the surfaces cannot drift (this codebase has had exactly that class of bug before).
2. Wire it into `editor_selection_bounds` (`crates/jackin/src/console/tui/input/editor.rs:496`), which today skips only `Spacer`.
3. Enter dispatch (`input/editor.rs:389`): delete the `WorkspaceSourceFolder | RoleSourceFolder → open_auth_source_folder_picker` arm and delete `open_auth_source_folder_picker` (`input/auth.rs:65`) plus the now-unused `FileBrowserTarget::AuthWorkspaceSourceFolder` / `AuthRoleSourceFolder` variants and their `apply_file_browser_to_editor` arms (`input/editor.rs:1176`). The `WorkspaceSource | RoleSource → open_auth_form_modal` arm also goes — those rows are no longer focusable; `Mode` is the single dialog entry point.
4. `D` key (`handle_d_on_auth_row`, `input/auth.rs:144`): remove the `WorkspaceSourceFolder` / `RoleSourceFolder` / `WorkspaceSource` / `RoleSource` arms (preview rows are inert); layer clearing lives in the dialog's `Reset`.
5. Footer (`crates/jackin/src/console/tui/components/footer/editor.rs:122`): drop the `EditSource` mapping for the demoted rows; `↵ edit source` disappears from the panel. `Mode` rows keep `↵ edit mode`.
6. Mouse (`crates/jackin/src/console/tui/input/mouse.rs`): route click-to-focus and click-to-activate through the shared predicate so clicking a preview row does nothing.
7. Tests: update the cursor-step tests (`auth_cursor_step_tests`, `input/editor.rs:1191`) for the new skip set; add a test that Enter on a source-folder row index is a no-op; mouse-click no-op test.

### Phase 4 — Display format

1. In the display builders — `editor_source_folder_display` (`crates/jackin/src/console/tui/components/auth_panel.rs:208`) and `settings_source_folder_display` (`:182`) — and the renderer (`crates/jackin-console/src/tui/screens/editor/view.rs:837`):
   - `Explicit` renders the bare path, no `explicit:` prefix.
   - `Default` renders `default: <path>`; `Inherited` renders `inherited: <path>`.
   - Drop the env-var suffix everywhere; remove the `env_var` field from `AuthSourceFolderDisplay` (`crates/jackin-console/src/tui/components/editor_rows.rs`) so the suffix cannot come back.
2. The dialog row (Phase 2) uses the same `AuthSourceFolderDisplay` type and rendering rules — one formatter, all surfaces (panel, dialog, Settings screen).
3. Tests: snapshot/unit tests for the three display kinds on both panel and dialog, asserting no env suffix and no `explicit:` literal anywhere in Auth rendering.

### Phase 5 — Confirm-changes preview

1. Extend `WorkspaceSavePreview` (`crates/jackin-console/src/tui/components/save_preview.rs`) with an auth-changes section and render it in `workspace_save_lines` (`:157`) using the mock format above (`- old` / `+ new`, `default:`/`inherited:` labels, `Role <name> / <Agent> source folder` for role scope).
2. Populate it in `workspace_save_preview` (`crates/jackin/src/console/tui/components/save_preview.rs:34`) by diffing `editor.original` vs `editor.pending`: mode changes, credential changes (presence/source only — never print secret values), and source-folder changes, at both layers. Reuse the same per-agent diff walk as the Phase 1 save pass so the preview and the write can never disagree about what changed.
3. Tests: preview-line tests for workspace-level change, role-level change, reset-to-default, and the mode/credential lines.

### Phase 6 — Cross-agent coverage and Settings-screen parity

1. Everything above is kind-generic via `auth_mode_supports_source_folder` and `state_paths()` — verify no Claude-only branch sneaks in (the bug history here is exactly "works for Claude, forgotten for Codex").
2. Settings screen (global layer) gets the same rules: `SettingsAuthLineRow::SourceFolder` (`crates/jackin-console/src/tui/screens/settings/view.rs:30`) becomes preview-only, the Settings edit-auth dialog gains the same source-folder row writing through `set_global_sync_source_dir` (`crates/jackin-config/src/editor.rs:278`), and the display rules match. Kimi is editable only here (see Kimi caveat).
3. Tests: cover Claude Code, Codex, and at least one of Amp/OpenCode in the workspace flow, plus Kimi in the Settings flow.

### Phase 7 — Docs (same PR as the implementation)

1. `docs/content/docs/reference/tui-design-decisions.mdx` (hard rule — cross-cutting TUI rules must land there in the same PR): add enforceable rules for (a) preview-only rows — visible, non-focusable, no cursor marker, inert to Enter/`D`/mouse; (b) dialogs as the only editing surface for grouped settings; (c) Enter-opens-chooser / Space-cycles-enum key semantics; (d) footers lead with the focused row's primary action.
2. User-facing docs (`guides/` / `commands/`): describe configuring auth source folders per workspace and per role through the edit-auth dialog.
3. Internals docs (`reference/`): update the workspace-save flow description (three diff passes + the new source-folder pass).

## Verification

- Unit: Auth panel cursor skips `WorkspaceSource`, `RoleSource`, `WorkspaceSourceFolder`, and `RoleSourceFolder`; lands only on `AuthKindRow`/`Mode`/`RoleHeader`/`AddSentinel`.
- Unit: Enter, `D`, and mouse click on a panel source-folder preview row are no-ops.
- Unit: Enter on `Mode` opens the edit-auth dialog; the dialog shows a source-folder row iff the form's current mode supports one, and the row appears/disappears as Space cycles modes.
- Unit: dialog source-folder browse stages the picked path; dialog Save commits it to `editor.pending`; Cancel discards; Reset clears the layer.
- Unit: `save_workspace` writes workspace-level and role-level `sync_source_dir` changes (set and clear) and reload shows them — the regression test for the reported revert bug.
- Unit: `Confirm changes` lists mode, credential, and source-folder diffs at both layers; no auth change can be pending without a preview line (assert via the shared diff walk).
- Visual (`cargo run --bin jackin -- console --debug`): Claude Code and Codex default / explicit / inherited / reset displays; no env-var suffix; no `explicit:` prefix; footer hints per focused row (`␣ cycle`, `↵ browse`, `↵ set`); one additional source-folder-capable agent (Amp or OpenCode) end-to-end; Settings screen parity incl. Kimi.
- End-to-end: pick folder in dialog → Save dialog → `S` → confirm shows the diff → Save → reopen editor → explicit path shown, zero pending changes.

## PR #550 Review Findings (fix in this PR, before merge)

Findings from the 2026-06-10 review of the branch itself, separate from the auth follow-up above.

1. **`crates/jackin-pr-trailers/Cargo.toml` is missing `[lints] workspace = true`** — violates the crates/AGENTS.md hard rule; the crate silently skips the workspace lint baseline (`print_stdout`/`print_stderr`, pedantic, unwrap/expect discipline) that every other crate obeys. Add the table and fix what fires.
2. **Crate-wide `#![expect(clippy::disallowed_methods)]`** (`src/main.rs:1`) — suppression discipline says smallest practical scope; move the expect to the individual `Command` call sites (or a small exec helper) with the boundary named.
3. **Hand-rolled trailer parser vs the prefer-libraries rule** — `git interpret-trailers --parse` implements exactly this (trailer-block detection, `Key: value` parsing) with git's own semantics. Either replace `extract_trailers` / `parse_commit_messages_from_git_log` with `git interpret-trailers` invocations (the tool already shells out to git), or keep the hand-rolled version with the rule-required justification comment. Known parser gaps if kept:
   - `parse_trailer_line`'s `Key #value` branch rewrites `Fixes #123` to `Fixes: 123` on output — drops the `#`, changes meaning.
   - A final body line shaped like `Note: something` is collected as a trailer (false positive; git requires a proper trailer block).
4. **Misleading sync error** — the missing-`origin/<branch>` case and the local≠remote case print the same message ("Branch … is different than the remote branch. You need to push…"), and the wording ("you have dirty things that are not extracted") is confusing. Distinguish "remote branch does not exist (push it first)" from "local and remote differ (push first)", and tighten the prose. The duplicated message block should be a single helper.
5. **Dead PR-discovery round-trip** — in the no-`--pr` path the tool finds the PR number via `gh pr list` but only logs it; extraction still reads local `git log`. Either drop the lookup or use the found number to fetch PR commits (and then the local/remote sync check becomes the fallback path only).
6. **PR title/description are stale** — the OpenTelemetry 0.32 bump and the criterion `black_box` fix now sit on `main`; the effective diff is the trailers crate + attribution-policy removal + xtask docs + these notes. Retitle/rewrite the body before merge per the title/description reconciliation rule in `.github/AGENTS.md` (e.g. `chore(tools,docs): add jackin-pr-trailers, drop agent-attribution mandate, capture auth notes`).
7. **`current-issue-notes.md` location** — a scratch planning file at the repo root sits outside the docs conventions (planned-work specs live under `docs/content/docs/reference/roadmap/`). Acceptable as a working capture for the immediate follow-up PR, but it must not linger: the implementing PR consumes it and deletes it (or relocates the durable parts into a roadmap item / tui-design-decisions.mdx).
8. **README/AGENTS examples vs binary reality** — `.github/AGENTS.md` shows `cargo build -p jackin-pr-trailers --release` then bare `jackin-pr-trailers`; the built binary lands in `target/release/`, which is not on `PATH`. Show `cargo run -p jackin-pr-trailers --` or the explicit `target/release/jackin-pr-trailers` path.
