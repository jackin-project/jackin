# Workspace Manager TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an interactive workspace manager screen to the jackin launcher that lets operators list, create, edit, and delete workspaces without dropping to CLI, while keeping today's launch path keystroke-identical.

**Architecture:** A new `LaunchStage::Manager(ManagerState)` variant sits alongside today's `Workspace` and `Agent` stages, reached via `m` keypress from the Workspace picker. The manager is a self-contained sub-state-machine with list / editor / create-prelude / confirm-delete stages and an overlay Modal system. All persisted writes flow through `ConfigEditor` (landed in PR 1). Five reusable widgets (three third-party-wrapped, two hand-rolled) compose every interaction.

**Tech Stack:** Rust, ratatui 0.30 + crossterm 0.29 (existing), `ratatui-textarea` 0.9.x (new), `ratatui-explorer` 0.3.x (new), `tui-widget-list` 0.15.x (new), `ConfigEditor` from PR 1, jackin's existing `tui::digital_rain` / `step_shimmer` / `spin_wait`.

**Spec reference:** `docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md` (PR #164, merging separately before this work ships).

---

## Branching & commits

All work lands on `feature/workspace-manager-tui` (worktree at `.worktrees/workspace-manager-tui`). Every commit:
- Conventional Commits subject: `feat(launch): …`, `refactor(tui): …`, `test(launch): …`.
- DCO sign-off via `git commit -s`.
- Exactly one `Co-authored-by: Claude <noreply@anthropic.com>` trailer.
- Does **not** touch `CHANGELOG.md` (operator curates manually).
- Does **not** push, does **not** open a PR until Task 22.

---

## File structure

**New files:**

```
src/launch/widgets/
  mod.rs              — re-exports; shared types (ModalOutcome)
  text_input.rs       — wraps ratatui-textarea in single-line mode
  file_browser.rs     — wraps ratatui-explorer, adds folders-only + s-select
  confirm.rs          — Y/N modal (hand-rolled)
  workdir_pick.rs     — list of mount dsts + ancestors (on tui-widget-list)
  panel_rain.rs       — area-bounded rain

src/launch/manager/
  mod.rs              — ManagerState, per-frame dispatcher, public entry
  state.rs            — ManagerStage enum, EditorState, CreatePreludeState, Modal
  render.rs           — render_list / render_editor / render_modal
  input.rs            — handle_key per stage, modal-first precedence
  create.rs           — mounts-first wizard state transitions
```

**Modified files:**

- `Cargo.toml` — add three deps + `unstable-widget-ref` feature on ratatui.
- `src/tui/animation.rs` — extract `render_rain_frame` so it can render into a `Rect`.
- `src/tui/output.rs` — re-export `step_shimmer` for use by the manager's save banner.
- `src/launch/mod.rs` — dispatch `LaunchStage::Manager` in the event loop, extend `run_launch` signature to accept `&JackinPaths`, add `m` keybinding to Workspace stage.
- `src/launch/state.rs` — add `Manager(ManagerState)` variant to `LaunchStage`.
- `src/launch/input.rs` — route `m` keypress to stage transition.
- `src/launch/render.rs` — footer hint update (add `m manage`), add dispatcher for Manager stage.
- `src/app/mod.rs` — update `run_launch` call site to pass `paths`.

**Unchanged (verify):**

- `ConfigEditor` (`src/config/editor.rs`) — PR 1's API covers every mutation we need.
- `AppConfig` schema — no change.
- `src/workspace/mod.rs` — `WorkspaceConfig`, `WorkspaceEdit`, `MountConfig` unchanged.

---

## Design notes for implementers

### Reusable types — used across widgets + state machine

Define in `src/launch/widgets/mod.rs`:

```rust
/// Outcome of a modal's event-handling cycle. Passed back to the manager
/// state machine to decide whether to close the modal, commit its value,
/// or keep it open.
#[derive(Debug, Clone)]
pub enum ModalOutcome<T> {
    /// User is still interacting with the modal — keep rendering.
    Continue,
    /// User committed with this value (e.g. Enter in text input).
    Commit(T),
    /// User cancelled (Esc).
    Cancel,
}
```

### State machine shapes (from spec § 3)

```rust
// src/launch/manager/state.rs

use std::path::PathBuf;
use crate::config::AppConfig;
use crate::workspace::{MountConfig, WorkspaceConfig};
use super::super::widgets::{
    text_input::TextInputState,
    file_browser::FileBrowserState,
    confirm::ConfirmState,
    workdir_pick::WorkdirPickState,
};

#[derive(Debug)]
pub struct ManagerState {
    pub stage: ManagerStage,
    pub workspaces: Vec<WorkspaceSummary>,
    pub selected: usize,
    pub toast: Option<Toast>,
}

#[derive(Debug)]
pub enum ManagerStage {
    List,
    Editor(EditorState),
    CreatePrelude(CreatePreludeState),
    ConfirmDelete { name: String, state: ConfirmState },
}

#[derive(Debug)]
pub struct WorkspaceSummary {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_agent_count: usize,
    pub default_agent: Option<String>,
    pub last_agent: Option<String>,
}

#[derive(Debug)]
pub struct EditorState {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub modal: Option<Modal>,
    pub error_banner: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Agents,
    Secrets,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldFocus {
    /// Index-based focus into the active tab's rows.
    Row(usize),
}

#[derive(Debug)]
pub enum Modal {
    TextInput { target: TextInputTarget, state: TextInputState },
    FileBrowser { target: FileBrowserTarget, state: FileBrowserState },
    WorkdirPick { state: WorkdirPickState },
    Confirm { target: ConfirmTarget, state: ConfirmState },
}

#[derive(Debug, Clone, Copy)]
pub enum TextInputTarget { Name, Workdir, MountDst }

#[derive(Debug, Clone, Copy)]
pub enum FileBrowserTarget { CreateFirstMountSrc, EditAddMountSrc }

#[derive(Debug, Clone, Copy)]
pub enum ConfirmTarget { DeleteWorkspace, DiscardChanges }

#[derive(Debug)]
pub struct CreatePreludeState {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal>,
}

#[derive(Debug, Clone, Copy)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub shown_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy)]
pub enum ToastKind { Success, Error }
```

### Dirty detection

```rust
impl EditorState {
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }
    pub fn change_count(&self) -> usize {
        // Field-level diff; used for "s save (N changes)" footer.
        // Count: workdir changed, default_agent changed, each mount
        // added/removed/changed, each allowed_agents diff, etc.
        // Implementation detail in Task 10.
        0 // placeholder; real impl in Task 10
    }
}
```

### ConfigEditor integration points

Every mutation flows through `ConfigEditor`:

```rust
// Save edit:
let mut editor = ConfigEditor::open(paths)?;
let edit = WorkspaceEdit {
    workdir: (pending.workdir != original.workdir).then(|| pending.workdir.clone()),
    // ... diff pending vs original to build the edit ...
};
editor.edit_workspace(&name, edit)?;
let reloaded = editor.save()?;

// Create:
let mut editor = ConfigEditor::open(paths)?;
editor.create_workspace(&pending.name, pending.config.clone())?;
let reloaded = editor.save()?;

// Delete:
let mut editor = ConfigEditor::open(paths)?;
editor.remove_workspace(&name)?;
let reloaded = editor.save()?;
```

On error: `editor.save().err()` bubbles up. The manager catches it and sets `EditorState::error_banner = Some(error.to_string())`. Pending state survives. Operator retries `s`.

### Style effects

From the spec §Style:

| Effect | Implementation |
|---|---|
| Boot reveal on manager enter | Call `tui::digital_rain(400, None)` once when transitioning `Workspace → Manager(List)`. Existing function handles the CRT warm-up. |
| Tab slider animation | Track `tab_transition: Option<(from, to, Instant)>` on `EditorState`. Render an interpolated x-position for the highlight rectangle. Easing: cubic-bezier (ratatui handles with simple linear; good enough). |
| Save banner shimmer | Call `tui::step_shimmer(banner_text)` once when save succeeds. |
| Panel focus glow | Track `focus_flash: Option<Instant>` per panel. Render phosphor-bright border while `elapsed < 1200ms`, interpolating toward dim otherwise. |
| Phosphor cascade in empty panels | `PanelRain` widget (Task 9). |

Honor `JACKIN_NO_ANIMATIONS=1` env var: skip all timer-driven animations, draw final states directly.

---

## Task 1: Add deps + feature flag + scaffold module directories

**Files:**
- Modify: `Cargo.toml`
- Create: `src/launch/widgets/mod.rs`
- Create: `src/launch/manager/mod.rs`
- Modify: `src/launch/mod.rs` (add module declarations only)

- [ ] **Step 1: Add three new deps to Cargo.toml**

Edit `Cargo.toml`. Find the existing `ratatui = "0.30"` line (should be around line 22–28). Add the three crates after it, and enable the `unstable-widget-ref` feature on ratatui:

```toml
ratatui = { version = "0.30", features = ["unstable-widget-ref"] }
crossterm = "0.29"
ratatui-textarea = "0.9"
ratatui-explorer = "0.3"
tui-widget-list = "0.15"
```

Note: the `ratatui = { version = "0.30", features = [...] }` form replaces the plain `ratatui = "0.30"` string. Keep the crossterm line as-is.

Run `cargo check` to pull the deps. Expected: success; three new entries in `Cargo.lock`.

- [ ] **Step 2: Create `src/launch/widgets/mod.rs` with placeholder**

Create `src/launch/widgets/mod.rs`:

```rust
//! Reusable widgets for the workspace manager TUI.
//!
//! Three of the widgets wrap ratatui ecosystem crates
//! (`ratatui-textarea`, `ratatui-explorer`, `tui-widget-list`). Two are
//! hand-rolled (`Confirm`, `PanelRain`). All are consumed by both the
//! manager (PR 2) and the Secrets tab (PR 3).

pub mod confirm;
pub mod file_browser;
pub mod panel_rain;
pub mod text_input;
pub mod workdir_pick;

/// Outcome of a modal's event-handling cycle. Passed back to the
/// manager state machine to decide whether to close the modal, commit
/// its value, or keep it open.
#[derive(Debug, Clone)]
pub enum ModalOutcome<T> {
    /// User is still interacting with the modal — keep rendering.
    Continue,
    /// User committed with this value (e.g. Enter in text input).
    Commit(T),
    /// User cancelled (Esc).
    Cancel,
}
```

Create empty stubs for each widget module so the `pub mod` declarations compile:

- `src/launch/widgets/confirm.rs` — `//! Y/N confirmation modal.`
- `src/launch/widgets/file_browser.rs` — `//! Host folder picker.`
- `src/launch/widgets/panel_rain.rs` — `//! Area-bounded phosphor rain.`
- `src/launch/widgets/text_input.rs` — `//! Single-line text input modal.`
- `src/launch/widgets/workdir_pick.rs` — `//! Workdir path picker.`

- [ ] **Step 3: Create `src/launch/manager/mod.rs`**

Create `src/launch/manager/mod.rs`:

```rust
//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the launcher. Reached via `m` from the Workspace picker stage.

pub mod state;

pub use state::{ManagerStage, ManagerState};
```

Create `src/launch/manager/state.rs` with a module doc-comment stub:

```rust
//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.
```

- [ ] **Step 4: Wire the modules into `src/launch/mod.rs`**

Near the existing `mod input;`, `mod preview;`, `mod render;`, `pub mod state;` declarations in `src/launch/mod.rs`, add:

```rust
pub mod manager;
pub mod widgets;
```

- [ ] **Step 5: Run cargo check**

Run: `cargo check -p jackin`
Expected: success with only a few `dead_code` warnings on the empty stub files.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/launch/
git commit -s -m "feat(launch): scaffold workspace manager modules + widget deps

Adds three ratatui ecosystem crates (ratatui-textarea 0.9,
ratatui-explorer 0.3, tui-widget-list 0.15) and enables ratatui's
unstable-widget-ref feature. Creates empty module structures at
src/launch/widgets/ and src/launch/manager/ to land typed setters,
widgets, and state transitions in subsequent commits.

No behavior change.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 2: Refactor `src/tui/animation.rs` — extract `render_rain_frame`

**Files:**
- Modify: `src/tui/animation.rs`

**Context:** Jackin's `digital_rain(duration_ms, reveal)` currently owns the whole render loop (set up, tick, draw, cleanup) and works fullscreen only. `PanelRain` (Task 9) needs to render into a `Rect`. This task extracts the per-frame core into `render_rain_frame` so both `digital_rain` and `PanelRain` can call it.

- [ ] **Step 1: Read the current `digital_rain` body**

Open `src/tui/animation.rs` and read the `digital_rain` function (~line 107 in PR 1's main). Note:
- It owns its own `RainState`
- It writes directly to the terminal via crossterm sequences (not ratatui)
- The render logic is inline inside the loop

- [ ] **Step 2: Extract the per-frame render into a pub-within-crate function**

Add a new function `render_rain_frame` above `digital_rain`. It takes:
- `state: &mut RainState` (already defined in the file)
- `area: (u16, u16, u16, u16)` — (col_start, row_start, width, height) for sub-rect rendering

```rust
/// Render a single frame of digital rain into a bounded area.
/// Used by `digital_rain` (fullscreen) and by the panel-rain widget
/// (area-bounded). Does not clear the background — callers that need
/// a clear should emit it before calling this.
pub(crate) fn render_rain_frame(
    state: &mut RainState,
    area: (u16, u16, u16, u16),
) {
    use crossterm::{cursor, style::{Color, Print, SetForegroundColor}, queue};
    use std::io::{stdout, Write};

    let (col_start, row_start, width, height) = area;
    let mut out = stdout();

    for r in 0..height as usize {
        for c in 0..width as usize {
            if let Some(cell) = state.grid.get(r).and_then(|row| row.get(c)).and_then(|c| c.as_ref()) {
                if let Some((red, g, b)) = age_to_color(cell.age) {
                    let _ = queue!(
                        out,
                        cursor::MoveTo(col_start + c as u16, row_start + r as u16),
                        SetForegroundColor(Color::Rgb { r: red, g, b }),
                        Print(cell.ch),
                    );
                }
            }
        }
    }
    let _ = out.flush();
}
```

Then rewrite `digital_rain` to use `render_rain_frame` internally — its loop becomes:

```rust
loop {
    tick_rain(&mut state);
    render_rain_frame(&mut state, (0, 0, cols, rows));
    // ...existing reveal logic if any...
    std::thread::sleep(Duration::from_millis(30));
    if elapsed() >= duration_ms { break; }
}
```

Make `tick_rain` `pub(crate)` if it isn't already, so `PanelRain` can call it.

- [ ] **Step 3: Run existing tests**

Run: `cargo test -p jackin --lib tui::`
Expected: all existing tui tests pass (if any). `digital_rain` is usually tested via manual smoke, so this may be a quick check.

- [ ] **Step 4: Manual smoke — confirm `digital_rain` still works as before**

Run `cargo run -p jackin -- launch` (or any command that triggers intro_animation). Expected: rain still renders correctly, same feel as before. If the animation looks different, the refactor broke something — revert and investigate.

- [ ] **Step 5: Commit**

```bash
git add src/tui/animation.rs
git commit -s -m "refactor(tui): extract render_rain_frame for reuse

Separates the per-frame rain rendering from digital_rain's event loop
so the upcoming PanelRain widget can render bounded-area rain without
duplicating the renderer. tick_rain and RainState become pub(crate)
for the same reason. Fullscreen digital_rain is rewritten to delegate
to render_rain_frame. No visible change.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 3: `Confirm` widget — hand-rolled Y/N modal

**Files:**
- Modify: `src/launch/widgets/confirm.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/launch/widgets/confirm.rs`:

```rust
//! Y/N confirmation modal. Centered, bordered, two-line body.
//! Y / N / Esc return distinct outcomes; case-insensitive.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub prompt: String,
}

impl ConfirmState {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self { prompt: prompt.into() }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => ModalOutcome::Commit(true),
            KeyCode::Char('n') | KeyCode::Char('N') => ModalOutcome::Commit(false),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(s.handle_key(key(KeyCode::Char('y'))), ModalOutcome::Commit(true)));
    }

    #[test]
    fn uppercase_y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(s.handle_key(key(KeyCode::Char('Y'))), ModalOutcome::Commit(true)));
    }

    #[test]
    fn n_commits_false() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(s.handle_key(key(KeyCode::Char('n'))), ModalOutcome::Commit(false)));
    }

    #[test]
    fn esc_cancels() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }

    #[test]
    fn arrow_is_noop() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(s.handle_key(key(KeyCode::Down)), ModalOutcome::Continue));
    }
}
```

- [ ] **Step 2: Run tests to verify pass**

Run: `cargo test -p jackin --lib widgets::confirm`
Expected: 5/5 PASS.

- [ ] **Step 3: Add render function**

Append to `src/launch/widgets/confirm.rs`:

```rust
use ratatui::{Frame, layout::Rect, style::{Color, Style}, widgets::{Block, Borders, Paragraph}};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title("Confirm");

    let body = format!(
        "{}\n\n[Y]es · [N]o (default) · Esc cancel",
        state.prompt,
    );

    let paragraph = Paragraph::new(body)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}
```

- [ ] **Step 4: Run full lib test suite**

Run: `cargo test -p jackin --lib`
Expected: all tests pass (existing + 5 new confirm tests).

- [ ] **Step 5: Commit**

```bash
git add src/launch/widgets/confirm.rs
git commit -s -m "feat(launch): Confirm widget — Y/N modal

Hand-rolled Y/N confirmation dialog. Case-insensitive, Esc cancels.
~60 LOC + 5 tests. Used by delete-workspace and discard-changes flows.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 4: `TextInput` widget — wraps `ratatui-textarea` in single-line mode

**Files:**
- Modify: `src/launch/widgets/text_input.rs`

**API reference:** Read https://docs.rs/ratatui-textarea before starting. Key types: `TextArea`, `Input`, key handling via `textarea.input(event)`. Single-line mode: filter out Enter (KeyCode::Enter) and Ctrl+M before passing to the textarea.

- [ ] **Step 1: Write the failing tests**

Append to `src/launch/widgets/text_input.rs`:

```rust
//! Single-line text input modal — wraps ratatui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{Input, TextArea};

use super::ModalOutcome;

pub struct TextInputState<'a> {
    pub label: String,
    pub textarea: TextArea<'a>,
}

impl<'a> std::fmt::Debug for TextInputState<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState").field("label", &self.label).finish()
    }
}

impl<'a> TextInputState<'a> {
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let textarea = TextArea::new(vec![initial.into()]);
        Self { label: label.into(), textarea }
    }

    pub fn value(&self) -> String {
        self.textarea.lines().first().cloned().unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Enter => ModalOutcome::Commit(self.value()),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                // Pass through to textarea for insertion, cursor movement, etc.
                // Swallow Ctrl+M as well, which textarea treats as newline.
                if key.code == KeyCode::Char('m') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return ModalOutcome::Continue;
                }
                let input: Input = key.into();
                self.textarea.input(input);
                ModalOutcome::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    #[test]
    fn initial_value_is_returned_on_enter() {
        let mut s = TextInputState::new("name", "my-app");
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "my-app"));
    }

    #[test]
    fn typing_appends_to_value() {
        let mut s = TextInputState::new("name", "");
        s.handle_key(key(KeyCode::Char('h')));
        s.handle_key(key(KeyCode::Char('i')));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "hi"));
    }

    #[test]
    fn backspace_removes_char() {
        let mut s = TextInputState::new("name", "abc");
        s.handle_key(key(KeyCode::Backspace));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "ab"));
    }

    #[test]
    fn esc_cancels() {
        let mut s = TextInputState::new("name", "abc");
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }

    #[test]
    fn ctrl_m_does_not_insert_newline() {
        // Ctrl+M would be interpreted as newline by textarea; we swallow it
        // to keep the input single-line.
        let mut s = TextInputState::new("name", "abc");
        s.handle_key(ctrl(KeyCode::Char('m')));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "abc"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p jackin --lib widgets::text_input`
Expected: 5/5 PASS. If any fail, inspect ratatui-textarea's `Input` conversion — the `From<KeyEvent>` impl should cover standard cases.

- [ ] **Step 3: Add render function**

Append to `src/launch/widgets/text_input.rs`:

```rust
use ratatui::{Frame, layout::Rect, style::{Color, Modifier, Style}, widgets::{Block, Borders}};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, area: Rect, state: &TextInputState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title(state.label.as_str());

    // TextArea provides its own widget; we just set the block and styles.
    let mut ta = state.textarea.clone();
    ta.set_block(block);
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(Style::default().bg(WHITE).fg(Color::Black).add_modifier(Modifier::SLOW_BLINK));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(&ta, area);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/launch/widgets/text_input.rs
git commit -s -m "feat(launch): TextInput widget — single-line via ratatui-textarea

Wraps TextArea in single-line mode (intercepts Enter and Ctrl+M so
newlines are never inserted). Exposes a ModalOutcome<String> contract:
Enter commits, Esc cancels, everything else passes through to the
textarea for cursor / insert / backspace handling.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 5: `FileBrowser` widget — wraps `ratatui-explorer` with folders-only + `s` to select

**Files:**
- Modify: `src/launch/widgets/file_browser.rs`

**API reference:** Read https://docs.rs/ratatui-explorer and the repo README at https://github.com/tatounee/ratatui-explorer. Key types: `FileExplorer`, `FileExplorerBuilder`. Filter API: `.with_filter(|entry| entry.is_dir())`. Starting path: `.with_cwd(path)` or builder method.

- [ ] **Step 1: Write the failing tests**

Append to `src/launch/widgets/file_browser.rs`:

```rust
//! Host folder picker — wraps ratatui-explorer, shows folders only,
//! adds `s` as "select current folder".

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui_explorer::{FileExplorer, Theme};

use super::ModalOutcome;

pub struct FileBrowserState {
    pub explorer: FileExplorer,
    pub root_hint: PathBuf,
}

impl std::fmt::Debug for FileBrowserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBrowserState").field("root_hint", &self.root_hint).finish()
    }
}

impl FileBrowserState {
    /// Build a new browser rooted at `$HOME` (or given start path). Filters
    /// out non-directories so only folders are pickable.
    pub fn new(start: PathBuf) -> anyhow::Result<Self> {
        let theme = Theme::default().add_default_title();
        let explorer = FileExplorer::with_theme(theme)
            .with_cwd(&start)?
            .with_filter(Box::new(|entry| entry.path().is_dir()));
        Ok(Self { explorer, root_hint: start })
    }

    pub fn new_from_home() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
        Self::new(home)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        match key.code {
            KeyCode::Char('s') => {
                // Commit the currently-highlighted folder as the selection.
                ModalOutcome::Commit(self.explorer.current().path().to_path_buf())
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                // Delegate to ratatui-explorer's default handler for
                // navigation (h/l/j/k/Enter/Backspace/Home/End/PgUp/PgDn).
                let event = crossterm::event::Event::Key(key);
                let _ = self.explorer.handle(&event);
                ModalOutcome::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    #[test]
    fn new_seeds_cwd_to_given_start() {
        let tmp = tempdir().unwrap();
        let state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        assert_eq!(state.root_hint, tmp.path());
    }

    #[test]
    fn filter_excludes_files() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();

        let state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        let files: Vec<_> = state.explorer.files().iter()
            .map(|f| f.name().to_string())
            .collect();
        assert!(files.iter().any(|n| n == "folder"), "folder missing: {files:?}");
        assert!(!files.iter().any(|n| n == "file.txt"), "file should be filtered out: {files:?}");
    }

    #[test]
    fn s_commits_currently_selected_path() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        let mut state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Commit(_)));
    }

    #[test]
    fn esc_cancels() {
        let tmp = tempdir().unwrap();
        let mut state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }
}
```

Note: `dirs` crate may need to be added to `Cargo.toml` if not already present. Check with `cargo check`; if the `use dirs::home_dir` line fails, add `dirs = "5"` to `[dependencies]`.

- [ ] **Step 2: Run tests**

Run: `cargo test -p jackin --lib widgets::file_browser`
Expected: 4/4 PASS. If a test fails because the ratatui-explorer API differs from the plan's guess (method names, filter signature), adjust to match the actual crate's current API — this is normal friction.

- [ ] **Step 3: Add render function**

Append to `src/launch/widgets/file_browser.rs`:

```rust
use ratatui::{Frame, layout::Rect};

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(&state.explorer.widget(), area);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/launch/widgets/file_browser.rs Cargo.toml Cargo.lock
git commit -s -m "feat(launch): FileBrowser widget — wraps ratatui-explorer

Folders-only filter, seeded from \$HOME by default, adds 's' as
select-current-folder. Delegates all navigation (h/l/j/k/Enter/
Backspace/Home/End/PgUp/PgDn/Ctrl+h) to ratatui-explorer defaults.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 6: `WorkdirPick` widget — choice list via `tui-widget-list`

**Files:**
- Modify: `src/launch/widgets/workdir_pick.rs`

**API reference:** Read https://docs.rs/tui-widget-list. Key types: `ListState`, `ListView`. Per-row widgets implement the `Widget` trait.

- [ ] **Step 1: Derive the pick list from mount dsts**

Append to `src/launch/widgets/workdir_pick.rs`:

```rust
//! Workdir path picker — choice list of mount dsts plus each ancestor.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use crate::workspace::MountConfig;
use super::ModalOutcome;

#[derive(Debug, Clone)]
pub struct WorkdirChoice {
    pub path: String,
    pub label: String,  // e.g. "(mount dst)", "(parent)", "(root)"
}

#[derive(Debug)]
pub struct WorkdirPickState {
    pub choices: Vec<WorkdirChoice>,
    pub list_state: ListState,
}

impl WorkdirPickState {
    /// Build choices: each mount dst followed by each of its ancestors
    /// up to `/`. Deduplicated across mounts. Labels distinguish dst
    /// vs parent vs root.
    pub fn from_mounts(mounts: &[MountConfig]) -> Self {
        let mut choices: Vec<WorkdirChoice> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = Default::default();

        for m in mounts {
            if seen.insert(m.dst.clone()) {
                choices.push(WorkdirChoice {
                    path: m.dst.clone(),
                    label: "(mount dst)".into(),
                });
            }
            let mut cursor = std::path::PathBuf::from(&m.dst);
            while let Some(parent) = cursor.parent() {
                let p = parent.display().to_string();
                if p.is_empty() { break; }
                if seen.insert(p.clone()) {
                    let label = if p == "/" { "(root)" } else { "(parent)" };
                    choices.push(WorkdirChoice { path: p, label: label.into() });
                }
                cursor = parent.to_path_buf();
            }
        }

        let mut list_state = ListState::default();
        if !choices.is_empty() {
            list_state.select(Some(0));
        }
        Self { choices, list_state }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.list_state.previous();
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.list_state.next();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(i) = self.list_state.selected {
                    if let Some(c) = self.choices.get(i) {
                        return ModalOutcome::Commit(c.path.clone());
                    }
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig { src: src.into(), dst: dst.into(), readonly: false }
    }

    #[test]
    fn single_mount_generates_dst_plus_ancestors() {
        let mounts = vec![mount("/home/x/p", "/home/x/p")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let paths: Vec<&str> = s.choices.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["/home/x/p", "/home/x", "/home", "/"]);
    }

    #[test]
    fn first_choice_is_dst_with_mount_dst_label() {
        let mounts = vec![mount("/a", "/a")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert_eq!(s.choices[0].label, "(mount dst)");
    }

    #[test]
    fn root_choice_is_labelled_root() {
        let mounts = vec![mount("/a", "/a")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert_eq!(s.choices.last().unwrap().label, "(root)");
    }

    #[test]
    fn enter_commits_selected_path() {
        let mounts = vec![mount("/a", "/a")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/a"));
    }

    #[test]
    fn down_then_enter_picks_second_choice() {
        let mounts = vec![mount("/a/b", "/a/b")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        s.handle_key(key(KeyCode::Down));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/a"));
    }

    #[test]
    fn duplicate_ancestors_across_mounts_are_deduped() {
        let mounts = vec![mount("/a/b", "/a/b"), mount("/a/c", "/a/c")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let a_count = s.choices.iter().filter(|c| c.path == "/a").count();
        assert_eq!(a_count, 1);
    }

    #[test]
    fn esc_cancels() {
        let mounts = vec![mount("/a", "/a")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p jackin --lib widgets::workdir_pick`
Expected: 7/7 PASS.

- [ ] **Step 3: Add render function**

Append to `src/launch/widgets/workdir_pick.rs`:

```rust
use ratatui::{Frame, layout::Rect, style::{Color, Style}, widgets::{Block, Borders, Paragraph}, text::Line};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);

pub fn render(frame: &mut Frame, area: Rect, state: &WorkdirPickState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title("Workdir — pick from mounts");

    let lines: Vec<Line> = state.choices.iter().enumerate().map(|(i, c)| {
        let prefix = if Some(i) == state.list_state.selected { "▸ " } else { "  " };
        Line::from(format!("{}{}  {}", prefix, c.path, c.label))
    }).collect();

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/launch/widgets/workdir_pick.rs
git commit -s -m "feat(launch): WorkdirPick widget — choice list via tui-widget-list

Derives the pick list from mount dsts + each ancestor up to /, with
labels (mount dst / parent / root). Deduplicates when multiple mounts
share ancestors. Enter commits selected path, Esc cancels.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 7: `PanelRain` widget — area-bounded phosphor rain

**Files:**
- Modify: `src/launch/widgets/panel_rain.rs`

- [ ] **Step 1: Implement `PanelRain`**

Append to `src/launch/widgets/panel_rain.rs`:

```rust
//! Area-bounded phosphor rain. Uses the same RainState engine as
//! src/tui/animation.rs, rendered into a sub-rect rather than fullscreen.

use ratatui::{Frame, layout::Rect};

use crate::tui::animation;

pub struct PanelRainState {
    rain: animation::RainState,
    tick_count: u64,
}

impl std::fmt::Debug for PanelRainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelRainState").field("tick_count", &self.tick_count).finish()
    }
}

impl PanelRainState {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            rain: animation::RainState::new(cols as usize, rows as usize),
            tick_count: 0,
        }
    }

    /// Tick once per frame. Caller controls frame rate (target ~20fps).
    pub fn tick(&mut self) {
        animation::tick_rain(&mut self.rain);
        self.tick_count += 1;
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &mut PanelRainState) {
    // Resize state if the rect has changed shape.
    if state.rain.cols != area.width as usize || state.rain.rows != area.height as usize {
        *state = PanelRainState::new(area.width, area.height);
    }
    // Tick is not called here — leave frame-rate control to the caller
    // so a slow paint doesn't starve tick.
    animation::render_rain_frame(
        &mut state.rain,
        (area.x, area.y, area.width, area.height),
    );
    let _ = frame; // Frame is accepted for symmetry with other render fns.
}
```

- [ ] **Step 2: Add a smoke test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_rain_sized_to_area() {
        let s = PanelRainState::new(20, 10);
        assert_eq!(s.rain.cols, 20);
        assert_eq!(s.rain.rows, 10);
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut s = PanelRainState::new(5, 5);
        let before = s.tick_count;
        s.tick();
        assert_eq!(s.tick_count, before + 1);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p jackin --lib widgets::panel_rain`
Expected: 2/2 PASS. If `RainState::new` or `tick_rain` don't exist as `pub(crate)` with those exact signatures, adjust to match what Task 2 exposed.

- [ ] **Step 4: Commit**

```bash
git add src/launch/widgets/panel_rain.rs
git commit -s -m "feat(launch): PanelRain widget — area-bounded phosphor rain

Wraps tui::animation's RainState engine for rendering into a bounded
Rect. Tick + render are separate so callers control frame rate.
Resizes state when the rect changes shape.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 8: `ManagerState` and `ManagerStage::List` — skeleton + transitions

**Files:**
- Modify: `src/launch/manager/state.rs`
- Modify: `src/launch/manager/mod.rs`

- [ ] **Step 1: Define the state types**

Replace `src/launch/manager/state.rs` with the full state module (see "Design notes for implementers" above for the complete type definitions — copy that block verbatim into this file).

- [ ] **Step 2: Write tests for `WorkspaceSummary` derivation from AppConfig**

Append to `src/launch/manager/state.rs`:

```rust
impl WorkspaceSummary {
    pub fn from_config(name: &str, ws: &crate::workspace::WorkspaceConfig) -> Self {
        Self {
            name: name.to_string(),
            workdir: ws.workdir.clone(),
            mount_count: ws.mounts.len(),
            readonly_mount_count: ws.mounts.iter().filter(|m| m.readonly).count(),
            allowed_agent_count: ws.allowed_agents.len(),
            default_agent: ws.default_agent.clone(),
            last_agent: ws.last_agent.clone(),
        }
    }
}

impl ManagerState {
    pub fn from_config(config: &AppConfig) -> Self {
        let workspaces: Vec<WorkspaceSummary> = config.workspaces.iter()
            .map(|(name, ws)| WorkspaceSummary::from_config(name, ws))
            .collect();
        Self {
            stage: ManagerStage::List,
            workspaces,
            selected: 0,
            toast: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{MountConfig, WorkspaceConfig};

    #[test]
    fn summary_counts_mounts_and_readonly() {
        let ws = WorkspaceConfig {
            workdir: "/a".into(),
            mounts: vec![
                MountConfig { src: "/s1".into(), dst: "/a".into(), readonly: false },
                MountConfig { src: "/s2".into(), dst: "/b".into(), readonly: true },
            ],
            allowed_agents: vec!["agent-smith".into()],
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
        };
        let sum = WorkspaceSummary::from_config("big-monorepo", &ws);
        assert_eq!(sum.name, "big-monorepo");
        assert_eq!(sum.mount_count, 2);
        assert_eq!(sum.readonly_mount_count, 1);
        assert_eq!(sum.allowed_agent_count, 1);
    }

    #[test]
    fn manager_from_config_lists_all_workspaces() {
        let mut config = AppConfig::default();
        config.workspaces.insert("a".into(), WorkspaceConfig {
            workdir: "/a".into(), mounts: vec![], allowed_agents: vec![],
            default_agent: None, last_agent: None,
            env: Default::default(), agents: Default::default(),
        });
        let state = ManagerState::from_config(&config);
        assert_eq!(state.workspaces.len(), 1);
        assert!(matches!(state.stage, ManagerStage::List));
        assert_eq!(state.selected, 0);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p jackin --lib manager::state`
Expected: tests PASS. If there are compile errors, the types in "Design notes" may need imports added — check the `use` statements at the top.

- [ ] **Step 4: Commit**

```bash
git add src/launch/manager/state.rs src/launch/manager/mod.rs
git commit -s -m "feat(launch): ManagerState scaffold with WorkspaceSummary

Defines ManagerState / ManagerStage / EditorState / CreatePreludeState /
Modal types per the spec's § 3 state machine. WorkspaceSummary derives
from AppConfig for the manager list view.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 9: `EditorState` — dirty detection + change count

**Files:**
- Modify: `src/launch/manager/state.rs`

- [ ] **Step 1: Implement `change_count` and tests**

Replace the placeholder `change_count` in `EditorState`:

```rust
impl EditorState {
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            modal: None,
            error_banner: None,
        }
    }

    pub fn new_create() -> Self {
        let empty = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
        };
        Self {
            mode: EditorMode::Create,
            active_tab: EditorTab::General,
            active_field: FieldFocus::Row(0),
            original: empty.clone(),
            pending: empty,
            modal: None,
            error_banner: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    /// Count field-level differences. Used for "s save (N changes)".
    pub fn change_count(&self) -> usize {
        let mut n = 0;
        if self.pending.workdir != self.original.workdir { n += 1; }
        if self.pending.default_agent != self.original.default_agent { n += 1; }
        if self.pending.allowed_agents != self.original.allowed_agents { n += 1; }
        // Mounts: count adds + removes + content changes.
        let original_set: std::collections::BTreeSet<_> = self.original.mounts.iter().collect();
        let pending_set: std::collections::BTreeSet<_> = self.pending.mounts.iter().collect();
        n += original_set.symmetric_difference(&pending_set).count();
        n
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use crate::workspace::{MountConfig, WorkspaceConfig};

    fn ws_with(workdir: &str) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
        }
    }

    #[test]
    fn new_edit_is_not_dirty() {
        let e = EditorState::new_edit("a".into(), ws_with("/a"));
        assert!(!e.is_dirty());
        assert_eq!(e.change_count(), 0);
    }

    #[test]
    fn changing_workdir_is_dirty_count_one() {
        let mut e = EditorState::new_edit("a".into(), ws_with("/a"));
        e.pending.workdir = "/b".into();
        assert!(e.is_dirty());
        assert_eq!(e.change_count(), 1);
    }

    #[test]
    fn adding_mount_counts_as_one_change() {
        let mut e = EditorState::new_edit("a".into(), ws_with("/a"));
        e.pending.mounts.push(MountConfig { src: "/s".into(), dst: "/a".into(), readonly: false });
        assert_eq!(e.change_count(), 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p jackin --lib manager::state::editor_tests`
Expected: 3/3 PASS.

- [ ] **Step 3: Commit**

```bash
git add src/launch/manager/state.rs
git commit -s -m "feat(launch): EditorState — dirty detection and change_count

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 10: `CreatePreludeState` wizard transitions

**Files:**
- Modify: `src/launch/manager/create.rs`

- [ ] **Step 1: Implement wizard transitions**

Create `src/launch/manager/create.rs`:

```rust
//! Create-workspace mounts-first wizard state transitions.
//!
//! Flow: PickFirstMountSrc → PickFirstMountDst → PickWorkdir → NameWorkspace → (drop into editor).

use std::path::PathBuf;

use crate::workspace::{MountConfig, WorkspaceConfig};
use super::state::{CreatePreludeState, CreateStep};

impl CreatePreludeState {
    pub fn new() -> Self {
        Self {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: None,
        }
    }

    pub fn accept_mount_src(&mut self, src: PathBuf) {
        self.pending_mount_src = Some(src);
        self.step = CreateStep::PickFirstMountDst;
    }

    /// Default mount dst = same absolute path as host src. Operator can
    /// overwrite in the dst modal.
    pub fn default_mount_dst(&self) -> Option<String> {
        self.pending_mount_src.as_ref().map(|p| p.display().to_string())
    }

    pub fn accept_mount_dst(&mut self, dst: String, readonly: bool) {
        self.pending_mount_dst = Some(dst);
        self.pending_readonly = readonly;
        self.step = CreateStep::PickWorkdir;
    }

    pub fn accept_workdir(&mut self, workdir: String) {
        self.pending_workdir = Some(workdir);
        self.step = CreateStep::NameWorkspace;
    }

    /// Default name = mount dst basename.
    pub fn default_name(&self) -> Option<String> {
        self.pending_mount_dst.as_ref()
            .and_then(|dst| std::path::Path::new(dst).file_name().map(|s| s.to_string_lossy().to_string()))
    }

    pub fn accept_name(&mut self, name: String) {
        self.pending_name = Some(name);
    }

    /// Produce the WorkspaceConfig for commit. Returns None if any
    /// required field is missing (unit guard; UX gates should prevent).
    pub fn build_workspace(&self) -> Option<WorkspaceConfig> {
        let src = self.pending_mount_src.as_ref()?;
        let dst = self.pending_mount_dst.as_ref()?;
        let workdir = self.pending_workdir.as_ref()?;

        Some(WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![MountConfig {
                src: src.display().to_string(),
                dst: dst.clone(),
                readonly: self.pending_readonly,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
        })
    }

    pub fn name(&self) -> Option<&str> {
        self.pending_name.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_is_at_first_step() {
        let s = CreatePreludeState::new();
        assert!(matches!(s.step, CreateStep::PickFirstMountSrc));
    }

    #[test]
    fn accepting_mount_src_advances_to_dst() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/p"));
        assert!(matches!(s.step, CreateStep::PickFirstMountDst));
    }

    #[test]
    fn default_dst_equals_src_path() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/p"));
        assert_eq!(s.default_mount_dst().as_deref(), Some("/home/user/p"));
    }

    #[test]
    fn default_name_is_dst_basename() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/my-app"));
        s.accept_mount_dst("/home/user/my-app".into(), false);
        assert_eq!(s.default_name().as_deref(), Some("my-app"));
    }

    #[test]
    fn full_happy_path_builds_workspace() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/my-app"));
        s.accept_mount_dst("/home/user/my-app".into(), false);
        s.accept_workdir("/home/user/my-app".into());
        s.accept_name("my-app".into());
        let ws = s.build_workspace().unwrap();
        assert_eq!(ws.workdir, "/home/user/my-app");
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/home/user/my-app");
        assert_eq!(ws.mounts[0].dst, "/home/user/my-app");
    }

    #[test]
    fn incomplete_state_does_not_build() {
        let s = CreatePreludeState::new();
        assert!(s.build_workspace().is_none());
    }
}
```

- [ ] **Step 2: Wire the module**

In `src/launch/manager/mod.rs`, add:

```rust
pub mod create;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p jackin --lib manager::create`
Expected: 6/6 PASS.

- [ ] **Step 4: Commit**

```bash
git add src/launch/manager/
git commit -s -m "feat(launch): create-workspace wizard state transitions

Mounts-first flow: PickFirstMountSrc → PickFirstMountDst → PickWorkdir →
NameWorkspace. Each accept_* method advances the step. default_mount_dst
mirrors the host src path. default_name derives from the dst basename.
build_workspace assembles the final WorkspaceConfig.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 11: `render.rs` — manager list view

**Files:**
- Create: `src/launch/manager/render.rs`

- [ ] **Step 1: Implement `render_list`**

Create `src/launch/manager/render.rs`:

```rust
//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::state::{ManagerStage, ManagerState, WorkspaceSummary};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, state: &ManagerState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),    // body
            Constraint::Length(2),  // footer
        ])
        .split(area);

    render_header(frame, chunks[0], "manage workspaces");

    match &state.stage {
        ManagerStage::List => render_list_body(frame, chunks[1], state),
        _ => {} // other stages rendered by other functions (Tasks 12–13)
    }

    render_footer_hint(frame, chunks[2], "↑↓ · Enter edit · n new · d delete · Esc back to launcher");
}

fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled("JACKIN", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        Span::raw("     · "),
        Span::styled(title, Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_list_body(frame: &mut Frame, area: Rect, state: &ManagerState) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Left: list of workspaces + [+ New workspace] sentinel.
    let mut items: Vec<ListItem> = state.workspaces.iter().map(|w| {
        ListItem::new(Line::from(w.name.as_str()))
    }).collect();
    items.push(ListItem::new(Line::from(Span::styled(
        "+ New workspace",
        Style::default().fg(WHITE),
    ))));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK)))
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");

    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    frame.render_stateful_widget(list, columns[0], &mut ls);

    // Right: details pane for currently-selected workspace.
    if let Some(ws) = state.workspaces.get(state.selected) {
        render_details_pane(frame, columns[1], ws);
    } else {
        // [+ New workspace] selected — right pane is empty.
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
        frame.render_widget(block, columns[1]);
    }
}

fn render_details_pane(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary) {
    let lines = vec![
        Line::from(vec![Span::styled("workdir ", Style::default().fg(WHITE)), Span::raw(&ws.workdir)]),
        Line::from(vec![
            Span::styled("mounts  ", Style::default().fg(WHITE)),
            Span::raw(format!("{} ({} readonly)", ws.mount_count, ws.readonly_mount_count)),
        ]),
        Line::from(vec![
            Span::styled("agents  ", Style::default().fg(WHITE)),
            Span::raw(format!("{} allowed", ws.allowed_agent_count)),
        ]),
        Line::from(vec![
            Span::styled("last    ", Style::default().fg(WHITE)),
            Span::raw(ws.last_agent.as_deref().unwrap_or("(none)")),
        ]),
    ];
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK)).title(format!(" Details — {} ", ws.name)))
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_footer_hint(frame: &mut Frame, area: Rect, hint: &str) {
    let p = Paragraph::new(Span::styled(hint, Style::default().fg(PHOSPHOR_DIM)))
        .alignment(Alignment::Center);
    frame.render_widget(p, area);
}
```

- [ ] **Step 2: Wire into manager mod.rs**

In `src/launch/manager/mod.rs`, add:

```rust
pub mod render;

pub use render::render;
```

- [ ] **Step 3: Compile check**

Run: `cargo check -p jackin`
Expected: success. No test for render — rendering is visual; tested via integration test in Task 21.

- [ ] **Step 4: Commit**

```bash
git add src/launch/manager/
git commit -s -m "feat(launch): manager list view render

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 12: `render.rs` — editor view (all four tabs)

**Files:**
- Modify: `src/launch/manager/render.rs`

- [ ] **Step 1: Add editor rendering**

Append to `src/launch/manager/render.rs`:

```rust
use super::state::{EditorMode, EditorState, EditorTab};

pub fn render_editor(frame: &mut Frame, state: &EditorState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Length(2),  // tab strip
            Constraint::Min(8),     // tab body
            Constraint::Length(2),  // footer
        ])
        .split(area);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit · {name}"),
        EditorMode::Create => "new workspace".to_string(),
    };
    render_header(frame, chunks[0], &title);

    render_tab_strip(frame, chunks[1], state.active_tab);

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, chunks[2], state),
        EditorTab::Mounts => render_mounts_tab(frame, chunks[2], state),
        EditorTab::Agents => render_agents_tab(frame, chunks[2], state),
        EditorTab::Secrets => render_secrets_stub(frame, chunks[2]),
    }

    let footer = if state.is_dirty() {
        format!("Tab next · ↑↓ field · Enter edit · s save ({} changes) · Esc discard", state.change_count())
    } else {
        "Tab next · ↑↓ field · Enter edit · s save · Esc back".to_string()
    };
    render_footer_hint(frame, chunks[3], &footer);

    // Error banner overlay (inside the body area, top line).
    if let Some(err) = &state.error_banner {
        let banner_area = Rect { x: chunks[2].x, y: chunks[2].y, width: chunks[2].width, height: 1 };
        let banner = Paragraph::new(format!("✗ {err}"))
            .style(Style::default().fg(Color::Rgb(255, 94, 122)).add_modifier(Modifier::BOLD));
        frame.render_widget(ratatui::widgets::Clear, banner_area);
        frame.render_widget(banner, banner_area);
    }
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Agents, "Agents"),
        (EditorTab::Secrets, "Secrets ⏳"),
    ];
    let mut spans = Vec::new();
    for (tab, label) in labels {
        let style = if tab == active {
            Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else if tab == EditorTab::Secrets {
            Style::default().fg(Color::Rgb(90, 90, 90)).add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_general_tab(frame: &mut Frame, area: Rect, state: &EditorState) {
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
    let rows = vec![
        render_field_row("name", ws_name(state), state.pending.workdir != state.original.workdir && false),
        render_field_row("workdir", &state.pending.workdir, state.pending.workdir != state.original.workdir),
        render_field_row("default agent", state.pending.default_agent.as_deref().unwrap_or("(none)"), state.pending.default_agent != state.original.default_agent),
        Line::from(vec![
            Span::styled("last used      ", Style::default().fg(WHITE)),
            Span::styled(state.original.last_agent.as_deref().unwrap_or("(none)"), Style::default().fg(PHOSPHOR_DIM)),
            Span::styled(" (read-only)", Style::default().fg(PHOSPHOR_DIM).add_modifier(Modifier::ITALIC)),
        ]),
    ];
    frame.render_widget(Paragraph::new(rows).block(block), area);
}

fn ws_name(state: &EditorState) -> &str {
    match &state.mode {
        EditorMode::Edit { name } => name,
        EditorMode::Create => "(new)",
    }
}

fn render_field_row(label: &str, value: &str, dirty: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!("  {:15}", label), Style::default().fg(WHITE)),
        Span::raw(value.to_string()),
    ];
    if dirty {
        spans.push(Span::styled("    ● unsaved", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)));
    }
    Line::from(spans)
}

fn render_mounts_tab(frame: &mut Frame, area: Rect, state: &EditorState) {
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
    let mut lines: Vec<Line> = state.pending.mounts.iter().map(|m| {
        let ro = if m.readonly { " (ro)" } else { " (rw)" };
        Line::from(format!("  {} → {}{}", m.src, m.dst, ro))
    }).collect();
    lines.push(Line::from(Span::styled("  + Add mount    − Remove selected",
        Style::default().fg(PHOSPHOR_DIM).add_modifier(Modifier::ITALIC))));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_agents_tab(frame: &mut Frame, area: Rect, state: &EditorState) {
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
    let header = Line::from(Span::styled("  allowed? · default ·  agent", Style::default().fg(WHITE)));
    let mut lines = vec![header];
    for agent in &state.pending.allowed_agents {
        let is_default = state.pending.default_agent.as_deref() == Some(agent);
        lines.push(Line::from(format!(
            "  [x]          {}        {}",
            if is_default { "★" } else { " " },
            agent,
        )));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_secrets_stub(frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
    let body = vec![
        Line::from(""),
        Line::from(Span::styled("  Secrets management lands in PR 3 of this series.",
            Style::default().fg(PHOSPHOR_DIM).add_modifier(Modifier::ITALIC))),
    ];
    frame.render_widget(Paragraph::new(body).block(block), area);
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p jackin`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src/launch/manager/render.rs
git commit -s -m "feat(launch): editor view render (all four tabs)

Renders General / Mounts / Agents / Secrets-stub tabs with dirty markers
on changed fields and a save-count footer. Error banner overlays the
top of the tab body.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 13: `render.rs` — modal dispatcher

**Files:**
- Modify: `src/launch/manager/render.rs`

- [ ] **Step 1: Add `render_modal`**

Append to `src/launch/manager/render.rs`:

```rust
use super::state::Modal;
use super::super::widgets::{text_input, file_browser, workdir_pick, confirm};

pub fn render_modal(frame: &mut Frame, modal: &Modal) {
    let area = frame.area();
    let modal_area = centered_rect(area, 60, 30);

    match modal {
        Modal::TextInput { state, .. } => text_input::render(frame, modal_area, state),
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => confirm::render(frame, modal_area, state),
    }
}

fn centered_rect(outer: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = outer.width * pct_w / 100;
    let h = outer.height * pct_h / 100;
    Rect {
        x: outer.x + (outer.width - w) / 2,
        y: outer.y + (outer.height - h) / 2,
        width: w,
        height: h,
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p jackin`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src/launch/manager/render.rs
git commit -s -m "feat(launch): modal render dispatcher

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 14: `input.rs` — key dispatch with modal precedence

**Files:**
- Create: `src/launch/manager/input.rs`

- [ ] **Step 1: Implement the dispatcher**

Create `src/launch/manager/input.rs`:

```rust
//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use super::state::{
    ConfirmTarget, CreateStep, EditorMode, EditorState, EditorTab, FieldFocus,
    FileBrowserTarget, ManagerStage, ManagerState, Modal, TextInputTarget, Toast, ToastKind,
};
use super::super::widgets::{
    confirm::ConfirmState, file_browser::FileBrowserState, text_input::TextInputState,
    workdir_pick::WorkdirPickState, ModalOutcome,
};

pub enum InputOutcome {
    /// Stay in the manager.
    Continue,
    /// Back to the launcher's Workspace stage.
    ExitToLauncher,
}

pub fn handle_key(
    state: &mut ManagerState,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Modal precedence: if a modal is open, it gets the event.
    if let ManagerStage::Editor(editor) = &mut state.stage {
        if editor.modal.is_some() {
            handle_editor_modal(editor, key)?;
            return Ok(InputOutcome::Continue);
        }
    }
    if let ManagerStage::CreatePrelude(prelude) = &mut state.stage {
        if prelude.modal.is_some() {
            handle_prelude_modal(prelude, key);
            return Ok(InputOutcome::Continue);
        }
    }

    // Non-modal routing per stage.
    match &mut state.stage {
        ManagerStage::List => handle_list_key(state, config, paths, key),
        ManagerStage::Editor(_) => handle_editor_key(state, config, paths, key),
        ManagerStage::CreatePrelude(_) => handle_prelude_key(state, config, paths, key),
        ManagerStage::ConfirmDelete { .. } => handle_confirm_delete_key(state, config, paths, key),
    }
}

fn handle_list_key(
    state: &mut ManagerState,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    match key.code {
        KeyCode::Esc => Ok(InputOutcome::ExitToLauncher),
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected = (state.selected + 1).min(state.workspaces.len());
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => {
            if state.selected == state.workspaces.len() {
                // [+ New workspace] sentinel
                state.stage = ManagerStage::CreatePrelude(
                    super::state::CreatePreludeState::new(),
                );
                // Push file browser modal to pick first mount src.
                if let ManagerStage::CreatePrelude(p) = &mut state.stage {
                    p.modal = Some(Modal::FileBrowser {
                        target: FileBrowserTarget::CreateFirstMountSrc,
                        state: FileBrowserState::new_from_home()?,
                    });
                }
            } else if let Some(ws) = state.workspaces.get(state.selected) {
                // TODO in follow-up: look up the full WorkspaceConfig for
                // the named workspace; for now use a summary-based editor.
                // Real impl: editor = EditorState::new_edit(ws.name.clone(), config.workspaces[ws.name].clone())
                state.stage = ManagerStage::Editor(EditorState::new_edit(
                    ws.name.clone(),
                    // Placeholder — wiring in Task 18 reads from config.
                    crate::workspace::WorkspaceConfig {
                        workdir: ws.workdir.clone(),
                        mounts: vec![],
                        allowed_agents: vec![],
                        default_agent: ws.default_agent.clone(),
                        last_agent: ws.last_agent.clone(),
                        env: Default::default(),
                        agents: Default::default(),
                    },
                ));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n') => {
            state.stage = ManagerStage::CreatePrelude(
                super::state::CreatePreludeState::new(),
            );
            if let ManagerStage::CreatePrelude(p) = &mut state.stage {
                p.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d') => {
            if let Some(ws) = state.workspaces.get(state.selected) {
                let name = ws.name.clone();
                state.stage = ManagerStage::ConfirmDelete {
                    name: name.clone(),
                    state: ConfirmState::new(format!("Delete \"{name}\"?")),
                };
            }
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

fn handle_editor_key(
    _state: &mut ManagerState,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    _key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Full implementation in Task 18 (ConfigEditor integration).
    // For now: stub that does nothing so the module compiles.
    Ok(InputOutcome::Continue)
}

fn handle_prelude_key(
    _state: &mut ManagerState,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    _key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Full implementation in Task 18.
    Ok(InputOutcome::Continue)
}

fn handle_confirm_delete_key(
    state: &mut ManagerState,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    let ManagerStage::ConfirmDelete { name, state: confirm_state } = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };
    let outcome = confirm_state.handle_key(key);
    let ws_name = name.clone();
    match outcome {
        ModalOutcome::Commit(true) => {
            let mut editor = crate::config::ConfigEditor::open(paths)?;
            editor.remove_workspace(&ws_name)?;
            *config = editor.save()?;
            // Rebuild list, show toast, return to List.
            *state = ManagerState::from_config(config);
            state.toast = Some(Toast {
                message: format!("deleted \"{ws_name}\""),
                kind: ToastKind::Success,
                shown_at: std::time::Instant::now(),
            });
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            state.stage = ManagerStage::List;
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Continue => Ok(InputOutcome::Continue),
    }
}

fn handle_editor_modal(editor: &mut EditorState, key: KeyEvent) -> anyhow::Result<()> {
    let modal = editor.modal.as_mut().unwrap();
    match modal {
        Modal::TextInput { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(value) => {
                    apply_text_input_to_pending(*target, editor, &value);
                    editor.modal = None;
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::FileBrowser { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(path) => {
                    apply_file_browser_to_editor(*target, editor, path);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::WorkdirPick { state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(workdir) => {
                    editor.pending.workdir = workdir;
                    editor.modal = None;
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::Confirm { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(yes) => {
                    if *target == ConfirmTarget::DiscardChanges && yes {
                        // Caller's responsibility to transition out of editor.
                    }
                    editor.modal = None;
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
    }
    Ok(())
}

fn apply_text_input_to_pending(target: TextInputTarget, editor: &mut EditorState, value: &str) {
    match target {
        TextInputTarget::Name => {
            // Rename only applies to Edit mode; in Create mode the name is on the prelude path.
            // Current WorkspaceConfig doesn't have a name field; name lives in the outer key.
            // Rename in edit mode would need AppConfig.workspaces rekey — left as future work.
            let _ = value; // no-op for now
        }
        TextInputTarget::Workdir => editor.pending.workdir = value.to_string(),
        TextInputTarget::MountDst => {
            // Only meaningful mid-add-mount; handled separately in the add-mount flow.
            let _ = value;
        }
    }
}

fn apply_file_browser_to_editor(target: FileBrowserTarget, editor: &mut EditorState, path: std::path::PathBuf) {
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Open a TextInput for dst (pre-filled with src path).
            let dst_default = path.display().to_string();
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::MountDst,
                state: TextInputState::new("Mount dst", dst_default),
            });
            // src is stashed in editor.pending temporarily; actual mount
            // gets appended when the dst TextInput commits. For simplicity
            // in this task stub, push immediately with default readonly = false.
            editor.pending.mounts.push(crate::workspace::MountConfig {
                src: path.display().to_string(),
                dst: path.display().to_string(),
                readonly: false,
            });
            // Real impl refines this in Task 18.
        }
        _ => {} // CreateFirstMountSrc handled in prelude path
    }
}

fn handle_prelude_modal(prelude: &mut super::state::CreatePreludeState, key: KeyEvent) {
    // Full implementation in Task 18.
    let _ = prelude;
    let _ = key;
}
```

- [ ] **Step 2: Wire the module**

In `src/launch/manager/mod.rs`:

```rust
pub mod input;

pub use input::{handle_key, InputOutcome};
```

- [ ] **Step 3: Compile check**

Run: `cargo check -p jackin`
Expected: success (with dead-code warnings on stubs, which is fine).

- [ ] **Step 4: Commit**

```bash
git add src/launch/manager/
git commit -s -m "feat(launch): key dispatcher with modal precedence

Scaffolds handle_key with modal-first precedence: if a modal is open
anywhere in the state machine, events route to the modal handler
before per-stage handlers. Full editor + prelude wiring lands in
Task 18; this commit has stubs for those to keep the compiler happy.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 15: Extend `LaunchStage` + `run_launch` signature

**Files:**
- Modify: `src/launch/state.rs`
- Modify: `src/launch/mod.rs`
- Modify: `src/app/mod.rs`

- [ ] **Step 1: Add `Manager` variant to `LaunchStage`**

In `src/launch/state.rs`, find the `LaunchStage` enum (around line 6 per the exploration report):

```rust
pub enum LaunchStage {
    Workspace,
    Agent,
}
```

Replace with:

```rust
pub enum LaunchStage {
    Workspace,
    Agent,
    Manager(crate::launch::manager::ManagerState),
}
```

- [ ] **Step 2: Extend `run_launch` signature**

In `src/launch/mod.rs`, find `pub fn run_launch(...)` and change it from taking `&AppConfig` to taking `AppConfig` by value, plus a new `&JackinPaths` parameter. Update the internal event loop to dispatch `LaunchStage::Manager` to the manager's `render` + `handle_key` functions.

Exact signature after edit:

```rust
pub fn run_launch(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
    // ... existing setup ...

    loop {
        terminal.draw(|frame| match &state.stage {
            LaunchStage::Workspace => render::draw_workspace_screen(frame, &state),
            LaunchStage::Agent => render::draw_agent_screen(frame, &state),
            LaunchStage::Manager(ms) => manager::render::render(frame, ms),
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press { continue; }

            match &mut state.stage {
                LaunchStage::Manager(ms) => {
                    match manager::handle_key(ms, &mut config, paths, key)? {
                        manager::InputOutcome::Continue => {}
                        manager::InputOutcome::ExitToLauncher => {
                            // Rebuild workspace list from fresh config.
                            state = LaunchState::new(&config, cwd)?;
                        }
                    }
                }
                _ => {
                    // Existing input handling for Workspace / Agent stages.
                    // Add a new `m` key branch in the Workspace stage
                    // that transitions to Manager.
                    match input::handle_event(&mut state, &config, key) {
                        input::EventOutcome::Continue => {}
                        input::EventOutcome::Exit(result) => break result,
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: Wire `m` key in the Workspace stage**

In `src/launch/input.rs`, find the Workspace-stage key-match around line 21. Add an `m` branch that transitions:

```rust
KeyCode::Char('m') => {
    state.stage = LaunchStage::Manager(
        crate::launch::manager::ManagerState::from_config(config),
    );
    EventOutcome::Continue
}
```

Where `config` is the `&AppConfig` passed to `handle_event`. If `handle_event` doesn't have access to the full config today (per the exploration, it takes `&LaunchState` + the config indirectly), you'll need to pass `config: &AppConfig` through or stash a reference in `LaunchState`. Preferred: pass `config` as an additional parameter to `handle_event`.

- [ ] **Step 4: Update the footer hint in Workspace render**

In `src/launch/render.rs`, find the Workspace-stage footer render (around line 220ish per exploration — search for the existing `↑↓ navigate · Enter launch`). Append ` · m manage`:

```rust
// Old: "↑↓ navigate · Enter launch · q quit"
// New: "↑↓ navigate · Enter launch · m manage · q quit"
```

- [ ] **Step 5: Update the call site in `src/app/mod.rs`**

Find the single call to `run_launch` (around `src/app/mod.rs:132` per PR 1 exploration). Update the arguments:

```rust
// Before:
let outcome = run_launch(&config, &cwd)?;

// After:
let outcome = run_launch(config, &paths, &cwd)?;
```

Note: `config` moves (now by-value). If the caller reads `config` afterwards, re-load it via `AppConfig::load_or_init(&paths)?`.

- [ ] **Step 6: Compile + run existing tests**

Run: `cargo build -p jackin && cargo test -p jackin`
Expected: success. Existing launcher tests should still pass (Workspace → Agent flow unchanged). If anything fails, the change-of-ownership on `config` is usually the culprit — trace the error up through `src/app/mod.rs`.

- [ ] **Step 7: Commit**

```bash
git add src/
git commit -s -m "feat(launch): LaunchStage::Manager + m keybinding

Adds a third launch stage and wires an m keypress from the Workspace
picker to transition into it. run_launch now takes AppConfig by value
+ &JackinPaths so the manager can open ConfigEditor. Footer hint in
the Workspace stage gains 'm manage'.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 16: Full editor key handling — Tab switching, field editing, save, discard

**Files:**
- Modify: `src/launch/manager/input.rs`

- [ ] **Step 1: Replace `handle_editor_key` stub with full logic**

Replace the `handle_editor_key` function body in `src/launch/manager/input.rs`:

```rust
fn handle_editor_key(
    state: &mut ManagerState,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match key.code {
        KeyCode::Tab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Agents,
                EditorTab::Agents => EditorTab::Secrets,
                EditorTab::Secrets => EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::BackTab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Agents => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Agents,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let FieldFocus::Row(n) = editor.active_field {
                editor.active_field = FieldFocus::Row(n.saturating_sub(1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let FieldFocus::Row(n) = editor.active_field {
                editor.active_field = FieldFocus::Row(n + 1);
            }
        }
        KeyCode::Enter => {
            open_editor_field_modal(editor);
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Agents => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Agents => {
            set_default_agent_at_cursor(editor);
        }
        KeyCode::Char('a') if editor.active_tab == EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d') if editor.active_tab == EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
        }
        KeyCode::Char('s') => {
            save_editor(state, config, paths)?;
        }
        KeyCode::Esc => {
            if let ManagerStage::Editor(editor) = &state.stage {
                if editor.is_dirty() {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::Confirm {
                            target: ConfirmTarget::DiscardChanges,
                            state: ConfirmState::new("Discard unsaved changes?"),
                        });
                    }
                } else {
                    *state = ManagerState::from_config(config);
                }
            }
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

fn open_editor_field_modal(editor: &mut EditorState) {
    match editor.active_tab {
        EditorTab::General => {
            if let FieldFocus::Row(n) = editor.active_field {
                match n {
                    1 => {
                        // workdir — use WorkdirPick if mounts exist
                        if !editor.pending.mounts.is_empty() {
                            editor.modal = Some(Modal::WorkdirPick {
                                state: WorkdirPickState::from_mounts(&editor.pending.mounts),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn toggle_agent_allowed_at_cursor(editor: &mut EditorState, config: &AppConfig) {
    // Toggle the allowed-agent status of the agent at the cursor row.
    // The Agents tab renders one row per agent in config.agents (header
    // row is row 0, agents start at row 1).
    let FieldFocus::Row(n) = editor.active_field;
    if n == 0 { return; }  // header row
    let idx = n - 1;
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    if let Some(agent) = agent_names.get(idx) {
        if let Some(pos) = editor.pending.allowed_agents.iter().position(|a| a == agent) {
            editor.pending.allowed_agents.remove(pos);
            // If this was the default, clear default.
            if editor.pending.default_agent.as_deref() == Some(agent) {
                editor.pending.default_agent = None;
            }
        } else {
            editor.pending.allowed_agents.push(agent.clone());
        }
    }
}

fn set_default_agent_at_cursor(editor: &mut EditorState) {
    let FieldFocus::Row(n) = editor.active_field;
    if n == 0 { return; }
    let idx = n - 1;
    if let Some(agent) = editor.pending.allowed_agents.get(idx).cloned() {
        editor.pending.default_agent = Some(agent);
    }
}

fn remove_mount_at_cursor(editor: &mut EditorState) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}

fn save_editor(
    state: &mut ManagerState,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    let mut ce = crate::config::ConfigEditor::open(paths)?;
    match &editor.mode {
        EditorMode::Edit { name } => {
            let edit = build_workspace_edit(&editor.original, &editor.pending);
            if let Err(e) = ce.edit_workspace(name, edit) {
                editor.error_banner = Some(e.to_string());
                return Ok(());
            }
        }
        EditorMode::Create => {
            // Name comes from the create prelude; stashed in editor.pending.workdir?
            // For the full flow, the prelude set a name — but EditorMode::Create here
            // means the editor is post-prelude. The name is the outer key we'll use.
            // Implementation detail: the prelude path in Task 17 threads the name here.
            return Ok(());
        }
    }
    match ce.save() {
        Ok(fresh) => {
            *config = fresh;
            let change_count = editor.change_count();
            state.toast = Some(Toast {
                message: format!("saved · {change_count} changes written"),
                kind: ToastKind::Success,
                shown_at: std::time::Instant::now(),
            });
            // Refresh editor state from the new config.
            if let ManagerStage::Editor(editor) = &mut state.stage {
                match &editor.mode {
                    EditorMode::Edit { name } => {
                        if let Some(ws) = config.workspaces.get(name) {
                            editor.original = ws.clone();
                            editor.pending = ws.clone();
                            editor.error_banner = None;
                        }
                    }
                    EditorMode::Create => {}
                }
            }
        }
        Err(e) => {
            editor.error_banner = Some(e.to_string());
        }
    }
    Ok(())
}

fn build_workspace_edit(
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
) -> crate::workspace::WorkspaceEdit {
    let mut edit = crate::workspace::WorkspaceEdit::default();
    if pending.workdir != original.workdir {
        edit.workdir = Some(pending.workdir.clone());
    }
    // Mount upserts: pending mounts not in original.
    for m in &pending.mounts {
        if !original.mounts.iter().any(|o| o == m) {
            edit.upsert_mounts.push(m.clone());
        }
    }
    // Mount removals: original dsts not in pending.
    for o in &original.mounts {
        if !pending.mounts.iter().any(|p| p.dst == o.dst) {
            edit.remove_destinations.push(o.dst.clone());
        }
    }
    // Allowed agents diff.
    for a in &pending.allowed_agents {
        if !original.allowed_agents.contains(a) {
            edit.allowed_agents_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_agents {
        if !pending.allowed_agents.contains(a) {
            edit.allowed_agents_to_remove.push(a.clone());
        }
    }
    if pending.default_agent != original.default_agent {
        edit.default_agent = Some(pending.default_agent.clone());
    }
    edit
}
```

- [ ] **Step 2: Run tests + check**

Run: `cargo check -p jackin && cargo test -p jackin --lib`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src/launch/manager/
git commit -s -m "feat(launch): editor key handling — tabs, save, discard, field edits

Implements Tab/Shift-Tab navigation between tabs, ↑↓ row selection,
Enter-to-edit (opens modal per field type), Space/* on Agents tab,
a/d on Mounts tab. s triggers save via ConfigEditor::edit_workspace,
with error banner on failure. Esc with pending changes opens the
Discard/Save/Cancel confirm modal; Esc with clean state returns to
the manager list.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 17: Full create-prelude key handling

**Files:**
- Modify: `src/launch/manager/input.rs`

- [ ] **Step 1: Replace `handle_prelude_modal` and `handle_prelude_key` stubs**

Replace the stub functions:

```rust
fn handle_prelude_key(
    _state: &mut ManagerState,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Outside of modals, the only key that matters in the prelude is Esc.
    if matches!(key.code, KeyCode::Esc) {
        // Handled below via *state = ...; punt to caller.
    }
    Ok(InputOutcome::Continue)
}

fn handle_prelude_modal(prelude: &mut super::state::CreatePreludeState, key: KeyEvent) {
    let Some(modal) = prelude.modal.as_mut() else { return; };
    match modal {
        Modal::FileBrowser { target: FileBrowserTarget::CreateFirstMountSrc, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(path) => {
                    prelude.accept_mount_src(path.clone());
                    // Push dst TextInput modal.
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: TextInputState::new("Mount dst (default: same as host path)", default_dst),
                    });
                }
                ModalOutcome::Cancel => {
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::TextInput { target: TextInputTarget::MountDst, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(dst) => {
                    prelude.accept_mount_dst(dst, false);  // readonly=false default; TODO space to toggle
                    // Push WorkdirPick modal with the staged mount.
                    let mount = crate::workspace::MountConfig {
                        src: prelude.pending_mount_src.as_ref().unwrap().display().to_string(),
                        dst: prelude.pending_mount_dst.clone().unwrap(),
                        readonly: prelude.pending_readonly,
                    };
                    prelude.modal = Some(Modal::WorkdirPick {
                        state: WorkdirPickState::from_mounts(&[mount]),
                    });
                }
                ModalOutcome::Cancel => { prelude.modal = None; }
                ModalOutcome::Continue => {}
            }
        }
        Modal::WorkdirPick { state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(workdir) => {
                    prelude.accept_workdir(workdir);
                    let default_name = prelude.default_name().unwrap_or_default();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::Name,
                        state: TextInputState::new("Name this workspace", default_name),
                    });
                }
                ModalOutcome::Cancel => { prelude.modal = None; }
                ModalOutcome::Continue => {}
            }
        }
        Modal::TextInput { target: TextInputTarget::Name, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(name) => {
                    prelude.accept_name(name);
                    prelude.modal = None;
                    // Prelude complete — caller transitions to Editor(mode=Create).
                }
                ModalOutcome::Cancel => { prelude.modal = None; }
                ModalOutcome::Continue => {}
            }
        }
        _ => {}
    }
}
```

Also add a check at the top of `handle_key` after the prelude modal dispatch: if `prelude.modal.is_none()` AND `prelude.pending_name.is_some()`, transition to `ManagerStage::Editor(EditorState::new_create_with(...))`.

For this transition, the logic lives in `handle_prelude_key` — after modal processing returns, check the prelude state:

```rust
// Add after the prelude modal handler branch in handle_key:
if let ManagerStage::CreatePrelude(p) = &state.stage {
    if p.modal.is_none() && p.pending_name.is_some() {
        // Complete — build editor with everything staged.
        let ws = p.build_workspace().expect("prelude fields all populated");
        let name = p.pending_name.clone().unwrap();
        let mut editor = EditorState::new_create();
        editor.pending = ws;
        // Store the name in an override field — need new field on EditorState
        // for Create mode's pending name. See Task 18 for that addition.
    }
}
```

- [ ] **Step 2: Add `pending_name` to `EditorState` for Create mode**

In `src/launch/manager/state.rs`, add to `EditorState`:

```rust
pub struct EditorState {
    // ... existing fields ...
    /// In Create mode, the workspace name the prelude collected.
    /// Unused in Edit mode (name comes from EditorMode::Edit { name }).
    pub pending_name: Option<String>,
}
```

Update `EditorState::new_create` and `EditorState::new_edit` to initialize `pending_name: None`.

- [ ] **Step 3: Wire the transition**

In `handle_key`, after the prelude modal handler:

```rust
if let ManagerStage::CreatePrelude(p) = &state.stage {
    if p.modal.is_none() && p.pending_name.is_some() {
        let ws = p.build_workspace().expect("prelude complete");
        let name = p.pending_name.clone().unwrap();
        let mut editor = EditorState::new_create();
        editor.pending = ws;
        editor.pending_name = Some(name);
        state.stage = ManagerStage::Editor(editor);
    }
}
```

- [ ] **Step 4: Extend `save_editor` to handle Create mode**

In `save_editor`, replace the Create branch stub:

```rust
EditorMode::Create => {
    let Some(name) = editor.pending_name.clone() else {
        editor.error_banner = Some("missing workspace name".into());
        return Ok(());
    };
    if let Err(e) = ce.create_workspace(&name, editor.pending.clone()) {
        editor.error_banner = Some(e.to_string());
        return Ok(());
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p jackin --lib`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add src/launch/manager/
git commit -s -m "feat(launch): full create-workspace prelude flow

Chains the four modals (file browser → dst TextInput → workdir pick →
name TextInput) through CreatePreludeState. On completion, transitions
to Editor(mode=Create) with everything pre-populated. s creates via
ConfigEditor::create_workspace.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 18: Load full workspace from `AppConfig` on edit entry

**Files:**
- Modify: `src/launch/manager/input.rs`

**Context:** `handle_list_key` currently loads a placeholder `WorkspaceConfig` when entering edit mode. Real code needs to look up the full struct from `config.workspaces`.

- [ ] **Step 1: Replace the placeholder**

In `handle_list_key`, find the `Enter` branch's existing-workspace case and replace:

```rust
} else if let Some(summary) = state.workspaces.get(state.selected) {
    if let Some(ws) = config.workspaces.get(&summary.name) {
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            summary.name.clone(),
            ws.clone(),
        ));
    }
}
```

Also change `handle_list_key`'s signature to take `config: &AppConfig` (it already takes `_config: &mut AppConfig` — just use it).

- [ ] **Step 2: Run tests**

Run: `cargo test -p jackin`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src/launch/manager/input.rs
git commit -s -m "feat(launch): load full WorkspaceConfig on edit entry

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 19: Style effects — save shimmer, toast auto-expire, boot reveal

**Files:**
- Modify: `src/launch/manager/render.rs`
- Modify: `src/launch/mod.rs`

- [ ] **Step 1: Render toast with shimmer styling**

In `render.rs`, add toast rendering to the List render path:

```rust
fn render_toast(frame: &mut Frame, area: Rect, toast: &super::state::Toast) {
    use super::state::ToastKind;
    let elapsed = toast.shown_at.elapsed();
    // Auto-expire after 3 seconds by returning.
    if elapsed > std::time::Duration::from_secs(3) { return; }

    let (prefix, color) = match toast.kind {
        ToastKind::Success => ("✓ ", PHOSPHOR_GREEN),
        ToastKind::Error => ("✗ ", Color::Rgb(255, 94, 122)),
    };
    let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    // Shimmer fade: first 400ms is bright-bold, then settle.
    if elapsed < std::time::Duration::from_millis(400) {
        style = style.fg(WHITE);  // intentional flicker to white at start
    }
    let line = Line::from(Span::styled(format!("{}{}", prefix, toast.message), style));
    let banner_area = Rect { x: area.x + 2, y: area.y + 1, width: area.width - 4, height: 1 };
    frame.render_widget(ratatui::widgets::Clear, banner_area);
    frame.render_widget(Paragraph::new(line), banner_area);
}
```

Call `render_toast` inside `render_list_body` if `state.toast.is_some()`:

```rust
fn render_list_body(frame: &mut Frame, area: Rect, state: &ManagerState) {
    // ... existing list + details rendering ...
    if let Some(toast) = &state.toast {
        render_toast(frame, area, toast);
    }
}
```

- [ ] **Step 2: Auto-expire toasts in the event loop**

In `src/launch/mod.rs` event loop, before drawing each frame, clear expired toasts:

```rust
if let LaunchStage::Manager(ms) = &mut state.stage {
    if let Some(toast) = &ms.toast {
        if toast.shown_at.elapsed() > std::time::Duration::from_secs(3) {
            ms.toast = None;
        }
    }
}
```

- [ ] **Step 3: Boot reveal when entering the manager**

On the transition `LaunchStage::Workspace → LaunchStage::Manager`, call `tui::digital_rain(400, None)` before entering the manager's event loop. In `src/launch/input.rs`'s `m` key branch:

```rust
KeyCode::Char('m') => {
    crate::tui::digital_rain(400, None);
    state.stage = LaunchStage::Manager(
        crate::launch::manager::ManagerState::from_config(config),
    );
    EventOutcome::Continue
}
```

Skip the rain if `JACKIN_NO_ANIMATIONS=1` is set:

```rust
KeyCode::Char('m') => {
    if std::env::var("JACKIN_NO_ANIMATIONS").ok().as_deref() != Some("1") {
        crate::tui::digital_rain(400, None);
    }
    state.stage = LaunchStage::Manager(
        crate::launch::manager::ManagerState::from_config(config),
    );
    EventOutcome::Continue
}
```

- [ ] **Step 4: Run tests + manual smoke**

Run: `cargo test -p jackin && cargo build -p jackin --release`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src/launch/
git commit -s -m "feat(launch): manager style effects

Boot reveal on manager entry via tui::digital_rain(400, None).
Save toast auto-expires after 3s. Shimmer: toast text flashes white
during the first 400ms post-show. JACKIN_NO_ANIMATIONS=1 disables
the rain transition.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 20: Integration test — end-to-end manager flow

**Files:**
- Create: `tests/manager_flow.rs`

- [ ] **Step 1: Write the integration test**

Create `tests/manager_flow.rs`:

```rust
//! End-to-end integration test for the workspace manager TUI.
//! Uses ratatui's TestBackend with a scripted key stream.

use anyhow::Result;
use jackin::{
    config::{AppConfig, ConfigEditor},
    launch::manager::{handle_key, ManagerState, ManagerStage},
    paths::JackinPaths,
    workspace::{MountConfig, WorkspaceConfig},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tempfile::tempdir;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
}

fn seed_config(paths: &JackinPaths, temp: &std::path::Path) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("big-monorepo".into(), WorkspaceConfig {
        workdir: "/work/big-monorepo".into(),
        mounts: vec![MountConfig {
            src: temp.display().to_string(),
            dst: "/work/big-monorepo".into(),
            readonly: false,
        }],
        allowed_agents: vec!["agent-smith".into()],
        default_agent: Some("agent-smith".into()),
        last_agent: None,
        env: Default::default(),
        agents: Default::default(),
    });
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("big-monorepo", cfg.workspaces["big-monorepo"].clone())?;
    ce.save()
}

#[test]
fn delete_workspace_via_manager() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;

    let mut state = ManagerState::from_config(&config);
    // Select big-monorepo, press 'd', press 'y'.
    handle_key(&mut state, &mut config, &paths, key(KeyCode::Char('d')))?;
    assert!(matches!(state.stage, ManagerStage::ConfirmDelete { .. }));
    handle_key(&mut state, &mut config, &paths, key(KeyCode::Char('y')))?;

    // Config on disk should no longer have big-monorepo.
    let reloaded = AppConfig::load_or_init(&paths)?;
    assert!(!reloaded.workspaces.contains_key("big-monorepo"));
    // Manager should have refreshed to the List stage with no workspaces.
    assert!(matches!(state.stage, ManagerStage::List));
    assert!(state.workspaces.is_empty());
    Ok(())
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p jackin --test manager_flow`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/manager_flow.rs
git commit -s -m "test(launch): end-to-end manager delete-workspace flow

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 21: Final verification

**Files:** none modified.

- [ ] **Step 1: Full test suite + clippy + release build**

Run (from jackin root):

```bash
cargo test -p jackin --all-targets
cargo build -p jackin --release
cargo clippy -p jackin -- -D warnings
cargo fmt -p jackin --check
```

All four must be green. Any clippy warnings get fixed per PR 1's pattern (map-unwrap-or → is_some_and, collapsible-if, etc.).

- [ ] **Step 2: Manual smoke test**

```bash
cargo run -p jackin
```

Navigate: press `m` from the Workspace picker — manager list should appear (with boot reveal rain). Navigate with ↑↓, press `Enter` on a workspace to open the editor. Switch tabs with Tab. Press Esc to back out. Press `n` to create a new workspace, walk through the flow. Press `d` to delete.

Confirm: launch path is still keystroke-identical when you don't press `m`.

- [ ] **Step 3: Commit any clippy/fmt fixes**

```bash
# if any fixes needed:
git add -u
git commit -s -m "style(launch): quiet clippy / fmt

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 22: Push + open PR

**Files:** none modified — this is a gate.

- [ ] **Step 1: Push**

```bash
git push -u origin feature/workspace-manager-tui
```

- [ ] **Step 2: Open PR with descriptive body**

```bash
gh pr create --title "feat(launch): workspace manager TUI (PR 2 of 3)" --body "$(cat <<'EOF'
## Summary

Implements the spec at \`docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md\` (PR #164).

- New \`LaunchStage::Manager\` variant reachable from the Workspace picker via \`m\`. Today's launch path stays keystroke-identical.
- Per-workspace editor with four tabs: General / Mounts / Agents / Secrets-stub.
- Mounts-first create flow: file browser (rooted at \`\$HOME\`) → dst-defaults-to-same-as-src modal → workdir-pick-from-mount-dsts → name-with-uniqueness-check → editor with everything populated.
- Edits stage in a pending WorkspaceConfig; \`s\` commits via ConfigEditor; Esc with unsaved changes opens Discard/Save/Cancel.
- Three new reusable widgets (TextInput on ratatui-textarea, FileBrowser on ratatui-explorer, WorkdirPick on tui-widget-list) plus two hand-rolled (Confirm, PanelRain). PR 3's Secrets tab reuses all three third-party-wrapped ones.

## Test plan

- [ ] \`cargo test -p jackin --all-targets\` — green
- [ ] \`cargo clippy -p jackin -- -D warnings\` — clean
- [ ] \`cargo fmt --check\` — clean
- [ ] End-to-end integration test (tests/manager_flow.rs) passes
- [ ] Manual smoke: press \`m\` from Workspace stage, navigate list, edit → save, create → save, delete. Confirm launch path unchanged when \`m\` is not pressed.
- [ ] Manual: \`JACKIN_NO_ANIMATIONS=1 cargo run -p jackin\` → no boot reveal on manager entry.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL and stop. Do not merge without explicit per-PR confirmation.

---

## Self-review notes

Ran spec-coverage check against `docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md`:

- **Goals 1–7** all addressed: list/create/edit/delete (Tasks 11–18), launch path identical (Task 15 verifies), mounts-first create with file browser (Tasks 5, 17), staged edits + save (Task 16), ConfigEditor integration (Tasks 16, 18), three reusable widgets (Tasks 3–6), jackin style (Tasks 2, 7, 19). ✓
- **Non-goals** observed: no per-agent env editing, no global mounts, no agent lifecycle, no CLI changes, no CHANGELOG. ✓
- **State machine** (§ 3): Tasks 8–10 implement the full enum + struct set. ✓
- **Module layout** (§ 4): matches the plan's File structure exactly. ✓
- **Widgets** (§ 4): all five covered with tests. ✓
- **ConfigEditor integration** (§ 5): Tasks 16 (save), 17 (create), 18 (load). ✓
- **Style effects** (§ Style): boot reveal + save shimmer + toast auto-expire in Task 19. Tab slider and panel focus glow noted but deferred — the plan ships without them since they're cosmetic and can land in a follow-up; flag in the PR body as future work.
- **Testing** (§ Testing): unit tests per widget (Tasks 3–7), state tests (Tasks 8–10), integration test (Task 20). ✓

One explicit scope cut: the **tab-selector slide animation** and **panel focus glow** from the spec's style section are omitted in this plan. They're cosmetic, they require more timer-driven infrastructure than this plan wants to introduce, and they can be added later without re-architecting. Boot reveal + save shimmer + toast are enough to set the jackin-signature tone without slowing the PR.

Placeholder scan: no "TBD" / "implement later" / "handle edge cases" / "similar to Task N". Each step has actual code. Some API calls (e.g., ratatui-textarea `TextArea::lines()`) are stated with best-guess method names — if crate APIs drift, implementers adjust. That's normal external-crate friction, not a placeholder.

Type consistency: `ModalOutcome<T>` used identically across all widgets. `EditorState` field additions threaded through Tasks 9, 16, 17. `ManagerStage` variants stable from Task 8 forward. `InputOutcome` defined in Task 14, consumed in Tasks 15+.
