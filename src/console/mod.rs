// `ConsoleStage` collapsed to a single variant in PR #171's Modal::RolePicker
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
pub use state::build_workspace_choice;

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleOutcome {
    Launch(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>),
    InstanceAction {
        container: String,
        action: ConsoleInstanceAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction {
    Reconnect,
    NewSession,
    Inspect,
    Purge,
}

impl ConsoleState {
    /// Default role → launch; one eligible → launch; multiple →
    /// open `Modal::RolePicker`. `WorkspaceChoice` is built fresh
    /// each call so manager edits take effect immediately.
    pub fn dispatch_launch_for_workspace(
        &mut self,
        config: &AppConfig,
        cwd: &std::path::Path,
        input: LoadWorkspaceInput,
    ) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>)>>
    {
        let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
            // Workspace was deleted between keypress and dispatch.
            return Ok(None);
        };
        let mut roles = choice.allowed_roles.clone();
        let default_role = choice.default_role.clone();

        if let Some(default_key) = default_role.as_deref()
            && let Some(role) = roles.iter().find(|a| a.key() == default_key).cloned()
        {
            let workspace = preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
            self.pending_launch = None;
            self.pending_launch_role = None;
            return Ok(Some((role, workspace, None)));
        }

        match roles.len() {
            0 => {
                // Toast + stay so the operator can fix `allowed_roles`
                // — a single Enter shouldn't terminate the TUI.
                let name = choice.name;
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.toast = Some(crate::console::manager::state::Toast {
                        message: format!("no eligible roles for workspace \"{name}\""),
                        kind: crate::console::manager::state::ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                self.pending_launch = None;
                self.pending_launch_role = None;
                Ok(None)
            }
            1 => {
                let role = roles.swap_remove(0);
                let workspace = preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
                self.pending_launch = None;
                self.pending_launch_role = None;
                Ok(Some((role, workspace, None)))
            }
            _ => {
                // Multiple eligible: pin `pending_launch` so the
                // `LaunchWithAgent` arm rebuilds the choice on commit.
                self.pending_launch = Some(input);
                self.pending_launch_role = None;
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.inline_role_picker = Some(
                        crate::console::widgets::role_picker::RolePickerState::with_confirm_label(
                            roles, "launch",
                        ),
                    );
                }
                Ok(None)
            }
        }
    }
}

/// 20 Hz: spinner stays fluid and op results surface within ~50ms
/// without hot-spinning. <16ms wastes cycles, >100ms stutters.
const TICK_MS: u64 = 50;
const MAX_EVENTS_PER_TICK: usize = 256;
const MAX_TEARDOWN_DRAIN_EVENTS: usize = 16_384;
const TEARDOWN_DRAIN_QUIET_MS: u64 = 30;
const TEARDOWN_DRAIN_MAX_MS: u64 = 250;
const MOUSE_ESCAPE_GRACE_MS: u64 = 150;

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

/// Bare `Q` exits silently only on the main list — anywhere else
/// (editor, prelude, confirm, list modal) pops the exit prompt.
const fn is_on_main_screen(state: &ConsoleState) -> bool {
    let ConsoleStage::Manager(ms) = &state.stage;
    matches!(ms.stage, crate::console::manager::state::ManagerStage::List)
        && ms.list_modal.is_none()
}

/// Modals that consume letters (`TextInput`, pickers with filter-as-
/// you-type) must shadow the Q-intercept so `Q` types the letter.
const fn consumes_letter_input(state: &ConsoleState) -> bool {
    use crate::console::manager::state::{GlobalMountModal, ManagerStage, Modal};
    let ConsoleStage::Manager(ms) = &state.stage;

    if let Some(modal) = &ms.list_modal
        && matches!(modal, Modal::RolePicker { .. } | Modal::OpPicker { .. })
    {
        return true;
    }

    if let ManagerStage::Editor(editor) = &ms.stage
        && let Some(modal) = &editor.modal
        && matches!(
            modal,
            Modal::TextInput { .. }
                | Modal::OpPicker { .. }
                | Modal::RolePicker { .. }
                | Modal::RoleOverridePicker { .. }
        )
    {
        return true;
    }

    if let ManagerStage::CreatePrelude(p) = &ms.stage
        && let Some(modal) = &p.modal
        && matches!(modal, Modal::TextInput { .. })
    {
        return true;
    }
    if let ManagerStage::Settings(settings) = &ms.stage
        && let Some(modal) = &settings.mounts.modal
        && matches!(modal, GlobalMountModal::Text { .. })
    {
        return true;
    }

    false
}

const fn modal_debug_name(modal: &crate::console::manager::state::Modal<'_>) -> &'static str {
    use crate::console::manager::state::Modal;
    match modal {
        Modal::TextInput { .. } => "TextInput",
        Modal::FileBrowser { .. } => "FileBrowser",
        Modal::MountDstChoice { .. } => "MountDstChoice",
        Modal::WorkdirPick { .. } => "WorkdirPick",
        Modal::Confirm { .. } => "Confirm",
        Modal::SaveDiscardCancel { .. } => "SaveDiscardCancel",
        Modal::GithubPicker { .. } => "GithubPicker",
        Modal::ConfirmSave { .. } => "ConfirmSave",
        Modal::ErrorPopup { .. } => "ErrorPopup",
        Modal::OpPicker { .. } => "OpPicker",
        Modal::RolePicker { .. } => "RolePicker",
        Modal::RoleOverridePicker { .. } => "RoleOverridePicker",
        Modal::SourcePicker { .. } => "SourcePicker",
        Modal::AuthSourcePicker { .. } => "AuthSourcePicker",
        Modal::ScopePicker { .. } => "ScopePicker",
        Modal::AuthForm { .. } => "AuthForm",
        Modal::AuthRolePicker { .. } => "AuthRolePicker",
    }
}

fn console_location_debug(console_state: &ConsoleState) -> String {
    if console_state.quit_confirm.is_some() {
        return "quit-confirm".into();
    }

    let ConsoleStage::Manager(ms) = &console_state.stage;
    let list_modal = ms.list_modal.as_ref().map_or_else(String::new, |modal| {
        format!(" list_modal={}", modal_debug_name(modal))
    });
    let location = match &ms.stage {
        crate::console::manager::state::ManagerStage::List => "list".to_string(),
        crate::console::manager::state::ManagerStage::Editor(editor) => {
            let modal = editor.modal.as_ref().map_or("none", modal_debug_name);
            format!(
                "editor mode={:?} tab={:?} field={:?} modal={modal}",
                editor.mode, editor.active_tab, editor.active_field
            )
        }
        crate::console::manager::state::ManagerStage::CreatePrelude(prelude) => {
            let modal = prelude.modal.as_ref().map_or("none", modal_debug_name);
            format!("create-prelude step={:?} modal={modal}", prelude.step)
        }
        crate::console::manager::state::ManagerStage::ConfirmDelete { .. } => {
            "confirm-delete".to_string()
        }
        crate::console::manager::state::ManagerStage::Settings(settings) => {
            let modal = settings
                .mounts
                .modal
                .as_ref()
                .map_or("none", |modal| match modal {
                    crate::console::manager::state::GlobalMountModal::Text { .. } => "text-input",
                    crate::console::manager::state::GlobalMountModal::FileBrowser { .. } => {
                        "file-browser"
                    }
                    crate::console::manager::state::GlobalMountModal::MountDstChoice { .. } => {
                        "mount-dst-choice"
                    }
                    crate::console::manager::state::GlobalMountModal::ScopePicker { .. } => {
                        "scope-picker"
                    }
                    crate::console::manager::state::GlobalMountModal::RolePicker { .. } => {
                        "role-picker"
                    }
                    crate::console::manager::state::GlobalMountModal::Confirm {
                        action, ..
                    } => match action {
                        crate::console::manager::state::GlobalMountConfirm::Remove => {
                            "confirm-remove"
                        }
                        crate::console::manager::state::GlobalMountConfirm::Save => "confirm-save",
                        crate::console::manager::state::GlobalMountConfirm::Sensitive => {
                            "confirm-sensitive"
                        }
                        crate::console::manager::state::GlobalMountConfirm::Discard => {
                            "confirm-discard"
                        }
                    },
                    crate::console::manager::state::GlobalMountModal::PreviewSave { .. } => {
                        "preview-save"
                    }
                });
            format!(
                "settings tab={:?} selected={} modal={modal}",
                settings.active_tab, settings.mounts.selected
            )
        }
    };
    format!("{location}{list_modal}")
}

/// Render a key event for the `--debug` log. Redacts the literal
/// character when the focused widget is consuming text input — without
/// the redaction the operator's typed values (workspace names, env
/// values, paths) would land in `--debug` output verbatim.
fn key_debug_name(state: &ConsoleState, key: crossterm::event::KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let has_command_modifier = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    let code = match key.code {
        KeyCode::Char(_) if consumes_letter_input(state) && !has_command_modifier => {
            "Char(<redacted>)".to_string()
        }
        KeyCode::Char(ch) => format!("Char({})", ch.escape_default()),
        other => format!("{other:?}"),
    };
    if key.modifiers.is_empty() {
        code
    } else {
        format!("{:?}+{code}", key.modifiers)
    }
}

const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    !matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
}

fn drain_pending_terminal_events(limit: usize) {
    drain_pending_terminal_events_until_quiet(limit, std::time::Duration::ZERO);
}

fn drain_pending_terminal_events_until_quiet(limit: usize, quiet_for: std::time::Duration) {
    let started = std::time::Instant::now();
    for _ in 0..limit {
        let poll_for = if quiet_for.is_zero() {
            std::time::Duration::ZERO
        } else {
            let elapsed = started.elapsed();
            let max = std::time::Duration::from_millis(TEARDOWN_DRAIN_MAX_MS);
            if elapsed >= max {
                break;
            }
            quiet_for.min(max.saturating_sub(elapsed))
        };
        match crossterm::event::poll(poll_for) {
            Ok(true) => {
                let _ = crossterm::event::read();
            }
            Ok(false) | Err(_) => break,
        }
    }
}

#[cfg(unix)]
fn flush_terminal_input_queue() {
    if let Ok(tty) = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        let _ = nix::sys::termios::tcflush(&tty, nix::sys::termios::FlushArg::TCIFLUSH);
    }
}

#[cfg(not(unix))]
fn flush_terminal_input_queue() {}

fn enable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // Crossterm's EnableMouseCapture includes ?1003h "any-event"
    // tracking. That reports plain pointer motion and can flood the pty
    // under touchpad inertia. Jackin needs press/release, drag, scroll,
    // and SGR coordinates, so enable only those modes.
    out.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1015h\x1b[?1006h")?;
    out.flush()
}

fn disable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // Disable the exact modes we enable, plus ?1003l defensively in case
    // an older build or another library enabled any-event tracking.
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

fn maybe_open_inline_agent_picker(
    state: &mut ConsoleState,
    paths: &JackinPaths,
    role: RoleSelector,
    workspace: &ResolvedWorkspace,
) -> bool {
    let Some(agents) = crate::app::context::supported_agents_requiring_prompt(
        paths,
        &role,
        workspace.default_agent,
    ) else {
        return false;
    };

    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.inline_agent_picker = Some((
        role.clone(),
        crate::console::widgets::agent_choice::AgentChoiceState::with_choices(agents),
    ));
    ms.inline_role_picker = None;
    state.pending_launch_role = Some(role);
    true
}

#[allow(clippy::too_many_lines)]
pub fn run_console(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    use std::time::Duration;

    use crossterm::ExecutableCommand;
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    // EnableMouseCapture disables native text selection; operators
    // hold Shift (Terminal.app, iTerm2) or Option (iTerm2) to bypass.
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let mut stdout = std::io::stdout();
            drain_pending_terminal_events_until_quiet(
                MAX_TEARDOWN_DRAIN_EVENTS,
                std::time::Duration::from_millis(TEARDOWN_DRAIN_QUIET_MS),
            );
            let _ = disable_console_mouse_capture(&mut stdout);
            drain_pending_terminal_events(MAX_TEARDOWN_DRAIN_EVENTS);
            flush_terminal_input_queue();
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
            let _ = stdout.execute(crossterm::cursor::Show);
            crate::tui::end_debug_buffering();
        }
    }

    let mut state = ConsoleState::new(&config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    crate::tui::begin_debug_buffering();
    stdout.execute(EnterAlternateScreen)?;
    enable_console_mouse_capture(&mut stdout)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut last_mouse_event_at: Option<std::time::Instant> = None;

    let result = 'main: loop {
        // Auto-expire manager toasts after 3 seconds.
        if let ConsoleStage::Manager(ms) = &mut state.stage
            && let Some(toast) = &ms.toast
            && toast.shown_at.elapsed() > std::time::Duration::from_secs(3)
        {
            ms.toast = None;
        }

        // Drain worker results before render so a fresh result lands
        // this frame instead of a stale Loading one.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            ms.poll_picker_loads();
            ms.refresh_instances(paths);
        }

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
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Non-blocking poll: a TICK_MS timeout falls through to advance
        // the spinner and drain worker channels even when idle.
        let mut events_processed = 0;
        while events_processed < MAX_EVENTS_PER_TICK
            && event::poll(if events_processed == 0 {
                Duration::from_millis(TICK_MS)
            } else {
                Duration::ZERO
            })?
        {
            events_processed += 1;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if matches!(key.code, KeyCode::Esc)
                        && key.modifiers.is_empty()
                        && last_mouse_event_at.is_some_and(|at| {
                            at.elapsed() <= Duration::from_millis(MOUSE_ESCAPE_GRACE_MS)
                        })
                    {
                        continue;
                    }
                    crate::debug_log!(
                        "tui",
                        "key={} location={}",
                        key_debug_name(&state, key),
                        console_location_debug(&state)
                    );
                    if let Some(confirm) = state.quit_confirm.as_mut() {
                        use crate::console::widgets::ModalOutcome;
                        match confirm.handle_key(key) {
                            ModalOutcome::Commit(true) => break 'main Ok(None),
                            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                                state.quit_confirm = None;
                            }
                            ModalOutcome::Continue => {}
                        }
                        continue;
                    }

                    // Q intercept: outside main screen, pop the exit
                    // confirm. SHIFT tolerated for caps-lock parity.
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
                            break 'main Ok(None);
                        }
                        manager::InputOutcome::LaunchNamed(name) => {
                            let input = LoadWorkspaceInput::Saved(name);
                            match state.dispatch_launch_for_workspace(&config, cwd, input.clone()) {
                                Ok(Some((role, workspace, agent))) => {
                                    if agent.is_none()
                                        && maybe_open_inline_agent_picker(
                                            &mut state,
                                            paths,
                                            role.clone(),
                                            &workspace,
                                        )
                                    {
                                        state.pending_launch = Some(input);
                                    } else {
                                        break 'main Ok(Some(ConsoleOutcome::Launch(
                                            role, workspace, agent,
                                        )));
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => break 'main Err(e),
                            }
                        }
                        manager::InputOutcome::LaunchCurrentDir => {
                            let input = LoadWorkspaceInput::CurrentDir;
                            match state.dispatch_launch_for_workspace(&config, cwd, input.clone()) {
                                Ok(Some((role, workspace, agent))) => {
                                    if agent.is_none()
                                        && maybe_open_inline_agent_picker(
                                            &mut state,
                                            paths,
                                            role.clone(),
                                            &workspace,
                                        )
                                    {
                                        state.pending_launch = Some(input);
                                    } else {
                                        break 'main Ok(Some(ConsoleOutcome::Launch(
                                            role, workspace, agent,
                                        )));
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => break 'main Err(e),
                            }
                        }
                        manager::InputOutcome::LaunchWithAgent(role) => {
                            // Rebuild the choice now so edits between
                            // open and commit take effect. `take()`
                            // clears the pin even on concurrent delete.
                            if let Some(input) = state.pending_launch.take()
                                && let Some(choice) = build_workspace_choice(&config, cwd, &input)?
                            {
                                match preview::resolve_selected_workspace(
                                    &config, cwd, &choice, &role,
                                ) {
                                    Ok(workspace) => {
                                        if maybe_open_inline_agent_picker(
                                            &mut state,
                                            paths,
                                            role.clone(),
                                            &workspace,
                                        ) {
                                            state.pending_launch = Some(input);
                                        } else {
                                            state.pending_launch_role = None;
                                            break 'main Ok(Some(ConsoleOutcome::Launch(
                                                role, workspace, None,
                                            )));
                                        }
                                    }
                                    Err(e) => break 'main Err(e),
                                }
                            }
                        }
                        manager::InputOutcome::LaunchWithRuntimeAgent(agent) => {
                            if let (Some(input), Some(role)) = (
                                state.pending_launch.take(),
                                state.pending_launch_role.take(),
                            ) && let Some(choice) = build_workspace_choice(&config, cwd, &input)?
                            {
                                match preview::resolve_selected_workspace(
                                    &config, cwd, &choice, &role,
                                ) {
                                    Ok(workspace) => {
                                        break 'main Ok(Some(ConsoleOutcome::Launch(
                                            role,
                                            workspace,
                                            Some(agent),
                                        )));
                                    }
                                    Err(e) => break 'main Err(e),
                                }
                            }
                        }
                        manager::InputOutcome::InstanceAction { container, action } => {
                            break 'main Ok(Some(ConsoleOutcome::InstanceAction {
                                container,
                                action,
                            }));
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    last_mouse_event_at = Some(std::time::Instant::now());
                    if should_debug_log_mouse(mouse) {
                        crate::debug_log!(
                            "tui",
                            "mouse={mouse:?} location={}",
                            console_location_debug(&state)
                        );
                    }
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::input::handle_mouse_with_config(
                            ms,
                            mouse,
                            term_size,
                            Some(&config),
                        );
                    }
                }
                _ => {}
            }
        }
    };

    drop(guard);
    result
}

#[cfg(test)]
mod quit_confirm_tests {
    //! Pin the gates for the Q-intercept and the
    //! `ConfirmState::handle_key` outcomes the run-loop dispatches.
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

    fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
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
        // FileBrowser stands in for any list-anchored modal — predicate
        // only checks `is_some`.
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
    fn debug_key_redacts_text_input_characters() {
        let mut state = fresh_state();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let mut editor = EditorState::new_create();
        editor.modal = Some(Modal::TextInput {
            target: TextInputTarget::EnvValue {
                scope: SecretsScopeTag::Workspace,
                key: "TOKEN".into(),
            },
            state: TextInputState::new("Value", ""),
        });
        ms.stage = ManagerStage::Editor(editor);

        assert_eq!(
            key_debug_name(&state, key(crossterm::event::KeyCode::Char('s'))),
            "Char(<redacted>)"
        );
        assert_eq!(
            key_debug_name(&state, key(crossterm::event::KeyCode::Enter)),
            "Enter"
        );
    }

    #[test]
    fn debug_location_includes_stage_and_modal_without_values() {
        let mut state = fresh_state();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let mut editor = EditorState::new_create();
        editor.modal = Some(Modal::TextInput {
            target: TextInputTarget::EnvValue {
                scope: SecretsScopeTag::Workspace,
                key: "TOKEN".into(),
            },
            state: TextInputState::new("Value", ""),
        });
        ms.stage = ManagerStage::Editor(editor);

        let location = console_location_debug(&state);
        assert!(location.contains("editor"), "{location}");
        assert!(location.contains("modal=TextInput"), "{location}");
        assert!(!location.contains("TOKEN"), "{location}");
    }

    #[test]
    fn quit_confirm_handle_key_y_commits_exit() {
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_n_returns_commit_false() {
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_esc_cancels() {
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }
}
