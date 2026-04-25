// `ConsoleStage` collapsed to a single variant in PR #171's Modal::AgentPicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![allow(irrefutable_let_patterns)]

pub mod manager;
pub mod op_cache;
mod preview;
pub mod state;
pub mod widgets;

pub use op_cache::OpCache;
pub use state::ConsoleStage;
pub use state::ConsoleState;
pub use state::WorkspaceChoice;

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

impl ConsoleState {
    /// Shared `LaunchNamed` / `LaunchCurrentDir` transition: preselect
    /// the workspace at `idx` in `ConsoleState::workspaces`, then route
    /// by agent count.
    ///
    /// Three branches:
    /// 1. `default_agent` set on the workspace → launch immediately with
    ///    that agent.
    /// 2. Exactly one eligible agent (after the
    ///    `eligible_agents_for_workspace` filtering already baked into
    ///    `WorkspaceChoice.allowed_agents`) → launch immediately with it.
    /// 3. Multiple eligible agents and no default → open
    ///    `Modal::AgentPicker` on the manager list and stay in the
    ///    run-loop until the operator commits a choice.
    ///
    /// Returns `Ok(Some(_))` if the caller should break with that
    /// outcome, `Ok(None)` to stay in the run-loop (modal opened, or
    /// there are no eligible agents and we surface a toast). Errors only
    /// when workspace resolution itself fails.
    pub fn dispatch_launch_for_workspace(
        &mut self,
        config: &AppConfig,
        cwd: &std::path::Path,
        idx: usize,
    ) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
        self.selected_workspace = idx;

        let Some(choice) = self.workspaces.get(idx) else {
            return Ok(None);
        };
        let agents = choice.allowed_agents.clone();
        let default_agent = choice.default_agent.clone();

        // Branch 1: default agent set + present in the eligible set → direct launch.
        if let Some(default_key) = default_agent.as_deref()
            && let Some(agent) = agents.iter().find(|a| a.key() == default_key).cloned()
        {
            let workspace =
                preview::resolve_selected_workspace(config, cwd, &self.workspaces[idx], &agent)?;
            return Ok(Some((agent, workspace)));
        }

        // Branch 2: zero or one eligible agent.
        match agents.len() {
            0 => {
                // No eligible agents — toast and stay in the manager list so
                // the operator can edit the workspace's `allowed_agents` or
                // register an agent. Avoids a hard error that would terminate
                // the TUI from a single Enter press.
                let name = self
                    .workspaces
                    .get(idx)
                    .map_or("<unknown>", |choice| choice.name.as_str())
                    .to_string();
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.toast = Some(crate::console::manager::state::Toast {
                        message: format!("no eligible agents for workspace \"{name}\""),
                        kind: crate::console::manager::state::ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                Ok(None)
            }
            1 => {
                let agent = agents.into_iter().next().unwrap();
                let workspace = preview::resolve_selected_workspace(
                    config,
                    cwd,
                    &self.workspaces[idx],
                    &agent,
                )?;
                Ok(Some((agent, workspace)))
            }
            _ => {
                // Branch 3: multiple eligible — open the picker overlay.
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.list_modal = Some(crate::console::manager::state::Modal::AgentPicker {
                        state: crate::console::widgets::agent_picker::AgentPickerState::with_confirm_label(
                            agents, "launch",
                        ),
                    });
                }
                Ok(None)
            }
        }
    }
}

/// Outer event-loop tick interval. 20 Hz keeps the picker's Braille
/// spinner visibly fluid and lets the per-tick worker-channel drain
/// surface `op` results within ~50 ms of the worker finishing — without
/// hot-spinning the CPU. Matched against [`crossterm::event::poll`]'s
/// timeout: when no input arrives within `TICK_MS`, the loop falls
/// through to the next iteration, re-renders, and re-polls. Picked at
/// the brief's recommended balance — tighter (≤16 ms) wastes cycles on
/// idle frames; looser (>100 ms) makes the spinner stutter.
const TICK_MS: u64 = 50;

/// Centered rect for the "Exit jackin'?" confirm dialog. Sized to
/// what the `confirm` widget needs given its prompt; clamped to a
/// modest 44-column width so it reads as a small dialog rather than
/// a full-screen takeover.
fn quit_confirm_area(
    frame: ratatui::layout::Rect,
    confirm: &crate::console::widgets::confirm::ConfirmState,
) -> ratatui::layout::Rect {
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = crate::console::widgets::confirm::required_height(confirm)
        .min(frame.height.saturating_sub(2));
    let x = frame.x + frame.width.saturating_sub(width) / 2;
    let y = frame.y + frame.height.saturating_sub(height) / 2;
    ratatui::layout::Rect {
        x,
        y,
        width,
        height,
    }
}

/// `true` when the operator is on the main manager list — the only
/// place a bare `Q` exits silently. Defined as: top-level stage is
/// `Manager`, the manager sub-stage is `List`, and no `list_modal`
/// is open. Any other condition (editor stage, create prelude,
/// confirm-delete, or a list-anchored modal like `AgentPicker`) is
/// "inside" something and should pop the exit confirmation instead.
const fn is_on_main_screen(state: &ConsoleState) -> bool {
    let ConsoleStage::Manager(ms) = &state.stage;
    matches!(ms.stage, crate::console::manager::state::ManagerStage::List)
        && ms.list_modal.is_none()
}

/// `true` when the active modal consumes letter characters as input
/// (text entry or filter-as-you-type). The Q-intercept must defer to
/// these modals so pressing Q types the letter rather than popping
/// the exit confirmation.
///
/// Modals checked:
/// - [`TextInput`](manager::state::Modal::TextInput) (name, workdir,
///   mount dst, `EnvKey`, `EnvValue`) — types `Q` into the textarea.
/// - [`OpPicker`](manager::state::Modal::OpPicker) (any pane: Account /
///   Vault / Item / Field) — appends `Q` to the per-pane filter buffer.
/// - [`AgentPicker`](manager::state::Modal::AgentPicker) — appends `Q`
///   to its filter.
///
/// Modals deliberately omitted because they don't consume letters as
/// input: `Confirm`, `ConfirmSave`, `MountDstChoice`, `WorkdirPick`,
/// `SaveDiscardCancel`, `GithubPicker`, `SourcePicker`, `ErrorPopup`,
/// `FileBrowser`. Letting `Q` escape from those modals to the exit
/// confirm matches the spec — the operator typically uses arrow keys /
/// enter / esc there.
const fn consumes_letter_input(state: &ConsoleState) -> bool {
    use crate::console::manager::state::{ManagerStage, Modal};
    let ConsoleStage::Manager(ms) = &state.stage;

    // List-anchored modals (AgentPicker, GithubPicker, etc.).
    if let Some(modal) = &ms.list_modal
        && matches!(modal, Modal::AgentPicker { .. } | Modal::OpPicker { .. })
    {
        return true;
    }

    // Editor-anchored modals.
    if let ManagerStage::Editor(editor) = &ms.stage
        && let Some(modal) = &editor.modal
        && matches!(
            modal,
            Modal::TextInput { .. }
                | Modal::OpPicker { .. }
                | Modal::AgentPicker { .. }
                | Modal::AgentOverridePicker { .. }
        )
    {
        return true;
    }

    // CreatePrelude-anchored modals (FileBrowser, MountDstChoice,
    // WorkdirPick, TextInput for naming the workspace).
    if let ManagerStage::CreatePrelude(p) = &ms.stage
        && let Some(modal) = &p.modal
        && matches!(modal, Modal::TextInput { .. })
    {
        return true;
    }

    false
}

#[allow(clippy::too_many_lines)]
pub fn run_console(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
    use std::time::Duration;

    use crossterm::ExecutableCommand;
    use crossterm::event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    };
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    // NOTE: `EnableMouseCapture` intercepts mouse events so the workspace
    // manager can drive a draggable split between the list and details
    // panes. A known side-effect is that the terminal's native text
    // selection (click-drag to select, cmd/ctrl-C to copy) stops working
    // while the TUI is running. Operators who need to copy text from the
    // TUI can hold Shift (Terminal.app, iTerm2) or Option (iTerm2) to
    // bypass capture at the terminal level. A runtime toggle could be
    // added later but is out of scope here.
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let mut stdout = std::io::stdout();
            let _ = stdout.execute(DisableMouseCapture);
            let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
            let _ = stdout.execute(crossterm::cursor::Show);
        }
    }

    let mut state = ConsoleState::new(&config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        // Auto-expire manager toasts after 3 seconds.
        if let ConsoleStage::Manager(ms) = &mut state.stage
            && let Some(toast) = &ms.toast
            && toast.shown_at.elapsed() > std::time::Duration::from_secs(3)
        {
            ms.toast = None;
        }

        // Drain pending background-worker results (1Password picker
        // loading state, etc.) BEFORE rendering so a freshly-arrived
        // result lands in this frame rather than a one-tick-stale
        // Loading frame. The render path's `OpPickerState::tick` also
        // drains the channel; both call sites are idempotent on an
        // empty channel.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            ms.poll_picker_loads();
        }

        // Render the manager. `ConsoleStage` is single-variant today —
        // the legacy full-screen agent picker was replaced by a
        // `Modal::AgentPicker` overlay that the manager render already
        // handles via the list_modal slot.
        //
        // When the top-level "Exit jackin'?" confirm is open, render
        // the manager first as the underlay, then overlay the confirm
        // dialog as a centered modal in the same frame so the operator
        // sees both — the dialog doesn't lose its context.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            let confirm_state = state.quit_confirm.as_ref();
            terminal.draw(|frame| {
                manager::render(frame, ms, &config, cwd);
                if let Some(confirm) = confirm_state {
                    let area = quit_confirm_area(frame.area(), confirm);
                    crate::console::widgets::confirm::render(frame, area, confirm);
                }
            })?;
        }
        // Capture terminal size before polling for input so the mouse
        // handler can hit-test against the current seam position. Cheap
        // syscall; harmless to call every loop turn. Convert the `Size`
        // into a `Rect` with zero origin so the handler signature stays
        // aligned with ratatui's own area-based hit-test conventions.
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Non-blocking event poll with a tick timeout. When no input
        // arrives within `TICK_MS`, `poll` returns `false` and the loop
        // falls through to the next iteration — keeping the picker
        // spinner advancing and the worker channel draining at 20 Hz
        // regardless of operator input. (A prior blocking
        // `event::read()` froze both updates between keystrokes, making
        // the picker feel unresponsive while `op` was loading.)
        if event::poll(Duration::from_millis(TICK_MS))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // Quit-confirm dialog is the single chokepoint when
                    // open: it consumes ALL keys until it closes. Y commits
                    // exit; N / Esc closes the dialog without touching any
                    // underlying state, so the operator returns to exactly
                    // where they were.
                    if let Some(confirm) = state.quit_confirm.as_mut() {
                        use crate::console::widgets::ModalOutcome;
                        match confirm.handle_key(key) {
                            ModalOutcome::Commit(true) => break Ok(None),
                            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                                state.quit_confirm = None;
                            }
                            ModalOutcome::Continue => {}
                        }
                        continue;
                    }

                    // Top-level Q intercept: outside the main screen, Q
                    // pops the "Exit jackin'?" confirmation. Skips when a
                    // modal that consumes letter input is up (textarea or
                    // filter-as-you-type picker), so Q types as a
                    // character there. Caps-lock parity: accept Shift but
                    // no other modifiers (matches commit 24).
                    if matches!(key.code, KeyCode::Char('q' | 'Q'))
                        && (key.modifiers - KeyModifiers::SHIFT).is_empty()
                        && !is_on_main_screen(&state)
                        && !consumes_letter_input(&state)
                    {
                        state.quit_confirm = Some(
                            crate::console::widgets::confirm::ConfirmState::new("Exit jackin'?"),
                        );
                        continue;
                    }

                    let outcome = if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::handle_key(ms, &mut config, paths, cwd, key)?
                    } else {
                        manager::InputOutcome::Continue
                    };
                    match outcome {
                        manager::InputOutcome::Continue => {}
                        manager::InputOutcome::ExitJackin => {
                            break Ok(None);
                        }
                        manager::InputOutcome::LaunchNamed(name) => {
                            // Find the workspace by name in ConsoleState.workspaces.
                            if let Some(idx) = state
                                .workspaces
                                .iter()
                                .position(|choice| choice.name == name)
                            {
                                match state.dispatch_launch_for_workspace(&config, cwd, idx) {
                                    Ok(Some(outcome)) => break Ok(Some(outcome)),
                                    Ok(None) => {}
                                    Err(e) => break Err(e),
                                }
                            }
                        }
                        manager::InputOutcome::LaunchCurrentDir => {
                            // Index 0 of ConsoleState.workspaces is the
                            // synthetic "Current directory" choice (built
                            // in ConsoleState::new). Route it through the
                            // same dispatcher as a saved workspace.
                            match state.dispatch_launch_for_workspace(&config, cwd, 0) {
                                Ok(Some(outcome)) => break Ok(Some(outcome)),
                                Ok(None) => {}
                                Err(e) => break Err(e),
                            }
                        }
                        manager::InputOutcome::LaunchWithAgent(agent) => {
                            // The `AgentPicker` modal just committed. The
                            // dispatcher pinned `selected_workspace` when
                            // it opened the picker, so resolve against
                            // that. Should be unreachable when the index
                            // is missing — the dispatcher validated it on
                            // open — but fall back to staying in the
                            // run-loop rather than panicking on a state-
                            // machine inconsistency.
                            let idx = state.selected_workspace;
                            if let Some(choice) = state.workspaces.get(idx) {
                                match preview::resolve_selected_workspace(
                                    &config, cwd, choice, &agent,
                                ) {
                                    Ok(workspace) => break Ok(Some((agent, workspace))),
                                    Err(e) => break Err(e),
                                }
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    // Only the Manager/List stage consumes mouse events
                    // today (list/details seam drag). Modals on other
                    // stages fall through as silent no-ops.
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::input::handle_mouse(ms, mouse, term_size);
                    }
                }
                _ => {}
            }
        }
        // No `else` — when `poll` times out, fall through to the next
        // loop turn so the spinner ticks and channels drain.
    };

    drop(guard);
    result
}

#[cfg(test)]
mod quit_confirm_tests {
    //! Pin the routing rules for the top-level "Exit jackin'?" confirm:
    //!
    //! - `is_on_main_screen` is true ONLY on the bare manager list with
    //!   no list_modal open. Any sub-stage / modal flips it false.
    //! - `consumes_letter_input` is true when a TextInput / OpPicker /
    //!   AgentPicker modal owns the keyboard so Q types as input.
    //!
    //! These two predicates gate the Q-intercept in `run_console`'s
    //! event loop. The integration of those gates with the actual
    //! keypress dispatch is verified end-to-end in the
    //! `quit_confirm_handle_key_*` tests below, which drive the same
    //! `ConfirmState::handle_key` the loop calls.
    use super::*;
    use crate::console::manager::state::{
        EditorState, FileBrowserTarget, ManagerStage, Modal, SecretsScopeTag, TextInputTarget,
    };
    use crate::console::widgets::{
        ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
        text_input::TextInputState,
    };

    fn fresh_state() -> ConsoleState {
        let cwd = std::env::temp_dir();
        let config = AppConfig::default();
        ConsoleState::new(&config, &cwd).unwrap()
    }

    #[test]
    fn main_screen_is_list_with_no_modal() {
        let state = fresh_state();
        assert!(is_on_main_screen(&state));
        assert!(!consumes_letter_input(&state));
    }

    #[test]
    fn editor_stage_is_not_main_screen() {
        let mut state = fresh_state();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        ms.stage = ManagerStage::Editor(EditorState::new_create());
        assert!(!is_on_main_screen(&state));
    }

    #[test]
    fn list_modal_is_not_main_screen() {
        let mut state = fresh_state();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        // Open a FileBrowser as a stand-in for any list-anchored modal.
        // (Production list_modal slots hold AgentPicker / GithubPicker /
        // OpPicker; FileBrowser happens to construct cleanly without a
        // real picker setup, and the predicate only checks `is_some`.)
        ms.list_modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: FileBrowserState::new_from_home().unwrap(),
        });
        assert!(!is_on_main_screen(&state));
    }

    #[test]
    fn text_input_modal_consumes_letter_input() {
        let mut state = fresh_state();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let mut editor = EditorState::new_create();
        editor.modal = Some(Modal::TextInput {
            target: TextInputTarget::EnvKey {
                scope: SecretsScopeTag::Workspace,
            },
            state: TextInputState::new("Key", ""),
        });
        ms.stage = ManagerStage::Editor(editor);
        assert!(consumes_letter_input(&state));
        assert!(!is_on_main_screen(&state));
    }

    #[test]
    fn quit_confirm_handle_key_y_commits_exit() {
        let mut s = ConfirmState::new("Exit jackin'?");
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('y'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert!(matches!(s.handle_key(key), ModalOutcome::Commit(true)));
    }

    #[test]
    fn quit_confirm_handle_key_n_returns_commit_false() {
        let mut s = ConfirmState::new("Exit jackin'?");
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('n'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        // The run-loop maps Commit(false) | Cancel to "close dialog",
        // restoring the operator to where they were.
        assert!(matches!(s.handle_key(key), ModalOutcome::Commit(false)));
    }

    #[test]
    fn quit_confirm_handle_key_esc_cancels() {
        let mut s = ConfirmState::new("Exit jackin'?");
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Esc,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert!(matches!(s.handle_key(key), ModalOutcome::Cancel));
    }
}
