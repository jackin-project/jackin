# Current jackin' Issue Notes

## Context

- Branch: `chore/update-opentelemetry-to-0.32`
- Open PR: #550
- Date captured: 2026-06-09

## Issue Summary

In the workspace editor's `Auth` tab, agent credential panels show both `Mode` and `Source folder`. `Mode` is an editable setting, while `Source folder` is derived preview information in the panel. The current list presentation makes both rows feel like selectable/editable menu items, and the `Source folder` row is currently wired as editable from the panel.

This must be fixed consistently for every auth kind that supports sync source folders, not just Claude Code. Confirmed examples include Claude Code and Codex; the same interaction model should apply to all supported agents with source-folder configuration, such as Codex, Amp, Kimi, and OpenCode.

## Reproduction

Open a workspace editor and navigate to:

`Auth` -> `Claude Code`

Example screen:

```text
jackin'  . edit workspace . scentbird


 General   Mounts   Roles   Environments   Auth
                                          ------
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder default: ~/.claude (CLAUDE_CONFIG_DIR)                                             │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Also confirmed with:

`Auth` -> `Codex`

Example screen:

```text
jackin'  . edit workspace . scentbird


 General   Mounts   Roles   Environments   Auth
                                          ------
┌ Codex ────────────────────────────────────────────────────────────────────────────────────────────┐
│  Mode          sync (inherited)                                                                   │
│▸ Source folder default: ~/.codex (CODEX_HOME)                                                     │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Expected Behavior

Only editable/action rows should be selectable from the Auth panel. For a selected agent, `Mode` should be selectable. Pressing Enter on `Mode` should open the edit-auth dialog. That dialog should include the same source-folder information, but there it should be editable when the selected mode supports a source folder.

`Source folder` in the panel should remain visible, but it should clearly read as non-interactive preview/configuration context and should not be selectable or editable from the panel itself.

Every time the operator needs to change a source-folder value, they should do it only inside the edit-auth dialog. No source-folder value should be changed directly from the Auth panel.

The dialog style should be consistent with existing auth credential editing:

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    api_key                                                              │
  │  ANTHROPIC_API_KEY       required                                                             │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘
```

For `sync` modes that support source folders, the dialog should include a source-folder row in the same form, but the source folder is optional because a default source folder always exists.

## Actual Behavior

`Source folder` appears visually similar to the editable `Mode` row, so it reads as another selectable configuration row even though it is only preview information.

The edit-auth dialog opened from `Mode` currently only shows `Mode`, `Save`, `Cancel`, and `Reset`, so the operator cannot set the source folder from the correct editing surface.

When a source folder is selected from the panel today, the row shows `explicit: /path`. The desired display is:

- default path in use: show `default: <path>`
- explicitly configured path: show `<path>` without the word `explicit`
- inherited path: keep enough context to distinguish inherited configuration

Do not show the environment variable suffix, for example `(CLAUDE_CONFIG_DIR)`, in the Auth panel or edit-auth dialog source-folder display.

After selecting a source folder, the footer shows one pending change, but the `Confirm changes` dialog only shows `Edit workspace: <name>` and does not list the auth/source-folder change that is about to be saved.

After confirming the save, reopening the workspace editor shows the source folder reverted to the default. The selected source-folder value appears to be ignored or not persisted.

## Relevant Files / Areas

Likely areas:

- Workspace editor Auth tab rendering and input handling.
- Shared edit-auth modal/form rendering and key handling.
- Auth source-folder persistence for workspace and role overrides.
- Save-confirm preview lines for auth/source-folder changes.
- Cross-agent source-folder display and edit behavior for all supported agents.

## Fix Notes

Preserve the source folder information in the panel, but make it visually distinct from editable rows using existing TUI conventions. The row should be excluded from selection/focus behavior.

Move source-folder editing into the edit-auth dialog opened from the `Mode` row.

Ensure the save confirmation preview includes auth source-folder changes.

Ensure saving actually persists workspace-level and role-level source-folder changes.

Apply the same model to every auth kind with source-folder support. Do not implement a Claude-only path.

## Proposed Flow

### Design Rule

The Auth panel is a preview and navigation surface. It should show the effective configuration, but it should not directly edit the source folder.

The edit-auth dialog is the editing surface. Every source-folder change happens there, together with the mode and any required credential source.

This creates one consistent rule:

- Panel row: `Mode` is selectable and opens the edit-auth dialog.
- Panel row: `Source folder` is visible preview text, not selectable.
- Dialog row: `Source folder` is selectable when the current mode supports source folders.
- Save preview: every mode, credential, and source-folder change is listed before saving.
- Persistence: saving writes the source-folder change, and reopening the workspace shows the saved value.

### Supported Auth Kinds

Source-folder editing should use the same flow for every auth kind whose sync mode supports a host-side source folder:

- Claude Code: default `~/.claude`
- Codex: default `~/.codex`
- Amp: default `~/.local/share/amp`
- Kimi: default based on the Kimi agent state path
- OpenCode: default based on the OpenCode agent state path

Agents or auth kinds without source-folder support should not show a source-folder row in the dialog.

### Panel Preview: Default Source Folder

When the agent uses the built-in default path, the panel shows the source folder as context. The source folder row has no cursor marker and cannot receive focus.

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

Codex uses the same shape:

```text
jackin'  · edit workspace · scentbird


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Codex ────────────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync (inherited)                                                                   │
│  Source folder default: ~/.codex                                                                  │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

                 ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace   Esc back
```

Important details:

- No `(CLAUDE_CONFIG_DIR)`, `(CODEX_HOME)`, or other environment variable suffix.
- No cursor on `Source folder`.
- No footer hint saying `edit source` while the panel is focused.

### Panel Preview: Inherited Source Folder

When a workspace or role inherits a configured source folder from a higher layer, the panel should make the inheritance clear without making the row editable.

```text
jackin'  · edit workspace · scentbird


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Codex ────────────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync (inherited)                                                                   │
│  Source folder inherited: /Users/donbeave/.codex-work                                             │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

                 ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace   Esc back
```

`inherited:` is useful because it explains that the visible path is not owned by this workspace or role layer.

### Panel Preview: Explicit Source Folder

When the workspace or role has its own source-folder value, the panel should show the path directly. Do not prefix it with `explicit:`.

```text
jackin'  · edit workspace · scentbird


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

        ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace (1 changes)   Esc discard
```

The row still remains preview-only. To change this value, the operator presses Enter on `Mode`.

### Edit Dialog: Sync Mode With Default Source Folder

Pressing Enter on `Mode` opens the edit-auth dialog. In `sync` mode, the dialog includes a source-folder row because this is the correct editing surface.

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    sync                                                                 │
  │  Source folder           default: ~/.claude                                                    │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                          ␣ cycle · ↓ navigate   ⇥ button row   Esc cancel
```

Pressing Down or Tab moves from `Mode` to `Source folder`.

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Mode                    sync                                                                 │
  │▸ Source folder           default: ~/.claude                                                    │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                            ↵ browse   ↑↓ navigate   ⇥ button row   Esc cancel
```

Pressing Enter on `Source folder` opens the file browser. Picking a folder returns to the same dialog with the selected value staged.

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │  Mode                    sync                                                                 │
  │▸ Source folder           /Users/donbeave/.claude-scentbird                                    │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                            ↵ browse   ↑↓ navigate   ⇥ button row   Esc cancel
```

Pressing Save commits the staged source folder into the workspace editor's pending state. It does not write the config file until the workspace save flow is confirmed.

### Edit Dialog: API Key Mode

When the mode requires a credential, the credential row stays visible and required. The source-folder row should not appear unless the selected mode supports a source folder.

```text
┌ Edit auth ────────────────────────────────────────────────────────────────────────────────────┐
  │                                                                                               │
  │▸ Mode                    api_key                                                              │
  │  ANTHROPIC_API_KEY       required                                                             │
  │                                                                                               │
  │                                Save        Cancel        Reset                                │
  │                                                                                               │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘

                          ␣ cycle · ↓ navigate   ⇥ button row   Esc cancel
```

This keeps the existing credential flow intact.

### Edit Dialog: Back To Sync

If the operator cycles back to `sync`, the source-folder row appears again.

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

`Reset` in the edit-auth dialog should reset the current auth layer. For source folders, that means:

- Workspace layer reset removes the workspace source-folder override for that auth kind.
- Role layer reset removes the role source-folder override for that auth kind.
- The panel then shows either `inherited: <path>` or `default: <path>`, depending on the next effective layer.

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

Role overrides follow the same pattern. The role header and role mode remain selectable. Role source-folder rows are preview-only.

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

To change the role source folder, focus the role `Mode` row and press Enter. The dialog should open for that role layer.

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

After the dialog saves the pending source-folder value, pressing `S` from the workspace editor should show the auth change in the confirm dialog.

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

### Post-Save Reopen Flow

After confirming Save, reopening the workspace editor should show the persisted value.

```text
jackin'  · edit workspace · scentbird


 General   Mounts   Roles   Environments   Auth
                                          ━━━━━━
┌ Claude Code ──────────────────────────────────────────────────────────────────────────────────────┐
│▸ Mode          sync                                                                               │
│  Source folder /Users/donbeave/.claude-scentbird                                                  │
│                                                                                                   │
│  + Override for a role                                                                            │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘

                 ↑↓ navigate   ↵ edit mode   ⇧ tab bar   S save workspace   Esc back
```

No pending-change count should appear after a successful save and reload.

## Implementation Checklist

- Make `Source folder` rows in the workspace Auth panel non-focusable.
- Remove direct panel handling that opens a source-folder picker.
- Keep source-folder preview rows visible when the effective mode supports source folders.
- Remove environment-variable suffixes from source-folder display.
- Display default values as `default: <path>`.
- Display explicit values as `<path>`.
- Display inherited values as `inherited: <path>`.
- Add a source-folder row to the edit-auth dialog for every source-folder-capable auth kind in `sync` mode.
- Let Enter on the dialog source-folder row open the folder picker.
- Return from the picker to the dialog with the selected path staged, not immediately written to config.
- Persist staged source-folder values when the dialog Save button is committed.
- Include workspace and role source-folder diffs in the workspace save confirmation dialog.
- Ensure the workspace save path writes source-folder diffs to `config.toml` or the workspace config.
- Ensure reopening the workspace editor shows the saved explicit source folder.
- Cover Claude Code and Codex in tests, plus at least one other source-folder-capable auth kind to avoid a Claude-only implementation.

## Verification

- Unit test: Auth panel cursor skips `WorkspaceSourceFolder` and `RoleSourceFolder`.
- Unit test: pressing Enter on the panel source-folder preview is a no-op.
- Unit test: pressing Enter on `Mode` opens the edit-auth dialog with a source-folder row for `sync`.
- Unit test: edit-auth dialog source-folder picker stages the selected path and persists it on dialog Save.
- Unit test: workspace save confirmation lists source-folder diffs.
- Unit test: workspace save writes source-folder changes and reload shows the explicit path.
- Visual check: Claude Code default, explicit, inherited, and reset displays.
- Visual check: Codex default, explicit, inherited, and reset displays.
- Visual check: one additional source-folder-capable agent uses the same flow.
