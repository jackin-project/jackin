# Workspace Manager TUI — Interactive Create / Edit / Delete in the Launcher

**Status:** Proposed
**Date:** 2026-04-23
**Scope:** `jackin` crate only
**PR:** 2 of 3 in the launcher-workspace-manager series (after `toml_edit` migration merged in #162, before the Secrets tab)

## Problem

Today's launcher is strictly launch-oriented. Listing workspaces is a read-only side-effect of the Workspace picker stage; any actual management — creating a new workspace, renaming one, adding a mount, toggling which agents are allowed — requires dropping out to `jackin config workspace …` CLI commands with long argument lists. For operators who live in the launcher, this creates an awkward two-tool workflow: `jackin` to run things, `jackin config …` to set them up.

The problem has three compounding costs:

1. **Discoverability.** The TUI is where operators first encounter jackin. If everything non-launch lives in CLI flags, the feature surface is hidden — and the learning curve is steeper than it needs to be.
2. **Container-path literacy.** Editing a workspace via CLI today requires typing container-side paths like `/workspace/my-app`. That is jackin internals leaking into the UX. An interactive TUI can flip this — the operator picks a host folder (the thing they actually know), jackin maps it into the container automatically.
3. **PR 3's blocker.** The upcoming Secrets tab (PR 3 of this series) needs somewhere to live in the per-workspace editor. Adding secrets editing without first building the editor backwards-plans the work.

PR 1 (`toml_edit` migration, #162) already made `ConfigEditor` the sole write path for `~/.config/jackin/config.toml` and exposed typed setters for every mutation a workspace manager would need (`create_workspace`, `edit_workspace`, `remove_workspace`, `set_env_var`, `set_agent_trust`, etc.). This spec uses that foundation to build the TUI management surface.

## Goals

1. Operators can list, create, edit, and delete workspaces from the launcher — no CLI required for day-to-day operations.
2. Today's launch path stays **keystroke-identical**. `jackin` → Workspace picker → Enter → Agent picker → Enter → launch. The manager is an excursion, not a gate.
3. Create flow starts from host-side folder selection (file browser rooted at `$HOME`), auto-derives container-side paths, and **never requires typing a container path**. Mount `dst` defaults to the same absolute path as `src`; `workdir` is picked from a list of mount destinations and their ancestors.
4. Edit flow stages changes in an in-memory pending `WorkspaceConfig`; `s` persists via `ConfigEditor`, `Esc` with unsaved changes opens a discard/save/cancel confirmation modal.
5. Every persisted mutation flows through `ConfigEditor` (PR 1) — the launcher goes from read-only to read/write, but the write path is already proven.
6. Three reusable widgets (modal text input, file browser, Y/N confirm) emerge from this work. **PR 3's Secrets tab consumes all three.**
7. Visual style follows the jackin brand: phosphor-green palette from `docs/src/components/landing/styles.css`, reuse of `digital_rain` + `step_shimmer` + `spin_wait` from `src/tui/`, plus a new area-bounded rain renderer. No new visual vocabulary; just new compositions.

## Non-Goals

- Per-(workspace × agent) env overrides. That is **PR 3's Secrets tab**; the Agents tab in PR 2 only handles `allowed_agents` + `default_agent`.
- Global (non-workspace) mount management — `[docker.mounts]` and `[docker.mounts.<scope>]` stay CLI-only.
- Agent lifecycle from within the manager: registering an agent, trusting/untrusting, editing git URL or claude overrides. All CLI.
- Multi-process config write safety — same constraint as PR 1; jackin's single-operator pattern.
- Touching `CHANGELOG.md`. Operator curates.
- Any change to `jackin config workspace …` CLI semantics. The CLI remains source of truth; the TUI is a new front door.
- ASCII-art banners or theming beyond what jackin's existing color palette provides.

## Design

### User flow

The launcher gains a third stage, `LaunchStage::Manager`, reached via `m` from the existing `LaunchStage::Workspace`. The five-act flow (detailed walkthrough committed during brainstorming):

- **Act 1.** Jackin opens to today's Workspace picker, unchanged. Footer gains one hint: `m manage`.
- **Act 2 — edit existing.** From the manager list, `Enter` on a workspace pushes a per-workspace editor with four tabs: `General · Mounts · Agents · Secrets ⏳` (the Secrets stub is a visible placeholder so PR 3 fills in the panel without a tab-strip reshuffle).
  - Fields are edited via **modal text input** — a centered overlay when the user presses Enter on a field.
  - Mounts are added by pressing `a`, which opens a **file browser** rooted at `$HOME`. After picking a host folder, a follow-up modal collects `dst` (pre-filled with the same absolute path as host `src`) and `readonly` (single checkbox, off by default).
  - Agents tab: `Space` toggles `allowed`, `*` sets `default_agent`. Checkbox-style UI.
  - Edits stage in a pending `WorkspaceConfig`; dirty markers (`● unsaved`) appear on changed rows. The footer save prompt shows the count of pending changes (`s save (3 changes)`).
  - `s` persists through `ConfigEditor::edit_workspace` + `editor.save()?`. On success, the editor redraws with a `✓ saved · N changes written` banner; dirty markers clear.
  - `Esc` with pending changes opens a **Y/N/C confirm modal**: Discard / Save+leave / Cancel.
- **Act 3 — create new.** From the manager list, `Enter` on `[+ New workspace]` (or pressing `n`) starts the **mounts-first create wizard**:
  1. File browser for the first mount's host source.
  2. Dst + readonly modal (dst defaults to same path as src).
  3. Workdir pick modal — a list of choices: the mount `dst`, each of its ancestors up to `/`. No free-text container path.
  4. Name modal — defaults to the mount `dst` basename, live uniqueness check against existing workspaces.
  5. Drops into the editor with everything populated and staged; `s` runs `ConfigEditor::create_workspace`.
- **Act 4 — delete.** `d` on a workspace row opens a single-line Y/N confirm modal (`Delete "big-monorepo"? [Y/N]`). `Y` runs `ConfigEditor::remove_workspace`; list refreshes.
- **Act 5 — back to launch.** `Esc` from the manager list returns to today's Workspace picker. The freshly-reloaded `AppConfig` from the manager's last save is reflected in the picker's list.

### State machine

Today's `LaunchStage::{Workspace, Agent}` gains a third variant: `Manager(ManagerState)`. The manager is a self-contained sub-state-machine:

```rust
pub enum LaunchStage {
    Workspace,
    Agent,
    Manager(ManagerState),
}

pub struct ManagerState {
    pub stage: ManagerStage,
    pub workspaces: Vec<WorkspaceSummary>,
    pub selected: usize,
    pub toast: Option<Toast>,          // transient "✓ saved" / "✓ deleted" banner
}

pub enum ManagerStage {
    List,
    Editor(EditorState),               // same struct for Edit and Create
    CreatePrelude(CreatePreludeState),
    ConfirmDelete { name: String },
}

pub struct EditorState {
    pub mode: EditorMode,              // Edit { name: String } | Create
    pub active_tab: EditorTab,         // General | Mounts | Agents | Secrets
    pub active_field: FieldFocus,      // which row is highlighted in the tab
    pub original: WorkspaceConfig,     // snapshot from disk (empty on Create)
    pub pending: WorkspaceConfig,      // staged mutations
    pub modal: Option<Modal>,          // overlay if any
}

pub enum Modal {
    TextInput(TextInputState),
    FileBrowser(FileBrowserState),
    WorkdirPick(WorkdirPickState),
    Confirm(ConfirmState),
}

pub struct CreatePreludeState {
    pub step: CreateStep,              // PickFirstMountSrc -> PickFirstMountDst -> PickWorkdir -> NameWorkspace
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
}
```

**Dirty detection.** `pending != original` drives the dirty markers, the save count, and the discard-on-Esc confirmation path. `WorkspaceConfig` derives `PartialEq` already.

**Modal precedence.** When `EditorState.modal` is `Some(_)`, input events route to the modal handler. The underlying tab render still paints in the background (dimmed). Modal Esc closes the modal; modal Enter commits to pending state and closes.

### Module layout

New files, all under `src/launch/`:

```
src/launch/manager/
  mod.rs              — entry point, ManagerState, per-frame dispatcher
  state.rs            — ManagerStage, EditorState, CreatePreludeState, Modal
  render.rs           — render_list / render_editor / render_modal
  input.rs            — handle_key per stage, modal-first precedence
  create.rs           — mounts-first wizard state transitions

src/launch/widgets/   (new; PR 3 reuses all of these)
  mod.rs
  text_input.rs       — modal text field with cursor + single-line input
  file_browser.rs     — host $HOME folder browser
  confirm.rs          — Y/N modal
  workdir_pick.rs     — mount-dst-and-ancestors picker
  panel_rain.rs       — area-bounded wrapper around tui::digital_rain
```

Plus a refactor of `src/tui/animation.rs`:

- Extract the per-frame core of `digital_rain` into a callable `render_rain_frame(area: Rect, state: &mut RainState, frame: &mut Frame)` function. The existing fullscreen `digital_rain` and the new `panel_rain` widget both delegate to it. ~30 LOC refactor, no behavior change to existing callers.

**Estimated new Rust:** ~1500–2000 lines across manager + widgets + animation refactor.

### Widgets — new reusable UI primitives

Each widget holds its own small state struct, renders into a passed `Rect`, and returns an event outcome. All are consumed by both the manager (PR 2) and the Secrets tab (PR 3).

#### `TextInput`

A single-line text field with a block cursor. Renders as a centered modal with a bordered box, a label, the current value, and a footer hint (`Enter ok · Esc cancel`). Supports insert, backspace, arrow keys for cursor position. No clipboard, no multiline, no IME — PR 4 territory.

Used for: name entry, mount dst, any scalar string field on the editor.

#### `FileBrowser`

Modal folder picker rooted at `$HOME` (resolved from `dirs::home_dir()`). Shows folders only (files hidden — mounts are always directories). Columns: entries, with `..` as the first entry if not at root. Footer: `↑↓ · Enter open · u up · s select · Esc cancel`. Returns a selected absolute path.

Hidden folders (starting with `.`) are shown but visually dimmed — operator can still navigate into them. Symlinks follow the target path.

Used for: create-flow mount src, add-mount mount src.

#### `Confirm`

Y/N modal. Centered, bordered, two-line body. `Y` returns `true`, `N` returns `false`, `Esc` returns cancel (distinct from `N`). The prompt text is configurable.

Used for: delete-workspace confirm, discard-unsaved-changes confirm.

#### `WorkdirPick`

A choice-list modal. Given the current set of mount `dst` paths, enumerates `dst` itself + each ancestor up to `/` as pickable options. Labels annotate source: `(mount dst)`, `(parent)`, `(root)`. Returns the selected path.

Used for: create-flow workdir pick, edit General-tab workdir edit.

#### `PanelRain`

A bounded `digital_rain` renderer. Takes a `Rect` and renders jackin's phosphor rain inside it, at reduced density appropriate for a background effect (not fullscreen intensity). Uses the same `RainState` type as the existing `tui::digital_rain`, just applied to a sub-rect.

Used for: empty-details-pane when the manager list cursor is on `[+ New workspace]` (no existing workspace to summarize), during async operations if `spin_wait` is not sufficient.

### `ConfigEditor` integration

`run_launch`'s signature extends to accept `&JackinPaths` so the manager can open its own `ConfigEditor`:

```rust
// today
pub fn run_launch(
    config: &AppConfig,
    cwd: &Path,
) -> Result<Option<(ClassSelector, ResolvedWorkspace)>>;

// after PR 2
pub fn run_launch(
    config: AppConfig,      // owned; replaced after each save
    paths: &JackinPaths,    // needed to open ConfigEditor for writes
    cwd: &Path,
) -> Result<Option<(ClassSelector, ResolvedWorkspace)>>;
```

Every mutation flows through:

- **Save edit**: `editor.edit_workspace(&name, WorkspaceEdit { ... built from pending ... })?`
- **Create**: `editor.create_workspace(&pending.name, pending.workspace_config.clone())?`
- **Delete**: `editor.remove_workspace(&name)?`

After `editor.save()?` returns a fresh `AppConfig`, the manager rebuilds `ManagerState.workspaces` from it. The outer `LaunchState.workspaces` is also rebuilt on transition back to `Workspace` stage so the picker reflects changes.

**Error surfaces.** Validation errors from `ConfigEditor` (workdir/mount mismatch, collisions, reserved env names, etc.) render as a single-line banner at the top of the active panel, using the jackin `--landing-danger` color (`#ff5e7a`) — the one non-green accent the brand palette permits for actual errors. The banner text is the `anyhow::Error` display string. Pending state survives; operator corrects and retries `s`. Write errors (disk full, permission denied) render the same way and keep the editor open.

### Style & effects

The manager follows jackin's existing visual language, using tokens from `docs/src/components/landing/styles.css` and facilities from `src/tui/`:

| Effect | Existing jackin facility | New code |
|---|---|---|
| Boot reveal on manager enter | `tui::digital_rain(duration, reveal)` — existing | ~5 lines to invoke with short duration on `Workspace → Manager(List)` transition |
| Tab-selector slide on Tab/Shift-Tab | None | ~40 LOC timer-driven interpolation in `render.rs` |
| Save-banner shimmer | `tui::step_shimmer` (output.rs) | ~3 lines to apply to the banner text |
| Panel focus glow on panel activation | Custom `ratatui::widgets::Block` border styling | ~25 LOC `FocusGlow` wrapper |
| Phosphor cascade in empty panels | `digital_rain` fullscreen-only today | ~30 LOC via the `render_rain_frame` refactor + `panel_rain.rs` widget |

**Opt-out.** Operators can set `JACKIN_NO_ANIMATIONS=1` to skip all timer-driven effects and draw final states directly. Consistent with ratatui accessibility conventions. `prefers-reduced-motion` equivalent for terminal use.

**Color palette** (identical to jackin's landing page tokens):

- Background: `#0a0b0a` (`--landing-bg`) for the frame; `#0f1110` (`--landing-panel`) for terminal surfaces.
- Phosphor: `#00ff41` (`--landing-accent`, matches `src/tui/mod.rs` `PHOSPHOR_GREEN`).
- Text: `#f4f7f5` (`--landing-text`) for emphasized glyphs, `#9ca8a1` (`--landing-text-dim`) for secondary.
- Danger (only for real error banners): `#ff5e7a` (`--landing-danger`).

## Testing

**Unit tests**, co-located with each module:

- `state.rs`: `ManagerStage` transitions — `List → Editor → List` on Esc, `List → CreatePrelude → Editor{Create} → List` on complete, `List → ConfirmDelete → List` on Y/N. Dirty detection: `pending == original ⇒ not dirty`. Modal stacking: opening a modal preserves the underlying stage.
- `create.rs`: mounts-first wizard — driven by a sequence of test inputs, the resulting staged `WorkspaceConfig` matches a hand-built expected struct. Cover: default dst derivation (same as src), default name derivation (dst basename), collision rejection on existing name, workdir pick producing valid mount-dst-or-ancestor.
- `input.rs`: key dispatch per stage, modal-first precedence (when modal is open, underlying stage's handlers don't fire).

**Widget tests** — each new widget has its own small test module:

- `file_browser.rs`: given a `tempdir()` hierarchy, navigation (Enter, `u`, Backspace) moves the cursor correctly; `s` returns the selected absolute path. Hidden folders are visible but dimmed. Symlinks followed.
- `text_input.rs`: character insertion, backspace, Home/End, arrow-key cursor movement, Esc returns cancel.
- `confirm.rs`: Y returns `true`, N returns `false`, Esc returns cancel. Case-insensitive (`y` == `Y`).
- `workdir_pick.rs`: given a list of mount dsts, the generated options include each dst + each ancestor, deduplicated.

**Integration test** — one end-to-end manager flow. Uses `ratatui::backend::TestBackend` with a scripted key stream: press `m`, navigate to big-monorepo, press Enter, change workdir, press `s`, press Esc, assert persisted `AppConfig` matches expected. This is the regression guard against state-machine drift across future refactors.

**Render tests:** none. Rendering is visual; manual smoke plus the integration test's persisted-state assertion covers the regression surface without brittle golden-file tests.

## Rollout

- **No new dependencies.** Ratatui + crossterm + dialoguer (already in the tree). `tempfile` already a test dep. `dirs` for `$HOME` resolution is already pulled in transitively via workspace code — confirm during implementation, add explicitly if not.
- Lands as one PR off `main`, on `feature/workspace-manager-tui`.
- No schema change to `~/.config/jackin/config.toml`. No migration needed for existing users.
- Launch path is unchanged — operators who never press `m` see zero difference from today's binary.
- **Rollback:** revert the single PR. The `ConfigEditor` + schema are untouched, so no on-disk cleanup is required.

## Open questions

None. All design decisions settled during brainstorming: entry model (separate manager screen on `m`), tab set (General + Mounts + Agents + Secrets-stub), text-edit UX (modal push), staging semantics (explicit save with `s`, discard/save/cancel on Esc), create flow (mounts-first, file-browser-driven, auto-derived dst, workdir-from-dst-list, name last), delete UX (single Y/N modal), Agents tab scope (allowed + default only; env overrides deferred to PR 3), visual style (jackin landing-page palette + existing animation facilities).
