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

use crate::app::context::preferred_agent_index;
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
    /// Operator selected an agent AND a provider in the console picker.
    /// The chosen `Provider` drives the env redirection (e.g. Z.AI's
    /// Anthropic-compatible endpoint) and the tab-name suffix.
    NewSessionWithProvider {
        container: String,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
    /// Initial launch with a provider selected in the console before the
    /// container is created. The provider flows into the capsule's initial
    /// attach so the first session uses the chosen provider.
    LaunchWithProvider {
        selector: RoleSelector,
        workspace: ResolvedWorkspace,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction {
    Reconnect,
    /// Reconnect and ask the in-container daemon to focus this
    /// pane (`session_id`) before forwarding output. Carries through
    /// to `attach::reconnect_or_create_session_with_focus` which
    /// appends the `--focus <id>` flag on the `docker exec`.
    ReconnectFocus(u64),
    NewSession,
    NewSessionWithAgent(crate::agent::Agent),
    Shell,
    Inspect,
    Stop,
    Purge,
}

impl ConsoleInstanceAction {
    /// Actions that don't replace the TUI with another foreground process
    /// (Stop/Purge) run inside the console event loop via
    /// `InstanceActionHandler`. The rest tear down the TUI so the launched
    /// container/agent can own the terminal.
    pub const fn runs_in_place(self) -> bool {
        matches!(self, Self::Stop | Self::Purge)
    }
}

/// Callback invoked for `runs_in_place` actions.
///
/// The handler performs the docker work (eject, purge) and is expected to
/// be blocking from the caller's perspective so the TUI loop can show a
/// progress modal, run the op, then refresh.
pub trait InstanceActionHandler {
    fn run_in_place(
        &mut self,
        container: &str,
        action: ConsoleInstanceAction,
    ) -> anyhow::Result<()>;
}

impl ConsoleState {
    /// Open the inline role picker for every eligible role count except zero.
    /// `WorkspaceChoice` is built fresh each call so manager edits take effect
    /// immediately.
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
        let roles = choice.allowed_roles.clone();

        if roles.is_empty() {
            // Stay so the operator can fix `allowed_roles`
            // — a single Enter shouldn't terminate the TUI.
            let name = choice.name;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                ms.list_modal = Some(crate::console::manager::state::Modal::ErrorPopup {
                    state: crate::console::widgets::error_popup::ErrorPopupState::new(
                        "No eligible roles",
                        format!(
                            "Workspace \"{name}\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
                        ),
                    ),
                });
            }
            self.pending_launch = None;
            self.pending_launch_role = None;
        } else if roles.len() == 1 {
            // Single role — skip picker and proceed directly to agent selection.
            let role = roles.into_iter().next().unwrap();
            return preview::resolve_selected_workspace(config, cwd, &choice, &role)
                .map(|workspace| Some((role, workspace, None)));
        } else {
            let selected = preferred_agent_index(
                &roles,
                choice.last_role.as_deref(),
                choice.default_role.as_deref(),
            );
            self.pending_launch = Some(input);
            self.pending_launch_role = None;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                let mut picker =
                    crate::console::widgets::role_picker::RolePickerState::with_confirm_label(
                        roles, "launch",
                    );
                if let Some(selected) = selected {
                    picker.list_state.select(Some(selected));
                }
                ms.inline_role_picker = Some(picker);
            }
        }
        Ok(None)
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
        Modal::StatusPopup { .. } => "StatusPopup",
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
        crate::console::manager::state::ManagerStage::ConfirmInstancePurge { .. } => {
            "confirm-instance-purge".to_string()
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
    // ?1000h press/release, ?1002h drag, ?1003h any-event motion (drives tab
    // hover, matching the in-container multiplexer), ?1015h+?1006h SGR
    // coordinates. ?1003h motion floods only matter across a pty under inertia;
    // host events are local and the manager batches renders at 20Hz, so the
    // cost is paid once per coalesced frame.
    out.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h")?;
    out.flush()
}

fn disable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // Disable the exact modes we enable, plus ?1003l defensively in case
    // an older build or another library enabled any-event tracking.
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

/// Owns the terminal for an entire launch flow so it never flashes the shell.
///
/// Holds the alternate screen, raw mode, and mouse capture across console →
/// loading cockpit → capsule → exit outro so the terminal never drops back
/// to the cooked primary screen between surfaces. Each sub-surface checks
/// [`crate::tui::host_screen_owned`] and skips its own enter/leave while this
/// guard is alive; `Drop` restores the terminal exactly once, on every exit
/// path.
pub struct TerminalSession {
    _private: (),
}

impl TerminalSession {
    /// Enter raw mode + the alternate screen + mouse capture and mark the
    /// screen owned. The caller holds the returned guard for the whole flow.
    pub fn enter() -> std::io::Result<Self> {
        use crossterm::ExecutableCommand;
        let mut stdout = std::io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        crate::tui::begin_debug_buffering();
        let screen = Self { _private: () };
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        crate::tui::set_host_screen_owned(true);
        Ok(screen)
    }

    /// Drop to the cooked primary screen for the duration of `f`, then restore
    /// the full-screen session. Used for the rare interim prompts that sit
    /// between the console and the loading cockpit (sensitive-mount confirm,
    /// agent choice) and expect a normal line-buffered terminal.
    pub fn suspend<T>(&self, f: impl FnOnce() -> T) -> std::io::Result<T> {
        use crossterm::ExecutableCommand;
        let mut stdout = std::io::stdout();
        let _ = disable_console_mouse_capture(&mut stdout);
        crossterm::terminal::disable_raw_mode()?;
        stdout.execute(crossterm::terminal::LeaveAlternateScreen)?;
        stdout.execute(crossterm::cursor::Show)?;
        crate::tui::set_host_screen_owned(false);
        let out = f();
        crossterm::terminal::enable_raw_mode()?;
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        crate::tui::set_host_screen_owned(true);
        Ok(out)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        use crossterm::ExecutableCommand;
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
        crate::tui::set_host_screen_owned(false);
        crate::tui::end_debug_buffering();
    }
}

/// Hand the real terminal back to a child process: leave raw-mode +
/// alt-screen and stop debug buffering, mirroring `TerminalGuard::drop`
/// minus the input drain (the child reads stdin directly). Paired with
/// [`resume_console_terminal`] around a contained suspend → run → resume.
fn suspend_console_terminal(stdout: &mut std::io::Stdout) {
    use crossterm::ExecutableCommand;
    let _ = disable_console_mouse_capture(stdout);
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
    let _ = stdout.execute(crossterm::cursor::Show);
    crate::tui::end_debug_buffering();
}

/// Re-enter raw-mode + alt-screen after a [`suspend_console_terminal`]
/// detour, mirroring `run_console`'s initial setup so the TUI resumes
/// where it left off.
fn resume_console_terminal(stdout: &mut std::io::Stdout) -> anyhow::Result<()> {
    use crossterm::ExecutableCommand;
    crate::tui::begin_debug_buffering();
    crossterm::terminal::enable_raw_mode()?;
    stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
    enable_console_mouse_capture(stdout)?;
    Ok(())
}

async fn open_inline_agent_picker(
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    runner: &mut impl crate::docker::CommandRunner,
    role: &RoleSelector,
) -> anyhow::Result<bool> {
    let agents =
        crate::runtime::resolve_supported_agents_for_console(paths, config, role, runner).await?;
    if agents.len() < 2 {
        return Ok(false);
    }

    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.inline_agent_picker = Some((
        role.clone(),
        crate::console::widgets::agent_choice::AgentChoiceState::with_choices(agents),
    ));
    ms.inline_role_picker = None;
    state.pending_launch_role = Some(role.clone());
    Ok(true)
}

enum AgentPickerResolution {
    Opened,
    NotNeeded,
    Failed(anyhow::Error),
}

fn draw_role_resolution_dialog<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: &RoleSelector,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.status_overlay = Some(widgets::status_popup::StatusPopupState::new(
        "Resolving agent role",
        format!("Loading and resolving {}", role.key()),
    ));
    terminal.draw(|frame| {
        manager::render(frame, ms, config, cwd);
    })?;
    ms.status_overlay = None;
    Ok(())
}

/// Drop the cached item list (and that item's field list) for the
/// account/vault/item a freshly-minted op ref points at, so a picker
/// reopened in the same session re-fetches and shows the new entry. The
/// ref's `op` field is UUID-form `op://<vault>/<item>/[<section>/]<field>`.
fn invalidate_op_cache_for_ref(
    op_cache: &std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
    op_ref: &crate::operator_env::OpRef,
) {
    let Some(parts) = crate::operator_env::parse_op_reference(&op_ref.op) else {
        return;
    };
    let account = op_ref.account.as_deref();
    let mut cache = op_cache.borrow_mut();
    cache.invalidate_items(account, &parts.vault);
    cache.invalidate_fields(account, &parts.vault, &parts.item);
}

fn show_role_resolution_error(
    state: &mut ConsoleState,
    role: &RoleSelector,
    error: &anyhow::Error,
) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.list_modal = Some(manager::state::Modal::ErrorPopup {
        state: widgets::error_popup::ErrorPopupState::new(
            "Role resolution failed",
            format!("Could not resolve {}.\n\n{error:#}", role.key()),
        ),
    });
}

#[allow(clippy::too_many_arguments)]
async fn try_prompt_for_agent<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
) -> anyhow::Result<AgentPickerResolution>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    if workspace.default_agent.is_some() {
        return Ok(AgentPickerResolution::NotNeeded);
    }

    draw_role_resolution_dialog(terminal, state, config, cwd, role)?;
    Ok(
        match open_inline_agent_picker(state, paths, config, runner, role).await {
            Ok(true) => AgentPickerResolution::Opened,
            Ok(false) => AgentPickerResolution::NotNeeded,
            Err(error) => AgentPickerResolution::Failed(error),
        },
    )
}

/// Outcome of `prompt_agent_for_launch`. Two states because callers
/// only branch on "the helper already drives the next interaction"
/// (`Defer`) vs "no prompt was needed, launch immediately" (`Launch`).
enum PromptOutcome {
    Launch,
    Defer,
}

/// Whether `prompt_agent_for_launch` should hold the pending-launch
/// pin so the operator can retry after dismissing the error popup.
/// Arms that pinned `pending_launch` upstream pass `RestorePending`;
/// arms that built `input` fresh from the key event pass `ClearPending`.
#[derive(Clone, Copy)]
enum OnPromptFailure {
    ClearPending,
    RestorePending,
}

#[allow(clippy::too_many_arguments)]
async fn prompt_agent_for_launch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    input: LoadWorkspaceInput,
    on_failure: OnPromptFailure,
) -> anyhow::Result<PromptOutcome>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    match try_prompt_for_agent(terminal, state, paths, config, cwd, runner, role, workspace).await?
    {
        AgentPickerResolution::Opened => {
            state.pending_launch = Some(input);
            Ok(PromptOutcome::Defer)
        }
        AgentPickerResolution::NotNeeded => Ok(PromptOutcome::Launch),
        AgentPickerResolution::Failed(error) => {
            if matches!(on_failure, OnPromptFailure::RestorePending) {
                state.pending_launch = Some(input);
            }
            show_role_resolution_error(state, role, &error);
            Ok(PromptOutcome::Defer)
        }
    }
}

async fn dispatch_and_prompt_launch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let Some((role, workspace, agent)) =
        state.dispatch_launch_for_workspace(config, cwd, input.clone())?
    else {
        return Ok(None);
    };
    if agent.is_some() {
        return Ok(Some(ConsoleOutcome::Launch(role, workspace, agent)));
    }
    match prompt_agent_for_launch(
        terminal,
        state,
        paths,
        config,
        cwd,
        runner,
        &role,
        &workspace,
        input,
        OnPromptFailure::ClearPending,
    )
    .await?
    {
        PromptOutcome::Launch => Ok(Some(ConsoleOutcome::Launch(role, workspace, None))),
        PromptOutcome::Defer => Ok(None),
    }
}

async fn prompt_committed_role<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: RoleSelector,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    // Rebuild the choice now so edits between open and commit take
    // effect. `take()` clears the pin even on concurrent delete.
    let Some(input) = state.pending_launch.take() else {
        return Ok(None);
    };
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
    match prompt_agent_for_launch(
        terminal,
        state,
        paths,
        config,
        cwd,
        runner,
        &role,
        &workspace,
        input,
        OnPromptFailure::RestorePending,
    )
    .await?
    {
        PromptOutcome::Launch => {
            state.pending_launch_role = None;
            Ok(Some(ConsoleOutcome::Launch(role, workspace, None)))
        }
        PromptOutcome::Defer => Ok(None),
    }
}

fn zai_key_present(config: &AppConfig, workspace_name: &str, role_selector: &str) -> bool {
    crate::operator_env::lookup_operator_env_raw(
        config,
        Some(role_selector),
        Some(workspace_name),
        "ZAI_API_KEY",
    )
    .is_some()
}

pub(in crate::console) fn providers_for_launch(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    agent: crate::agent::Agent,
) -> Vec<jackin_protocol::Provider> {
    jackin_protocol::Provider::available_for(
        agent.slug(),
        zai_key_present(config, workspace_name, role_selector),
    )
}

fn launch_with_committed_agent(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: crate::agent::Agent,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    let (Some(input), Some(role)) = (
        state.pending_launch.take(),
        state.pending_launch_role.take(),
    ) else {
        return Ok(None);
    };
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = preview::resolve_selected_workspace(config, cwd, &choice, &role)?;

    let providers = providers_for_launch(config, &choice.name, &role.key(), agent);
    if providers.is_empty() {
        return Ok(Some(ConsoleOutcome::Launch(role, workspace, Some(agent))));
    }

    if let ConsoleStage::Manager(ms) = &mut state.stage {
        ms.launch_provider_picker = Some(crate::console::manager::state::ProviderPickerState::new(
            role.clone(),
            agent,
            providers,
        ));
    }
    state.pending_launch = Some(input);
    state.pending_launch_role = Some(role);
    Ok(None)
}

#[allow(clippy::too_many_lines)]
pub async fn run_console(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    action_handler: &mut dyn InstanceActionHandler,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    use std::time::Duration;

    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    use crate::console::manager::state::{ManagerStage, Modal};

    let mut state = ConsoleState::new(&config, cwd)?;
    // When the launch flow in `app` already owns the host screen, draw into it
    // and leave teardown to that guard; otherwise own the screen here for the
    // lifetime of the console (standalone `jackin console` with no launch).
    let owned_screen = if crate::tui::host_screen_owned() {
        None
    } else {
        Some(TerminalSession::enter()?)
    };
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut last_mouse_event_at: Option<std::time::Instant> = None;
    // Tracks whether the terminal pointer is currently the hand/`pointer`
    // shape, so OSC 22 is emitted only when the hover crosses a clickable
    // boundary rather than on every motion event.
    let mut pointer_is_hand = false;

    let result = 'main: loop {
        // Drain a pending token-generate request before render: suspend
        // the TUI, run the interactive `claude setup-token` mint + the
        // 1Password write, then resume. Done at the top of the loop (no
        // live `&mut state.stage` borrow, `config`/`paths`/`terminal` all
        // in scope) so a request set by the previous iteration's input is
        // handled before the next frame.
        let pending = if let ConsoleStage::Manager(ms) = &mut state.stage {
            match &mut ms.stage {
                ManagerStage::Editor(ed) => ed.pending_token_generate.take(),
                ManagerStage::Settings(s) => s.pending_token_generate.take(),
                _ => None,
            }
        } else {
            None
        };
        if let Some(req) = pending {
            use crate::workspace::token_setup::TokenSetupScope;
            let mut out = std::io::stdout();
            suspend_console_terminal(&mut out);
            let label = match &req.scope {
                TokenSetupScope::Workspace(name) => format!("workspace {name:?}"),
                TokenSetupScope::WorkspaceRole { workspace, role } => {
                    format!("workspace {workspace:?} role {role:?}")
                }
                TokenSetupScope::Global => "global config".to_string(),
            };
            println!(
                "\nGenerating Claude OAuth token for {label} — complete the browser \
                 sign-in, then paste the code below.\n",
            );
            // Mint without persisting: the op item is created / the
            // literal is captured and validated, but jackin config is NOT
            // written here. The minted value is staged into the stashed
            // auth form (re-mounted below) and persisted only when the
            // operator Saves — mirroring the provide path's "pick a value
            // → form re-mounts with the credential, focus Save → Save".
            let mint = crate::workspace::token_setup::mint_token_value(
                paths, &config, &req.scope, &req.args,
            );
            let _ = resume_console_terminal(&mut out);
            // Force a full redraw next frame so leftover child output is
            // cleared before the TUI repaints.
            let _ = terminal.clear();
            match mint {
                Ok(env_value) => {
                    // A successful op mint created or edited an item/field;
                    // drop the stale cached item/field lists so a reopened
                    // picker shows the new entry without a manual refresh.
                    if let (
                        crate::operator_env::EnvValue::OpRef(op_ref),
                        ConsoleStage::Manager(ms),
                    ) = (&env_value, &state.stage)
                    {
                        invalidate_op_cache_for_ref(&ms.op_cache, op_ref);
                    }
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        match &mut ms.stage {
                            // Re-mount the stashed auth form with the minted
                            // credential applied (op vs. plain), focus Save —
                            // the same helpers the provide path uses. The
                            // operator's Save then runs the normal
                            // persist_form → editor save that writes config.
                            ManagerStage::Editor(ed) => match env_value {
                                crate::operator_env::EnvValue::OpRef(op_ref) => {
                                    crate::console::manager::input::auth::apply_op_picker_to_auth_form(
                                        ed, op_ref,
                                    );
                                }
                                crate::operator_env::EnvValue::Plain(value) => {
                                    crate::console::manager::input::auth::apply_plain_text_to_auth_form(
                                        ed, &value,
                                    );
                                }
                            },
                            // Settings (global Claude) re-mounts via its own
                            // equivalents on the stashed settings auth form.
                            ManagerStage::Settings(s) => match env_value {
                                crate::operator_env::EnvValue::OpRef(op_ref) => {
                                    crate::console::manager::input::apply_op_picker_to_settings_auth_form(
                                        &mut s.auth, op_ref,
                                    );
                                }
                                crate::operator_env::EnvValue::Plain(value) => {
                                    crate::console::manager::input::apply_plain_text_to_settings_auth_form(
                                        &mut s.auth, &value,
                                    );
                                }
                            },
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        match &mut ms.stage {
                            ManagerStage::Editor(ed) => {
                                ed.modal = Some(Modal::ErrorPopup {
                                    state:
                                        crate::console::widgets::error_popup::ErrorPopupState::new(
                                            "Token generation failed",
                                            e.to_string(),
                                        ),
                                });
                            }
                            // Settings surfaces errors through its top-level
                            // error popup slot (same widget as the editor).
                            ManagerStage::Settings(s) => {
                                s.error_popup = Some(
                                    crate::console::widgets::error_popup::ErrorPopupState::new(
                                        "Token generation failed",
                                        e.to_string(),
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            continue;
        }

        // Drain worker results before render so a fresh result lands
        // this frame instead of a stale Loading one.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            manager::input::poll_background_loads(ms, &mut config, paths);
            if let Some(result) = ms.poll_instance_refresh(paths) {
                let _ = manager::update_manager(
                    ms,
                    manager::ManagerMessage::InstancesRefreshed(result),
                );
            }
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
                            if let Some(outcome) = dispatch_and_prompt_launch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                LoadWorkspaceInput::Saved(name),
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchCurrentDir => {
                            if let Some(outcome) = dispatch_and_prompt_launch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                LoadWorkspaceInput::CurrentDir,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchWithAgent(role) => {
                            if let Some(outcome) = prompt_committed_role(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                role,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchWithRuntimeAgent(agent) => {
                            if let Some(outcome) =
                                launch_with_committed_agent(&mut state, &config, cwd, agent)?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::NewSessionWithProvider {
                            container,
                            agent,
                            provider,
                        } => {
                            break 'main Ok(Some(ConsoleOutcome::NewSessionWithProvider {
                                container,
                                agent,
                                provider,
                            }));
                        }
                        manager::InputOutcome::LaunchWithProvider {
                            selector,
                            agent,
                            provider,
                        } => {
                            // Propagate resolution errors rather than mapping
                            // them to a silent no-op: an operator who confirmed
                            // a provider must see why the launch could not
                            // resolve. A genuinely absent pending input / choice
                            // is the only Ok(None) path.
                            let workspace = match state.pending_launch.take() {
                                Some(input) => {
                                    match build_workspace_choice(&config, cwd, &input)? {
                                        Some(choice) => Some(preview::resolve_selected_workspace(
                                            &config, cwd, &choice, &selector,
                                        )?),
                                        None => None,
                                    }
                                }
                                None => None,
                            };
                            let Some(workspace) = workspace else {
                                break 'main Ok(None);
                            };
                            break 'main Ok(Some(ConsoleOutcome::LaunchWithProvider {
                                selector,
                                workspace,
                                agent,
                                provider,
                            }));
                        }
                        manager::InputOutcome::InstanceAction { container, action } => {
                            if action.runs_in_place() {
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    let busy_title = match action {
                                        ConsoleInstanceAction::Stop => "Stopping",
                                        ConsoleInstanceAction::Purge => "Purging",
                                        _ => "Working",
                                    };
                                    let busy_body = format!("{busy_title} {container}…");
                                    ms.list_modal = Some(manager::state::Modal::ErrorPopup {
                                        state: widgets::error_popup::ErrorPopupState::new(
                                            busy_title, busy_body,
                                        ),
                                    });
                                    terminal.draw(|frame| {
                                        manager::render(frame, ms, &config, cwd);
                                    })?;
                                }
                                let result = action_handler.run_in_place(&container, action);
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    ms.list_modal = None;
                                    if let Err(error) = result {
                                        let err_title = match action {
                                            ConsoleInstanceAction::Stop => "Stop failed",
                                            ConsoleInstanceAction::Purge => "Purge failed",
                                            _ => "Action failed",
                                        };
                                        ms.list_modal = Some(manager::state::Modal::ErrorPopup {
                                            state: widgets::error_popup::ErrorPopupState::new(
                                                err_title,
                                                format!("{error:#}"),
                                            ),
                                        });
                                    }
                                    ms.force_refresh_instances();
                                }
                                continue;
                            }
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
                        // Switch the terminal pointer to the hand shape over any
                        // clickable element (and back off it), per the clickable
                        // affordance rule — only when the state changes.
                        let hand =
                            manager::input::clickable_at(ms, mouse, term_size, Some(&config));
                        if hand != pointer_is_hand {
                            pointer_is_hand = hand;
                            let seq = if hand {
                                jackin_tui::ansi::POINTER_HAND
                            } else {
                                jackin_tui::ansi::POINTER_DEFAULT
                            };
                            let mut out = std::io::stdout();
                            let _ = std::io::Write::write_all(&mut out, seq.as_bytes());
                            let _ = std::io::Write::flush(&mut out);
                        }
                    }
                }
                _ => {}
            }
        }
    };

    // Tears down only when the console owns the screen standalone. When the
    // launch flow owns it, this is `None` and teardown waits for that guard so
    // the console → loading transition stays on one alternate screen.
    drop(owned_screen);
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

    #[test]
    fn show_role_resolution_error_opens_error_popup_with_role_and_error() {
        let mut state = fresh_state();
        let selector = RoleSelector::new(Some("acme"), "agent-smith");
        let error = anyhow::anyhow!("network is unreachable");

        show_role_resolution_error(&mut state, &selector, &error);

        let ConsoleStage::Manager(ms) = &mut state.stage;
        let Some(Modal::ErrorPopup { state: popup }) = ms.list_modal.as_ref() else {
            panic!("expected ErrorPopup, got {:?}", ms.list_modal);
        };
        let body = format!("{popup:?}");
        assert!(
            body.contains("acme/agent-smith"),
            "popup must reference the failing role selector: {body}"
        );
        assert!(
            body.contains("network is unreachable"),
            "popup must surface the underlying error: {body}"
        );
    }

    #[test]
    fn providers_for_launch_include_all_zai_env_layers() {
        let mut config = AppConfig::default();
        config.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("global-key".into()),
        );
        config.workspaces.insert(
            "global-demo".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        assert_eq!(
            super::providers_for_launch(
                &config,
                "global-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );
        config.env.clear();

        let mut workspace = crate::workspace::WorkspaceConfig::default();
        workspace.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-key".into()),
        );
        config.workspaces.insert("workspace-demo".into(), workspace);
        assert_eq!(
            super::providers_for_launch(
                &config,
                "workspace-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );

        config.workspaces.remove("workspace-demo");
        let mut role = crate::config::RoleSource::default();
        role.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("role-key".into()),
        );
        config.roles.insert("the-architect".into(), role);
        config.workspaces.insert(
            "role-demo".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        assert_eq!(
            super::providers_for_launch(
                &config,
                "role-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );

        config.roles.clear();
        let mut workspace_role = crate::workspace::WorkspaceConfig::default();
        let mut role_override = crate::workspace::WorkspaceRoleOverride::default();
        role_override.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-role-key".into()),
        );
        workspace_role
            .roles
            .insert("the-architect".into(), role_override);
        config
            .workspaces
            .insert("workspace-role-demo".into(), workspace_role);
        let providers = super::providers_for_launch(
            &config,
            "workspace-role-demo",
            "the-architect",
            crate::agent::Agent::Claude,
        );
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[1], jackin_protocol::Provider::Zai);
    }

    #[test]
    fn providers_for_launch_rejects_non_claude_agents() {
        let mut config = AppConfig::default();
        config.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("global-key".into()),
        );
        config
            .workspaces
            .insert("demo".into(), crate::workspace::WorkspaceConfig::default());

        let providers = super::providers_for_launch(
            &config,
            "demo",
            "the-architect",
            crate::agent::Agent::Codex,
        );

        assert!(providers.is_empty());
    }

    fn unresolved_workspace() -> ResolvedWorkspace {
        ResolvedWorkspace {
            label: "scratch".to_string(),
            workdir: "/workspace".to_string(),
            mounts: Vec::new(),
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
        }
    }

    async fn run_prompt_for_unknown_role(
        on_failure: OnPromptFailure,
    ) -> (ConsoleState, PromptOutcome) {
        use ratatui::backend::TestBackend;
        let cwd = std::env::temp_dir();
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        // Empty config → resolve_supported_agents_for_console errors on
        // the unregistered selector; helper routes that into Failed.
        let config = AppConfig::default();
        let mut state = ConsoleState::new(&config, &cwd).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let workspace = unresolved_workspace();
        let mut runner = crate::runtime::FakeRunner::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let input = LoadWorkspaceInput::CurrentDir;
        let outcome = super::prompt_agent_for_launch(
            &mut terminal,
            &mut state,
            &paths,
            &config,
            &cwd,
            &mut runner,
            &selector,
            &workspace,
            input,
            on_failure,
        )
        .await
        .unwrap();
        (state, outcome)
    }

    #[tokio::test]
    async fn prompt_agent_for_launch_skips_resolution_when_workspace_default_agent_set() {
        // workspace.default_agent.is_some() must short-circuit before
        // any git work — operators with a configured default never
        // wait on a network round trip just to confirm a launch.
        use ratatui::backend::TestBackend;
        let cwd = std::env::temp_dir();
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ConsoleState::new(&config, &cwd).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut workspace = unresolved_workspace();
        workspace.default_agent = Some(crate::agent::Agent::Codex);
        let mut runner = crate::runtime::FakeRunner::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        let outcome = super::prompt_agent_for_launch(
            &mut terminal,
            &mut state,
            &paths,
            &config,
            &cwd,
            &mut runner,
            &selector,
            &workspace,
            LoadWorkspaceInput::CurrentDir,
            OnPromptFailure::ClearPending,
        )
        .await
        .unwrap();

        assert!(matches!(outcome, PromptOutcome::Launch));
        assert!(
            runner.recorded.is_empty(),
            "workspace default_agent must short-circuit before any git work: {:?}",
            runner.recorded
        );
        let ConsoleStage::Manager(ms) = &state.stage;
        assert!(
            ms.list_modal.is_none(),
            "no modal must be opened on the default-agent short-circuit"
        );
        assert!(
            ms.status_overlay.is_none(),
            "no status overlay must be left behind on the default-agent short-circuit"
        );
    }

    #[tokio::test]
    async fn prompt_agent_for_launch_restore_pending_keeps_input_for_retry() {
        let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::RestorePending).await;
        assert!(matches!(outcome, PromptOutcome::Defer));
        assert!(
            state.pending_launch.is_some(),
            "RestorePending must hold the input so the operator can retry after dismissing the error"
        );
        let ConsoleStage::Manager(ms) = &state.stage;
        assert!(
            matches!(ms.list_modal, Some(Modal::ErrorPopup { .. })),
            "Failed outcome must surface the error popup regardless of restore policy"
        );
    }

    #[tokio::test]
    async fn prompt_agent_for_launch_clear_pending_drops_input() {
        let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::ClearPending).await;
        assert!(matches!(outcome, PromptOutcome::Defer));
        assert!(
            state.pending_launch.is_none(),
            "ClearPending must drop the input so a fresh workspace pick re-resolves cleanly"
        );
        let ConsoleStage::Manager(ms) = &state.stage;
        assert!(
            matches!(ms.list_modal, Some(Modal::ErrorPopup { .. })),
            "Failed outcome must surface the error popup regardless of restore policy"
        );
    }
}

#[cfg(test)]
mod op_cache_invalidation_tests {
    use super::invalidate_op_cache_for_ref;
    use crate::console::op_cache::OpCache;
    use crate::operator_env::{OpField, OpItem, OpRef};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn invalidate_op_cache_for_ref_drops_items_and_fields() {
        let cache = Rc::new(RefCell::new(OpCache::default()));
        let account = Some("ACCT");
        cache.borrow_mut().put_items(
            account,
            "v1",
            vec![OpItem {
                id: "i1".into(),
                name: "Claude".into(),
                subtitle: String::new(),
            }],
        );
        cache.borrow_mut().put_fields(
            account,
            "v1",
            "i1",
            vec![OpField {
                id: "f1".into(),
                label: "token".into(),
                field_type: "CONCEALED".into(),
                concealed: true,
                reference: String::new(),
            }],
        );

        invalidate_op_cache_for_ref(
            &cache,
            &OpRef {
                op: "op://v1/i1/f1".into(),
                path: "Work/Claude/token".into(),
                account: Some("ACCT".into()),
            },
        );

        assert!(cache.borrow().get_items(account, "v1").is_none());
        assert!(cache.borrow().get_fields(account, "v1", "i1").is_none());
    }

    #[test]
    fn invalidate_op_cache_for_ref_ignores_unparseable_ref() {
        let cache = Rc::new(RefCell::new(OpCache::default()));
        // A non-op:// literal must be a no-op, not a panic.
        invalidate_op_cache_for_ref(
            &cache,
            &OpRef {
                op: "not-a-ref".into(),
                path: String::new(),
                account: None,
            },
        );
    }
}
