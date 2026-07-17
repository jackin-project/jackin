// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console TUI run entry: terminal setup, raw mode, cleanup on exit.
//!
//! Drives the per-tick event loop, input dispatch, and suspend/resume around
//! container attach. Not responsible for state construction (`app.rs`) or
//! individual dialog rendering — those are in sibling modules and
//! `jackin-console`.

use crate::console::terminal::{
    MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS, TerminalSession, host_console_terminal,
};
use crate::console::{ConsoleOutcome, ConsoleStage, ConsoleState, InstanceActionHandler};
use jackin_console::tui::components::error_popup::{
    instance_action_failed_error_message, instance_action_failed_error_title,
};
use jackin_console::tui::components::status_popup::{
    instance_action_busy_message, instance_action_busy_title,
};
use jackin_console::tui::message::PromptOutcome;
use jackin_console::tui::message::launch_prompt_should_probe_agents;
use jackin_console::tui::model::{clear_pending_launch_role_plan, take_pending_launch_plan};
use jackin_console::tui::prompts::{
    ConcreteAgentPickerChoices as AgentPickerChoices,
    ConcreteLaunchPromptDispatch as LaunchPromptDispatch,
    ConcreteLaunchPromptRequest as LaunchPromptRequest, committed_role_prompt,
    dispatch_launch_prompt, draw_role_resolution_dialog, launch_with_committed_agent,
    prompt_agent_for_launch,
};
use jackin_console::tui::run::{
    ConsoleChromeHover, ConsoleModalMouseLayerFacts, QuitConfirmPlan, console_pointer_shape,
    debug_chip_activation_allowed, debug_chip_row, debug_invocation_id_label,
    modal_mouse_layer_plan, quit_confirm_area, quit_intercept_state_for_console,
    should_open_quit_confirm, split_debug_area, startup_error_dismissed,
    startup_error_modal_active_for_console, token_generate_scope_label_for_console,
    token_generate_status_message,
};

use jackin_config::AppConfig;
use jackin_config::LoadWorkspaceInput;
use jackin_core::JackinPaths;

pub struct ConsoleRunOptions<'a> {
    pub op_available: bool,
    pub startup_error: Option<(String, String)>,
    pub parent_session: Option<&'a TerminalSession>,
}

impl std::fmt::Debug for ConsoleRunOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleRunOptions")
            .field("op_available", &self.op_available)
            .field("startup_error", &self.startup_error)
            .field("parent_session_present", &self.parent_session.is_some())
            .finish()
    }
}

/// Which telemetry screen the visible manager stage maps to. Confirm dialogs
/// overlay the list, so they stay on `List`; the create *prelude* and the
/// field editor are distinct screens (the create flow shows as `create` then
/// `editor`).
pub(crate) const fn screen_of(state: &ConsoleState) -> jackin_telemetry::schema::enums::ScreenId {
    use crate::console::adapter::state::ManagerStage;
    use jackin_telemetry::schema::enums::ScreenId;

    let ConsoleStage::Manager(ms) = &state.stage;
    match ms.stage {
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ScreenId::WorkspaceList,
        ManagerStage::Editor(_) => ScreenId::WorkspaceEditor,
        ManagerStage::Settings(_) => ScreenId::Settings,
        ManagerStage::CreatePrelude(_) => ScreenId::WorkspaceCreate,
    }
}

/// True iff no modal overlay is currently blocking input on the console surface.
///
/// Used by the mouse routing layer to enforce single-consumer precedence: when
/// this returns `false`, chrome interactions (debug chip) and base-surface mouse
/// handling are suppressed so only the active modal handles the event.
pub(crate) const fn no_modal_open(state: &ConsoleState) -> bool {
    use crate::console::adapter::state::ManagerStage;
    let ConsoleStage::Manager(ms) = &state.stage;
    state.quit_confirm.is_none()
        && ms.list_modal.is_none()
        && !matches!(&ms.stage, ManagerStage::Editor(e) if e.modal.is_some())
}

async fn execute_launch_prompt<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl jackin_docker::CommandRunner,
    request: LaunchPromptRequest,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let should_probe_agents =
        launch_prompt_should_probe_agents(request.workspace.default_agent.is_some());
    if should_probe_agents {
        draw_role_resolution_dialog(terminal, state, config, cwd, &request.role)?;
    }
    let choices = if should_probe_agents {
        match crate::console::services::agents::load_inline_picker_choices(
            paths,
            config,
            &request.role,
            runner,
        )
        .await
        {
            Ok(Some(choices)) => AgentPickerChoices::Choices(choices),
            Ok(None) => AgentPickerChoices::NotNeeded,
            Err(error) => AgentPickerChoices::Failed(error),
        }
    } else {
        AgentPickerChoices::NotNeeded
    };
    match prompt_agent_for_launch(
        state,
        &request.role,
        &request.workspace,
        request.input,
        request.on_failure,
        choices,
    ) {
        PromptOutcome::Launch => {
            clear_pending_launch_role_plan(state);
            Ok(Some(ConsoleOutcome::Launch(
                request.role,
                request.workspace,
                None,
            )))
        }
        PromptOutcome::Defer => Ok(None),
    }
}

async fn execute_launch_prompt_dispatch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl jackin_docker::CommandRunner,
    dispatch: LaunchPromptDispatch,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    match dispatch {
        LaunchPromptDispatch::Launch(outcome) => Ok(Some(outcome)),
        LaunchPromptDispatch::Prompt(request) => {
            execute_launch_prompt(terminal, state, paths, config, cwd, runner, request).await
        }
        LaunchPromptDispatch::None => Ok(None),
    }
}

enum ConsoleLoopFlow {
    Continue,
    Exit(Option<ConsoleOutcome>),
}

struct ConsoleLoopInputs<'a, H, R> {
    config: &'a mut AppConfig,
    paths: &'a JackinPaths,
    cwd: &'a std::path::Path,
    action_handler: &'a mut H,
    runner: &'a mut R,
    startup_error_pending: bool,
}

struct ConsoleMouseState {
    last_event_at: Option<std::time::Instant>,
    pointer_shape: termrock::osc::PointerShape,
    chrome_regions: Vec<termrock::interaction::HitRegion<ConsoleChromeHover>>,
    chrome_hover: Option<ConsoleChromeHover>,
}

impl ConsoleMouseState {
    fn new() -> Self {
        Self {
            last_event_at: None,
            pointer_shape: termrock::osc::PointerShape::Default,
            chrome_regions: Vec::new(),
            chrome_hover: None,
        }
    }
}

fn sync_active_screen(
    state: &ConsoleState,
    tracker: &mut jackin_telemetry::ui::ScreenVisitTracker,
    action_parent: Option<&jackin_telemetry::ui::ActionParent>,
) {
    let screen = screen_of(state);
    if tracker.current_screen().is_none() {
        let _screen_result = tracker.enter(screen);
    } else if tracker.current_screen() != Some(screen) {
        let reason = if action_parent.is_some() {
            jackin_telemetry::schema::enums::TransitionReason::Action
        } else {
            jackin_telemetry::schema::enums::TransitionReason::Completion
        };
        let _screen_result = tracker.transition(screen, reason, action_parent);
    }
}

fn widget_of(state: &ConsoleState) -> Option<&'static str> {
    use crate::console::adapter::state::ManagerStage;
    use jackin_console::tui::screens::editor::model::EditorTab;
    use jackin_console::tui::screens::settings::model::SettingsTab;

    let ConsoleStage::Manager(ms) = &state.stage;
    match &ms.stage {
        ManagerStage::Editor(editor) => Some(match editor.active_tab {
            EditorTab::General => "general",
            EditorTab::Mounts => "mounts",
            EditorTab::Roles => "roles",
            EditorTab::Secrets => "secrets_environments",
            EditorTab::Auth => "auth",
        }),
        ManagerStage::Settings(settings) => Some(match settings.active_tab {
            SettingsTab::General => "general",
            SettingsTab::Mounts => "mounts",
            SettingsTab::Environments => "environments",
            SettingsTab::Auth => "auth",
            SettingsTab::Trust => "trust",
        }),
        _ => None,
    }
}

fn sync_widget_focus(
    state: &ConsoleState,
    tracker: &mut jackin_telemetry::ui::WidgetFocusTracker,
    action_parent: Option<&jackin_telemetry::ui::ActionParent>,
) {
    let next = widget_of(state);
    if tracker.current_widget() == next {
        return;
    }
    let _focus_result = if let Some(parent) = action_parent {
        parent.in_scope(|| {
            if let Some(widget) = next {
                tracker.focus(widget)
            } else {
                tracker.unfocus()
            }
        })
    } else if let Some(widget) = next {
        tracker.focus(widget)
    } else {
        tracker.unfocus()
    };
}

fn input_outcome_action(
    outcome: &crate::console::adapter::InputOutcome,
) -> Option<jackin_telemetry::schema::enums::UiActionName> {
    use crate::console::adapter::InputOutcome;
    use jackin_telemetry::schema::enums::UiActionName;

    match outcome {
        InputOutcome::ExitJackin => Some(UiActionName::AppExitRequest),
        InputOutcome::LaunchNamed(_)
        | InputOutcome::LaunchCurrentDir
        | InputOutcome::LaunchWithAgent(_)
        | InputOutcome::LaunchWithRuntimeAgent(_)
        | InputOutcome::LaunchWithProvider { .. } => Some(UiActionName::WorkspaceLaunch),
        InputOutcome::PrewarmNamed(_) | InputOutcome::NewSessionWithProvider { .. } => {
            Some(UiActionName::AgentSpawn)
        }
        InputOutcome::InstanceAction {
            action:
                crate::console::ConsoleInstanceAction::NewSession
                | crate::console::ConsoleInstanceAction::NewSessionWithAgent(_),
            ..
        } => Some(UiActionName::AgentSpawn),
        InputOutcome::Continue | InputOutcome::InstanceAction { .. } => None,
    }
}

fn drain_background_messages(
    state: &mut ConsoleState,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    needs_redraw: &mut bool,
) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let messages = crate::console::effects::poll_background_messages(ms, config, paths);
    for message in messages {
        *needs_redraw |=
            crate::console::effects::apply_background_event(ms, config, paths, cwd, message);
    }
}

struct DrawConsoleContext<'a> {
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
    mouse_state: &'a mut ConsoleMouseState,
    container_info_overlay_active: &'a mut bool,
    action_parent: Option<&'a jackin_telemetry::ui::ActionParent>,
    jank_monitor: &'a mut jackin_telemetry::ui::JankMonitor,
}

fn draw_console_frame<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    context: DrawConsoleContext<'_>,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let full_area: ratatui::layout::Rect = terminal.size()?.into();
    let (main_area, debug_bar_area) =
        split_debug_area(full_area, jackin_diagnostics::is_debug_mode());
    {
        // Scoped mutable borrow of `state.stage`: released before the
        // `View<ConsoleState>` dispatch below needs an immutable borrow of
        // the whole `state`.
        let ConsoleStage::Manager(ms) = &mut state.stage;
        if *context.container_info_overlay_active
            && !matches!(
                ms.list_modal,
                Some(crate::console::adapter::state::Modal::ContainerInfo { .. })
            )
        {
            terminal.clear()?;
            *context.container_info_overlay_active = false;
        }
        crate::console::adapter::prepare_for_render(ms, context.config, context.cwd, main_area);
    }

    // Route the primary render through the shared `View<ConsoleState>`
    // dispatch instead of calling `crate::console::adapter::render`
    // directly. The confirm-dialog/debug-bar overlay compositing that used to
    // share the same `terminal.draw` closure is not part of the `View`
    // contract — it stays an `overlay` closure that `drive_frame` runs
    // against the same in-progress frame, unchanged from before.
    let view = jackin_console::tui::runtime::ConsoleView {
        context: jackin_console::tui::runtime::ConsoleViewContext {
            config: context.config,
            cwd: context.cwd,
        },
    };
    let confirm_state = state.quit_confirm.as_ref();
    let screen = screen_of(state);
    let render_started = std::time::Instant::now();
    let render_attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::std_attrs::APP_SCREEN_ID,
        value: jackin_telemetry::Value::Str(screen.as_str()),
    }];
    let render_operation = context.action_parent.and_then(|parent| {
        parent.in_scope(|| {
            jackin_telemetry::operation(&jackin_telemetry::operation::UI_RENDER, &render_attrs).ok()
        })
    });
    let render_result =
        jackin_tui::runtime::drive_frame(terminal, &view, &*state, main_area, |frame| {
            if let Some(confirm) = confirm_state {
                let hint_row = ratatui::layout::Rect {
                    x: main_area.x,
                    y: main_area.bottom().saturating_sub(1),
                    width: main_area.width,
                    height: 1,
                };
                let body = ratatui::layout::Rect {
                    height: main_area.height.saturating_sub(1),
                    ..main_area
                };
                jackin_console::tui::view::render_modal_backdrop(frame, body);
                let area = quit_confirm_area(body, confirm);
                jackin_console::tui::components::render_confirm_dialog(frame, area, confirm);
                termrock::widgets::render_hint_bar(
                    frame,
                    hint_row,
                    &jackin_console::tui::components::confirm_hint_spans(),
                    &termrock::Theme::default(),
                );
            }
            context.mouse_state.chrome_regions.clear();
            if let Some(bar_area) = debug_bar_area {
                let invocation_id = jackin_telemetry::identity::current_invocation()
                    .map(|invocation_id| invocation_id.to_string());
                let invocation_id = debug_invocation_id_label(invocation_id.as_deref());
                let chip_row = debug_chip_row(bar_area);
                let content = format!(" {invocation_id} ");
                let slots = [termrock::widgets::StatusSlot {
                    id: ConsoleChromeHover::DebugChip,
                    content: &content,
                    priority: 1,
                    min_width: 0,
                    enabled: true,
                    style: ratatui::style::Style::default()
                        .bg(termrock::Theme::default()
                            .style(termrock::style::Role::Danger)
                            .fg
                            .unwrap_or_default())
                        .fg(termrock::Theme::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                    hover_style: Some(
                        ratatui::style::Style::default()
                            .bg(termrock::Theme::default()
                                .style(termrock::style::Role::Text)
                                .fg
                                .unwrap_or_default())
                            .fg(termrock::Theme::default()
                                .style(termrock::style::Role::Danger)
                                .fg
                                .unwrap_or_default())
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    ),
                }];
                let mut status_state = termrock::widgets::StatusBarState {
                    hovered: (context.mouse_state.chrome_hover
                        == Some(ConsoleChromeHover::DebugChip))
                    .then_some(ConsoleChromeHover::DebugChip),
                    regions: Vec::new(),
                };
                let theme = termrock::Theme::default().with_role(
                    termrock::style::Role::StatusBar,
                    ratatui::style::Style::default()
                        .bg(termrock::Theme::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default())
                        .fg(jackin_tui::tokens::INK),
                );
                frame.render_stateful_widget(
                    &termrock::widgets::StatusBar::new(&[], &slots, &theme),
                    chip_row,
                    &mut status_state,
                );
                for region in status_state.regions {
                    context.mouse_state.chrome_regions.push(region);
                }
            }
        });
    if let Some(operation) = render_operation {
        operation.complete(
            if render_result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            render_result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::LaunchFailed),
        );
    }
    render_result?;
    context
        .jank_monitor
        .record_frame(screen, render_started.elapsed().as_secs_f64());

    let ConsoleStage::Manager(ms) = &state.stage;
    if let Some(modal @ crate::console::adapter::state::Modal::ContainerInfo { state: info }) =
        ms.list_modal.as_ref()
    {
        let rect = modal.rect(main_area);
        let overlay =
            jackin_console::tui::components::container_info_surface::hyperlink_overlay(rect, info);
        if !overlay.is_empty() {
            let mut out = std::io::stdout();
            drop(std::io::Write::write_all(&mut out, &overlay));
            drop(std::io::Write::flush(&mut out));
        }
        *context.container_info_overlay_active = true;
    }
    Ok(())
}

async fn dispatch_launch_input<B, H, R>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    R: jackin_docker::CommandRunner,
{
    let dispatch = dispatch_launch_prompt(state, inputs.config, inputs.cwd, input)?;
    execute_launch_prompt_dispatch(
        terminal,
        state,
        inputs.paths,
        inputs.config,
        inputs.cwd,
        inputs.runner,
        dispatch,
    )
    .await
}

async fn dispatch_committed_role<B, H, R>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    role: jackin_core::RoleSelector,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    R: jackin_docker::CommandRunner,
{
    let dispatch = committed_role_prompt(state, inputs.config, inputs.cwd, role)?;
    execute_launch_prompt_dispatch(
        terminal,
        state,
        inputs.paths,
        inputs.config,
        inputs.cwd,
        inputs.runner,
        dispatch,
    )
    .await
}

async fn handle_in_place_instance_action<B, H, R>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    container: &str,
    action: crate::console::ConsoleInstanceAction,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    H: InstanceActionHandler<jackin_core::Agent>,
{
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let action_fact = action.workspace_action_fact();
        let busy_title = instance_action_busy_title(action_fact);
        let busy_body = instance_action_busy_message(action_fact, container);
        let _unused = crate::console::adapter::update_manager(
            ms,
            crate::console::adapter::ManagerMessage::OpenStatusPopup {
                title: busy_title.into(),
                message: busy_body,
            },
        );
        terminal.draw(|frame| {
            let full_area = frame.area();
            let (main_area, _debug_bar) =
                split_debug_area(full_area, jackin_diagnostics::is_debug_mode());
            crate::console::adapter::render(frame, main_area, ms, inputs.config, inputs.cwd);
        })?;
    }
    let result = inputs.action_handler.run_in_place(container, action).await;
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let _unused = crate::console::adapter::update_manager(
        ms,
        crate::console::adapter::ManagerMessage::DismissStatusPopup,
    );
    if let Err(error) = result {
        let err_title = instance_action_failed_error_title(action.workspace_action_fact());
        let _unused = crate::console::adapter::update_manager(
            ms,
            crate::console::adapter::ManagerMessage::OpenListErrorPopup {
                title: err_title.into(),
                message: instance_action_failed_error_message(error),
            },
        );
    }
    ms.force_refresh_instances();
    Ok(())
}

async fn handle_input_outcome<B, H, R>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    outcome: crate::console::adapter::InputOutcome,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    needs_redraw: &mut bool,
) -> anyhow::Result<ConsoleLoopFlow>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    H: InstanceActionHandler<jackin_core::Agent>,
    R: jackin_docker::CommandRunner,
{
    if let Some(action) = input_outcome_action(&outcome) {
        jackin_telemetry::ui::record_action(action, screen_of(state), widget_of(state));
    }
    match outcome {
        crate::console::adapter::InputOutcome::Continue => {
            let ConsoleStage::Manager(ms) = &mut state.stage;
            if crate::console::effects::execute_pending_workspace_save_commit(
                ms,
                inputs.config,
                inputs.paths,
                inputs.cwd,
            )? {
                *needs_redraw = true;
            }
        }
        crate::console::adapter::InputOutcome::ExitJackin => {
            return Ok(ConsoleLoopFlow::Exit(None));
        }
        crate::console::adapter::InputOutcome::LaunchNamed(name) => {
            if let Some(outcome) =
                dispatch_launch_input(terminal, state, inputs, LoadWorkspaceInput::Saved(name))
                    .await?
            {
                return Ok(ConsoleLoopFlow::Exit(Some(outcome)));
            }
        }
        crate::console::adapter::InputOutcome::PrewarmNamed(name) => {
            return Ok(ConsoleLoopFlow::Exit(Some(ConsoleOutcome::PrewarmNamed(
                name,
            ))));
        }
        crate::console::adapter::InputOutcome::LaunchCurrentDir => {
            if let Some(outcome) =
                dispatch_launch_input(terminal, state, inputs, LoadWorkspaceInput::CurrentDir)
                    .await?
            {
                return Ok(ConsoleLoopFlow::Exit(Some(outcome)));
            }
        }
        crate::console::adapter::InputOutcome::LaunchWithAgent(role) => {
            if let Some(outcome) = dispatch_committed_role(terminal, state, inputs, role).await? {
                return Ok(ConsoleLoopFlow::Exit(Some(outcome)));
            }
        }
        crate::console::adapter::InputOutcome::LaunchWithRuntimeAgent(agent) => {
            if let Some(outcome) =
                launch_with_committed_agent(state, inputs.config, inputs.cwd, agent)?
            {
                return Ok(ConsoleLoopFlow::Exit(Some(outcome)));
            }
        }
        crate::console::adapter::InputOutcome::NewSessionWithProvider {
            container,
            agent,
            provider,
        } => {
            return Ok(ConsoleLoopFlow::Exit(Some(
                ConsoleOutcome::NewSessionWithProvider {
                    container,
                    agent,
                    provider,
                },
            )));
        }
        crate::console::adapter::InputOutcome::LaunchWithProvider {
            selector,
            agent,
            provider,
        } => {
            let Some(input) = take_pending_launch_plan(state) else {
                return Ok(ConsoleLoopFlow::Exit(None));
            };
            let workspace = jackin_console::services::launch::resolve_provider_launch_workspace(
                inputs.config,
                inputs.cwd,
                &input,
                &selector,
            )?;
            let Some(workspace) = workspace else {
                return Ok(ConsoleLoopFlow::Exit(None));
            };
            return Ok(ConsoleLoopFlow::Exit(Some(
                ConsoleOutcome::LaunchWithProvider {
                    selector,
                    workspace,
                    agent,
                    provider,
                },
            )));
        }
        crate::console::adapter::InputOutcome::InstanceAction { container, action } => {
            if action.runs_in_place() {
                handle_in_place_instance_action(terminal, state, inputs, &container, action)
                    .await?;
                *needs_redraw = true;
                return Ok(ConsoleLoopFlow::Continue);
            }
            return Ok(ConsoleLoopFlow::Exit(Some(
                ConsoleOutcome::InstanceAction { container, action },
            )));
        }
    }
    Ok(ConsoleLoopFlow::Continue)
}

async fn handle_key_event<B, H, R>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    key: crossterm::event::KeyEvent,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    mouse_state: &ConsoleMouseState,
    needs_redraw: &mut bool,
) -> anyhow::Result<ConsoleLoopFlow>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    H: InstanceActionHandler<jackin_core::Agent>,
    R: jackin_docker::CommandRunner,
{
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyModifiers};

    if matches!(key.code, KeyCode::Esc)
        && key.modifiers.is_empty()
        && mouse_state
            .last_event_at
            .is_some_and(|at| at.elapsed() <= Duration::from_millis(MOUSE_ESCAPE_GRACE_MS))
    {
        return Ok(ConsoleLoopFlow::Continue);
    }
    if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(ConsoleLoopFlow::Exit(None));
    }

    if let Some(plan) = state.handle_quit_confirm_key(key) {
        match plan {
            QuitConfirmPlan::Exit => return Ok(ConsoleLoopFlow::Exit(None)),
            QuitConfirmPlan::Dismiss | QuitConfirmPlan::Continue => {}
        }
        return Ok(ConsoleLoopFlow::Continue);
    }

    if should_open_quit_confirm(key, quit_intercept_state_for_console(state)) {
        state.open_quit_confirm();
        return Ok(ConsoleLoopFlow::Continue);
    }

    let outcome = {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        crate::console::adapter::handle_key(ms, inputs.config, inputs.paths, inputs.cwd, key)?
    };
    if startup_error_dismissed(state, inputs.startup_error_pending) {
        return Ok(ConsoleLoopFlow::Exit(None));
    }
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        for effect in ms.drain_effects() {
            *needs_redraw |= jackin_telemetry::ui::in_pending_action_scope(|| {
                crate::console::effects::execute_manager_effect(
                    ms,
                    inputs.config,
                    inputs.paths,
                    effect,
                )
            });
        }
    }
    handle_input_outcome(terminal, state, outcome, inputs, needs_redraw).await
}

fn reset_modal_mouse_state(mouse_state: &mut ConsoleMouseState, needs_redraw: &mut bool) {
    if mouse_state.chrome_hover.is_some() {
        mouse_state.chrome_hover = None;
        *needs_redraw = true;
    }
    if mouse_state.pointer_shape != termrock::osc::PointerShape::Default {
        mouse_state.pointer_shape = termrock::osc::PointerShape::Default;
        let mut out = std::io::stdout();
        let seq = termrock::osc::encode_pointer(mouse_state.pointer_shape);
        let _unused = std::io::Write::write_all(&mut out, &seq);
        drop(std::io::Write::flush(&mut out));
    }
}

fn update_console_pointer_shape(
    ms: &jackin_console::tui::state::ManagerState<'_>,
    config: &AppConfig,
    mouse: crossterm::event::MouseEvent,
    term_size: ratatui::layout::Rect,
    mouse_state: &mut ConsoleMouseState,
    needs_redraw: &mut bool,
    no_modal_open: bool,
) {
    let next_chrome_hover = if no_modal_open {
        mouse_state
            .chrome_regions
            .iter()
            .find(|region| {
                region
                    .area
                    .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
            })
            .map(|region| region.id)
    } else {
        None
    };
    if next_chrome_hover != mouse_state.chrome_hover {
        mouse_state.chrome_hover = next_chrome_hover;
        *needs_redraw = true;
    }
    let next_pointer_shape = console_pointer_shape(
        mouse_state.chrome_hover.is_some(),
        crate::console::adapter::input::clickable_at(ms, mouse, term_size, Some(config)),
    );
    if next_pointer_shape != mouse_state.pointer_shape {
        mouse_state.pointer_shape = next_pointer_shape;
        let seq = termrock::osc::encode_pointer(mouse_state.pointer_shape);
        let mut out = std::io::stdout();
        drop(std::io::Write::write_all(&mut out, &seq));
        drop(std::io::Write::flush(&mut out));
    }
}

fn handle_mouse_event<H, R>(
    state: &mut ConsoleState,
    mouse: crossterm::event::MouseEvent,
    term_size: ratatui::layout::Rect,
    inputs: &mut ConsoleLoopInputs<'_, H, R>,
    mouse_state: &mut ConsoleMouseState,
    needs_redraw: &mut bool,
) -> anyhow::Result<ConsoleLoopFlow> {
    mouse_state.last_event_at = Some(std::time::Instant::now());
    let no_modal_open = no_modal_open(state);
    let modal_plan = {
        let (main_area, _) = split_debug_area(term_size, jackin_diagnostics::is_debug_mode());
        let quit_confirm_rect = state
            .quit_confirm_state()
            .map(|confirm| quit_confirm_area(main_area, confirm));
        let ConsoleStage::Manager(ms) = &state.stage;
        let list_modal_rect = ms.list_modal.as_ref().map(|modal| modal.rect(main_area));
        modal_mouse_layer_plan(
            mouse,
            ConsoleModalMouseLayerFacts {
                quit_confirm_rect,
                list_modal_rect,
                list_modal_container_info: matches!(
                    ms.list_modal,
                    Some(crate::console::adapter::state::Modal::ContainerInfo { .. })
                ),
                startup_error_modal_active: startup_error_modal_active_for_console(
                    state,
                    inputs.startup_error_pending,
                ),
            },
        )
    };
    if modal_plan.dismiss_quit_confirm {
        state.dismiss_quit_confirm();
    }
    if modal_plan.dismiss_list_modal {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let _unused = crate::console::adapter::update_manager(
            ms,
            crate::console::adapter::ManagerMessage::DismissListModal,
        );
    }

    if modal_plan.consumed {
        if startup_error_dismissed(state, inputs.startup_error_pending) {
            return Ok(ConsoleLoopFlow::Exit(None));
        }
        reset_modal_mouse_state(mouse_state, needs_redraw);
        return Ok(ConsoleLoopFlow::Continue);
    }

    let ConsoleStage::Manager(ms) = &mut state.stage;
    let debug_chip_hovered = mouse_state.chrome_hover == Some(ConsoleChromeHover::DebugChip);
    let active_run = jackin_diagnostics::active_run();
    if debug_chip_activation_allowed(
        mouse,
        no_modal_open,
        debug_chip_hovered,
        active_run.is_some(),
    ) && let Some(run) = active_run
    {
        let _unused = crate::console::adapter::update_manager(
            ms,
            crate::console::adapter::ManagerMessage::OpenListContainerInfo {
                state: jackin_console::tui::components::container_info::debug_run_info_state(
                    env!("JACKIN_VERSION"),
                    run.run_id(),
                ),
            },
        );
    }

    let _outcome = crate::console::adapter::input::handle_mouse_with_config(
        ms,
        mouse,
        term_size,
        Some(inputs.config),
    );
    for effect in ms.drain_effects() {
        *needs_redraw |= crate::console::effects::execute_manager_effect(
            ms,
            inputs.config,
            inputs.paths,
            effect,
        );
    }
    update_console_pointer_shape(
        ms,
        inputs.config,
        mouse,
        term_size,
        mouse_state,
        needs_redraw,
        no_modal_open,
    );
    Ok(ConsoleLoopFlow::Continue)
}

async fn next_event_batch(
    event_stream: &mut crossterm::event::EventStream,
    animation_tick: &mut tokio::time::Interval,
) -> anyhow::Result<(Vec<crossterm::event::Event>, bool)> {
    use futures_util::{FutureExt as _, StreamExt as _};

    let mut tick_fired = false;
    let first = tokio::select! {
        event = event_stream.next() => event.map(|result| result.map_err(anyhow::Error::from)),
        _ = animation_tick.tick() => {
            tick_fired = true;
            None
        },
    };
    let mut events = Vec::new();
    if let Some(first_event) = first {
        events.push(first_event?);
        while events.len() < MAX_EVENTS_PER_TICK {
            let Some(Some(event)) = event_stream.next().now_or_never() else {
                break;
            };
            events.push(event?);
        }
    }
    Ok((events, tick_fired))
}

pub async fn run_console<H: InstanceActionHandler<jackin_core::Agent>>(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    options: ConsoleRunOptions<'_>,
    action_handler: &mut H,
    runner: &mut impl jackin_docker::CommandRunner,
) -> anyhow::Result<(Option<ConsoleOutcome>, AppConfig)> {
    use std::time::Duration;

    use crossterm::event::{Event, KeyEventKind};

    let startup_error_pending = options.startup_error.is_some();
    let mut state = jackin_console::tui::console::new_console_state_with_startup_error(
        &config,
        cwd,
        options.op_available,
        options.startup_error,
    )?;
    // When the launch flow in `app` already owns the host screen, draw into it
    // and leave teardown to that guard; otherwise own the screen here for the
    // lifetime of the console (standalone `jackin console` with no launch).
    let owned_screen = if options
        .parent_session
        .is_some_and(TerminalSession::is_active)
    {
        None
    } else {
        Some(TerminalSession::enter(host_console_terminal())?)
    };
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut mouse_state = ConsoleMouseState::new();
    // Async event source: yields to the Tokio reactor between events so
    // background tasks can progress instead of blocking for up to TICK_MS.
    let mut event_stream = crossterm::event::EventStream::new();
    // Animation tick: redraws the TUI when no events arrive so spinners,
    // the op-picker panel rain, and other animations stay live.
    let mut animation_tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    animation_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_redraw = true;

    // The Debug-info dialog paints OSC 8 hyperlinks as a raw overlay outside the
    // Ratatui buffer; when it closes we must force a full clear so that residue
    // does not linger on the screen behind it.
    let mut container_info_overlay_active = false;

    let mut screen_tracker = jackin_telemetry::ui::ScreenVisitTracker::new();
    let mut widget_tracker = jackin_telemetry::ui::WidgetFocusTracker::default();
    let mut jank_monitor = jackin_telemetry::ui::JankMonitor::default();

    let result: anyhow::Result<Option<ConsoleOutcome>> = 'main: loop {
        let action_parent = jackin_telemetry::ui::take_action_parent();
        sync_active_screen(&state, &mut screen_tracker, action_parent.as_ref());
        sync_widget_focus(&state, &mut widget_tracker, action_parent.as_ref());

        // Drain a pending token-generate request before render: suspend the
        // TUI, let the non-TUI effect executor run the interactive mint/write,
        // then resume. Done at the top of the loop (no live `&mut state.stage`
        // borrow, `config`/`paths`/`terminal` all in scope) so a request set by
        // the previous iteration's input is handled before the next frame.
        let pending = if let ConsoleStage::Manager(manager) = &mut state.stage {
            manager.take_pending_token_generate()
        } else {
            None
        };
        if let Some(req) = pending {
            let mint = if let Some(session) = owned_screen.as_ref().or(options.parent_session) {
                session.suspend(|| {
                    println!(
                        "{}",
                        token_generate_status_message(token_generate_scope_label_for_console(&req))
                    );
                    crate::console::effects::execute_token_generate(paths, &config, &req)
                })?
            } else {
                println!(
                    "{}",
                    token_generate_status_message(token_generate_scope_label_for_console(&req))
                );
                crate::console::effects::execute_token_generate(paths, &config, &req)
            };
            // Force a full repaint next frame so leftover child output is
            // overwritten. terminal.resize() resets Ratatui's internal diff
            // buffer (marks every cell dirty) without emitting \x1b[2J — this
            // avoids the blank-screen flash that terminal.clear() causes while
            // still guaranteeing that every cell is redrawn next tick.
            if let Ok(size) = terminal.size() {
                let rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                drop(terminal.resize(rect));
            }
            needs_redraw = true;
            if let ConsoleStage::Manager(ms) = &mut state.stage {
                crate::console::effects::apply_token_generate_result(ms, mint);
            }
            continue;
        }

        // Drain worker results before render so a fresh result lands
        // this frame instead of a stale Loading one.
        drain_background_messages(&mut state, &mut config, paths, cwd, &mut needs_redraw);

        if needs_redraw {
            draw_console_frame(
                &mut terminal,
                &mut state,
                DrawConsoleContext {
                    config: &config,
                    cwd,
                    mouse_state: &mut mouse_state,
                    container_info_overlay_active: &mut container_info_overlay_active,
                    action_parent: action_parent.as_ref(),
                    jank_monitor: &mut jank_monitor,
                },
            )?;
            needs_redraw = false;
        }
        drop(action_parent);
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Async event wait: yield to the Tokio reactor until either a
        // terminal event arrives or the animation tick fires. This frees
        // the reactor between events so background tasks can progress
        // instead of blocking for up to TICK_MS.
        let (event_batch, tick_fired) =
            next_event_batch(&mut event_stream, &mut animation_tick).await?;
        if tick_fired && let ConsoleStage::Manager(ms) = &mut state.stage {
            needs_redraw |= ms.tick_active_animation();
        }
        needs_redraw |= !event_batch.is_empty();
        for event in event_batch {
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let mut inputs = ConsoleLoopInputs {
                        config: &mut config,
                        paths,
                        cwd,
                        action_handler,
                        runner,
                        startup_error_pending,
                    };
                    match handle_key_event(
                        &mut terminal,
                        &mut state,
                        key,
                        &mut inputs,
                        &mouse_state,
                        &mut needs_redraw,
                    )
                    .await?
                    {
                        ConsoleLoopFlow::Continue => {}
                        ConsoleLoopFlow::Exit(outcome) => break 'main Ok(outcome),
                    }
                }
                Event::Mouse(mouse) => {
                    let mut inputs = ConsoleLoopInputs {
                        config: &mut config,
                        paths,
                        cwd,
                        action_handler,
                        runner,
                        startup_error_pending,
                    };
                    match handle_mouse_event(
                        &mut state,
                        mouse,
                        term_size,
                        &mut inputs,
                        &mut mouse_state,
                        &mut needs_redraw,
                    )? {
                        ConsoleLoopFlow::Continue => {}
                        ConsoleLoopFlow::Exit(outcome) => break 'main Ok(outcome),
                    }
                }
                _ => {}
            }
        }
    };

    let _screen_result =
        screen_tracker.exit(jackin_telemetry::schema::enums::TransitionReason::Shutdown);
    let _focus_result = widget_tracker.unfocus();
    // Tears down only when the console owns the screen standalone. When the
    // launch flow owns it, this is `None` and teardown waits for that guard so
    // the console → loading transition stays on one alternate screen.
    drop(owned_screen);
    // Return the in-memory config so the post-console path can skip a disk
    // reload when nothing was written (and still sees in-session mutations
    // that already updated `config` on successful save).
    Ok((result?, config))
}
