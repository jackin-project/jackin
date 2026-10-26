// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::console::terminal::{
    MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS, TerminalSession, host_console_terminal,
};
use crate::console::{ConsoleOutcome, ConsoleStage, ConsoleState, InstanceActionHandler};
use jackin_console::tui::components::error_popup::{
    instance_action_failed_error_message, instance_action_failed_error_title,
    launch_failed_error_message, launch_failed_error_title,
};
use jackin_console::tui::components::status_popup::{
    instance_action_busy_message, instance_action_busy_title,
};
use jackin_console::tui::message::PromptOutcome;
use jackin_console::tui::message::launch_prompt_should_probe_agents;
use jackin_console::tui::model::{
    ConsolePaintedInputOwner, clear_pending_launch_role_plan, take_pending_launch_plan,
};
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
    startup_error_modal_active_for_console,
};

use jackin_config::AppConfig;
use jackin_config::LoadWorkspaceInput;
use jackin_core::JackinPaths;

pub struct ConsoleRunOptions<'a> {
    pub op_available: bool,
    pub startup_error: Option<(String, String)>,
    pub parent_session: Option<&'a TerminalSession>,
}

pub(crate) fn load_console_usage_state(
    paths: &JackinPaths,
    force_refresh: bool,
) -> anyhow::Result<jackin_console::tui::state::UsageScreenState> {
    load_console_usage_state_inner(paths, force_refresh, false)
}

fn load_console_startup_usage_state(
    paths: &JackinPaths,
) -> anyhow::Result<jackin_console::tui::state::UsageScreenState> {
    load_console_usage_state_inner(paths, false, true)
}

fn load_console_usage_state_inner(
    paths: &JackinPaths,
    force_refresh: bool,
    request_due_refresh: bool,
) -> anyhow::Result<jackin_console::tui::state::UsageScreenState> {
    use jackin_usage::host::{UsageBrokerConfig, UsageDiscoveryScope, ensure_usage_broker_process};

    let discovery_scope = UsageDiscoveryScope::HostDesktop {
        config_root: paths.config_dir.clone(),
        operator_home: paths.home_dir.clone(),
    };
    let broker_config = UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    let client = if force_refresh || request_due_refresh {
        ensure_usage_broker_process(broker_config, &discovery_scope)
            .map_err(|error| anyhow::anyhow!(error.message))?
    } else {
        broker_config.client()
    };
    load_console_usage_projection(&client, force_refresh, request_due_refresh)
}

fn load_console_usage_projection(
    client: &jackin_usage::host::UsageBrokerClient,
    force_refresh: bool,
    request_due_refresh: bool,
) -> anyhow::Result<jackin_console::tui::state::UsageScreenState> {
    let current = client
        .current_projection()
        .map_err(|error| anyhow::anyhow!(error.message))?;
    let projection = if force_refresh || request_due_refresh {
        client
            .request_refresh(Some(current.projection_id), force_refresh)
            .map_err(|error| anyhow::anyhow!(error.message))?
    } else {
        current
    };
    Ok(jackin_console::tui::screens::usage::UsageScreenState::from_projection(&projection))
}

pub(crate) fn execute_usage_refresh_effect(
    manager: &mut crate::console::adapter::state::ManagerState<'_>,
    paths: &JackinPaths,
) -> bool {
    use std::time::Instant;

    let Some(screen) = manager.usage.screen.as_mut() else {
        return false;
    };

    let now = Instant::now();
    let mut changed = false;
    if let Some(outcome) = screen.poll_refresh() {
        match outcome {
            Ok(snapshot) => {
                manager.usage_snapshot = snapshot.clone();
                screen.apply_refresh(snapshot, now);
            }
            Err(message) => {
                let notice = format!("Usage unavailable: {message}");
                manager.usage_snapshot.notice = Some(notice.clone());
                screen.apply_refresh_error(notice, now);
            }
        }
        changed = true;
    }

    if let Some(plan) = screen.next_refresh_plan_if_due(now) {
        let paths = paths.clone();
        screen.begin_refresh(jackin_console::tui::runtime::spawn_blocking_subscription(
            move || {
                let outcome =
                    load_console_usage_state(&paths, plan.force).map_err(|error| error.to_string());
                (plan.generation, outcome)
            },
        ));
    }
    changed
}

fn poll_startup_usage(
    state: &mut ConsoleState,
    rx: &mut Option<
        jackin_console::tui::runtime::BlockingSubscription<
            jackin_console::tui::screens::usage::UsageRefreshOutcome,
        >,
    >,
) -> bool {
    use jackin_console::tui::runtime::SubscriptionPoll;

    let Some(inner) = rx.as_mut() else {
        return false;
    };
    let outcome = match inner.poll_next() {
        SubscriptionPoll::Ready(outcome) => outcome,
        SubscriptionPoll::Closed => Err("usage refresh worker disconnected".to_owned()),
        SubscriptionPoll::Pending => return false,
    };
    *rx = None;
    if let ConsoleStage::Manager(manager) = &mut state.stage {
        if manager.usage_snapshot.canonical_projection.is_some()
            || manager
                .usage
                .screen
                .as_ref()
                .is_some_and(|screen| screen.last_refresh_at.is_some())
        {
            return true;
        }
        match outcome {
            Ok(snapshot) => {
                manager.usage_snapshot = snapshot;
            }
            Err(error) => {
                manager.usage_snapshot.notice = Some(format!("Usage unavailable: {error}"));
            }
        }
    }
    true
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

pub(crate) const fn screen_of(state: &ConsoleState) -> jackin_telemetry::schema::enums::ScreenId {
    use crate::console::adapter::state::ManagerStage;
    use jackin_telemetry::schema::enums::ScreenId;

    let ConsoleStage::Manager(ms) = &state.stage;
    match ms.stage {
        ManagerStage::List
        | ManagerStage::Workflows(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ScreenId::WorkspaceList,
        ManagerStage::Editor(_) => ScreenId::WorkspaceEditor,
        ManagerStage::Settings(_) => ScreenId::Settings,
        ManagerStage::CreatePrelude(_) => ScreenId::WorkspaceCreate,
    }
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

#[expect(
    clippy::large_enum_variant,
    reason = "Exit carries the resolved launch outcome (workspace + role + agent) by value exactly once per committed launch; boxing saves nothing measurable"
)]
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
    last_pointer_position: Option<ratatui::layout::Position>,
    pointer_shape: termrock::osc::PointerShape,
    chrome_regions: Vec<termrock::interaction::HitRegion<ConsoleChromeHover>>,
    chrome_hover: Option<ConsoleChromeHover>,
    last_painted_owner: Option<(bool, ConsolePaintedInputOwner)>,
    last_painted_main_area: Option<ratatui::layout::Rect>,
    geometry_invalid: bool,
}

impl ConsoleMouseState {
    fn new() -> Self {
        Self {
            last_event_at: None,
            last_pointer_position: None,
            pointer_shape: termrock::osc::PointerShape::Default,
            chrome_regions: Vec::new(),
            chrome_hover: None,
            last_painted_owner: None,
            last_painted_main_area: None,
            geometry_invalid: false,
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
        | InputOutcome::LaunchWithAccount { .. } => Some(UiActionName::WorkspaceLaunch),
        InputOutcome::PrewarmNamed(_) | InputOutcome::NewSessionWithAccount { .. } => {
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
    *needs_redraw |= crate::console::effects::execute_manager_effect(
        ms,
        config,
        paths,
        jackin_console::tui::effect::ConsoleEffect::RequestUsageRefresh.into(),
    );
}

struct DrawConsoleContext<'a> {
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
    mouse_state: &'a mut ConsoleMouseState,
    container_info_overlay_active: &'a mut bool,
    action_parent: Option<&'a jackin_telemetry::ui::ActionParent>,
    jank_monitor: &'a mut jackin_telemetry::ui::JankMonitor,
}

fn paint_console_debug_chrome(
    frame: &mut ratatui::Frame<'_>,
    debug_bar_area: Option<ratatui::layout::Rect>,
    mouse_state: &mut ConsoleMouseState,
) {
    mouse_state.chrome_regions.clear();
    if let Some(bar_area) = debug_bar_area {
        let invocation_id = jackin_telemetry::identity::current_invocation()
            .map(|invocation_id| invocation_id.to_string());
        let invocation_id = debug_invocation_id_label(invocation_id.as_deref());
        let chip_row = debug_chip_row(bar_area);
        let content = format!(" {invocation_id} ");
        let slots = [
            termrock::widgets::StatusSlot::new(ConsoleChromeHover::DebugChip, &content)
                .priority(1)
                .min_width(0)
                .enabled(true)
                .style(
                    ratatui::style::Style::default()
                        .bg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Danger)
                            .fg
                            .unwrap_or_default())
                        .fg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )
                .hover_style(
                    ratatui::style::Style::default()
                        .bg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default())
                        .fg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Danger)
                            .fg
                            .unwrap_or_default())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
        ];
        let mut status_state = termrock::widgets::StatusBarState::new();
        status_state.hovered = (mouse_state.chrome_hover == Some(ConsoleChromeHover::DebugChip))
            .then_some(ConsoleChromeHover::DebugChip);
        let theme = termrock::style::DesignSystem::default().with_role(
            termrock::style::Role::StatusBar,
            ratatui::style::Style::default()
                .bg(termrock::style::DesignSystem::default()
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
            mouse_state.chrome_regions.push(region);
        }
    }
}

fn draw_console_frame<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    context: DrawConsoleContext<'_>,
) -> anyhow::Result<bool>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.usage.clear_painted_pointer();
    let full_area: ratatui::layout::Rect = terminal.size()?.into();
    let (main_area, debug_bar_area) =
        split_debug_area(full_area, jackin_diagnostics::is_debug_mode());
    let should_refresh_pointer_after_paint;
    {
        let mut ignored_redraw = false;
        let quit_confirm_open = state.quit_confirm.is_some();
        let base_surface_unblocked = state.base_surface_unblocked();
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let input_owner = ms.painted_input_owner();
        let painted_owner = (quit_confirm_open, input_owner);
        should_refresh_pointer_after_paint = context.mouse_state.last_painted_owner
            != Some(painted_owner)
            || context.mouse_state.last_painted_main_area != Some(main_area)
            || context.mouse_state.geometry_invalid;
        if context.mouse_state.last_painted_owner != Some(painted_owner) {
            ms.clear_pointer_state();
            reset_modal_mouse_state(context.mouse_state, &mut ignored_redraw);
        }
        context.mouse_state.last_painted_owner = Some(painted_owner);
        let manager_pointer_actions_enabled = input_owner.allows_manager_pointer_actions();
        if quit_confirm_open {
            ms.clear_pointer_state();
            reset_modal_mouse_state(context.mouse_state, &mut ignored_redraw);
        } else if !manager_pointer_actions_enabled {
            ms.clear_pointer_state();
            if !base_surface_unblocked {
                reset_modal_mouse_state(context.mouse_state, &mut ignored_redraw);
            }
        } else if !base_surface_unblocked {
            context.mouse_state.chrome_hover = None;
        }
        let container_info_painted = !quit_confirm_open
            && matches!(
                input_owner,
                ConsolePaintedInputOwner::Dispatch(
                    jackin_console::tui::model::ConsoleInputDispatchPlan::ListModal
                )
            )
            && matches!(
                ms.list_modal,
                Some(crate::console::adapter::state::Modal::ContainerInfo { .. })
            );
        if *context.container_info_overlay_active && !container_info_painted {
            terminal.clear()?;
            *context.container_info_overlay_active = false;
        }
        crate::console::adapter::prepare_for_render(ms, context.config, context.cwd, main_area);
    }

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
    let mut usage_receipt = None;
    let render_result = terminal.draw(|frame| {
        let ConsoleStage::Manager(ms) = &state.stage;
        usage_receipt = jackin_console::tui::view::render_with_usage_receipt(
            frame,
            main_area,
            ms,
            context.config,
            context.cwd,
        );
        if let Some(confirm) = confirm_state {
            let (body, hint_row) = quit_confirm_body_and_hint_area(main_area);
            jackin_console::tui::view::render_modal_backdrop(frame, body);
            let area = quit_confirm_area(body, confirm);
            jackin_console::tui::components::render_confirm_dialog(frame, area, confirm);
            termrock::widgets::render_hint_bar(
                frame,
                hint_row,
                &jackin_console::tui::components::confirm_hint_spans(),
                &termrock::style::DesignSystem::default(),
            );
        }
        paint_console_debug_chrome(frame, debug_bar_area, context.mouse_state);
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
    let root_surface_unblocked = state.quit_confirm.is_none();
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        if root_surface_unblocked && ms.painted_input_owner() == ConsolePaintedInputOwner::Usage {
            ms.usage.publish_painted_pointer(main_area, usage_receipt);
        } else {
            ms.usage.clear_painted_pointer();
        }
    }
    context.mouse_state.last_painted_main_area = Some(main_area);
    context
        .jank_monitor
        .record_frame(screen, render_started.elapsed().as_secs_f64());

    let ConsoleStage::Manager(ms) = &state.stage;
    let container_info_painted = state.quit_confirm.is_none()
        && matches!(
            ms.painted_input_owner(),
            ConsolePaintedInputOwner::Dispatch(
                jackin_console::tui::model::ConsoleInputDispatchPlan::ListModal
            )
        );
    if container_info_painted
        && let Some(modal @ crate::console::adapter::state::Modal::ContainerInfo { state: info }) =
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

    let hover_frame_owed = refresh_pointer_after_paint(
        state,
        main_area,
        context.config,
        context.mouse_state,
        should_refresh_pointer_after_paint,
    );
    context.mouse_state.geometry_invalid = false;
    Ok(hover_frame_owed)
}

fn quit_confirm_body_and_hint_area(
    main_area: ratatui::layout::Rect,
) -> (ratatui::layout::Rect, ratatui::layout::Rect) {
    let body = ratatui::layout::Rect {
        height: main_area.height.saturating_sub(1),
        ..main_area
    };
    let hint_row = if main_area.height == 0 {
        ratatui::layout::Rect::new(main_area.x, main_area.y, main_area.width, 0)
    } else {
        ratatui::layout::Rect::new(
            main_area.x,
            main_area.bottom().saturating_sub(1),
            main_area.width,
            1,
        )
    };
    (body, hint_row)
}

fn refresh_pointer_after_paint(
    state: &mut ConsoleState,
    main_area: ratatui::layout::Rect,
    config: &AppConfig,
    mouse_state: &mut ConsoleMouseState,
    should_refresh_manager_hover: bool,
) -> bool {
    if state.quit_confirm.is_some() {
        let mut redraw_owed = false;
        reset_modal_mouse_state(mouse_state, &mut redraw_owed);
        return redraw_owed;
    }
    let Some(position) = mouse_state.last_pointer_position else {
        return false;
    };
    let mouse = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Moved,
        column: position.x,
        row: position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let base_surface_unblocked = state.base_surface_unblocked();
    let mut redraw_owed = false;
    let ConsoleStage::Manager(manager) = &mut state.stage;
    if should_refresh_manager_hover
        && manager
            .painted_input_owner()
            .allows_manager_pointer_actions()
    {
        drop(crate::console::adapter::input::handle_mouse_with_config(
            manager,
            mouse,
            main_area,
            Some(config),
        ));
        redraw_owed |= manager_has_pointer_hover(manager);
    }
    update_console_pointer_shape(
        manager,
        config,
        mouse,
        main_area,
        mouse_state,
        &mut redraw_owed,
        base_surface_unblocked,
    );
    redraw_owed
}

fn manager_has_pointer_hover(manager: &jackin_console::tui::state::ManagerState<'_>) -> bool {
    let stage_has_hover = match &manager.stage {
        jackin_console::tui::state::ManagerStage::Editor(editor) => editor.hover_target.is_some(),
        jackin_console::tui::state::ManagerStage::Settings(settings) => {
            settings.hover_target.is_some()
        }
        _ => manager.hover_target.is_some(),
    };
    stage_has_hover
        || matches!(
            &manager.list_modal,
            Some(crate::console::adapter::state::Modal::ContainerInfo { state: info })
                if info.hovered_row().is_some()
        )
}

fn present_owed_frame<B>(
    pacer: &mut jackin_console::tui::runtime::ConsoleFramePacer,
    needs_redraw: &mut bool,
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    context: DrawConsoleContext<'_>,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    if *needs_redraw {
        pacer.mark_dirty();
    }
    let tick = pacer.tick();
    if !pacer.should_draw(tick.now()) {
        return Ok(());
    }
    pacer.begin_draw(tick.now());
    let hover_frame_owed = draw_console_frame(terminal, state, context)?;
    pacer.end_draw(tick.now());
    *needs_redraw = hover_frame_owed;
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
        crate::console::adapter::update_manager(
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
    crate::console::adapter::update_manager(
        ms,
        crate::console::adapter::ManagerMessage::DismissStatusPopup,
    );
    if let Err(error) = result {
        let err_title = instance_action_failed_error_title(action.workspace_action_fact());
        crate::console::adapter::update_manager(
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

fn show_launch_dispatch_error(state: &mut ConsoleState, error: &anyhow::Error) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    crate::console::adapter::update_manager(
        ms,
        crate::console::adapter::ManagerMessage::OpenListErrorPopup {
            title: launch_failed_error_title().into(),
            message: launch_failed_error_message(error),
        },
    );
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
            match dispatch_launch_input(terminal, state, inputs, LoadWorkspaceInput::Saved(name))
                .await
            {
                Ok(Some(outcome)) => return Ok(ConsoleLoopFlow::Exit(Some(outcome))),
                Ok(None) => {}
                Err(error) => {
                    show_launch_dispatch_error(state, &error);
                    *needs_redraw = true;
                }
            }
        }
        crate::console::adapter::InputOutcome::PrewarmNamed(name) => {
            return Ok(ConsoleLoopFlow::Exit(Some(ConsoleOutcome::PrewarmNamed(
                name,
            ))));
        }
        crate::console::adapter::InputOutcome::LaunchCurrentDir => {
            match dispatch_launch_input(terminal, state, inputs, LoadWorkspaceInput::CurrentDir)
                .await
            {
                Ok(Some(outcome)) => return Ok(ConsoleLoopFlow::Exit(Some(outcome))),
                Ok(None) => {}
                Err(error) => {
                    show_launch_dispatch_error(state, &error);
                    *needs_redraw = true;
                }
            }
        }
        crate::console::adapter::InputOutcome::LaunchWithAgent(role) => {
            match dispatch_committed_role(terminal, state, inputs, role).await {
                Ok(Some(outcome)) => return Ok(ConsoleLoopFlow::Exit(Some(outcome))),
                Ok(None) => {}
                Err(error) => {
                    show_launch_dispatch_error(state, &error);
                    *needs_redraw = true;
                }
            }
        }
        crate::console::adapter::InputOutcome::LaunchWithRuntimeAgent(agent) => {
            match launch_with_committed_agent(state, inputs.config, inputs.cwd, agent) {
                Ok(Some(outcome)) => return Ok(ConsoleLoopFlow::Exit(Some(outcome))),
                Ok(None) => {}
                Err(error) => {
                    show_launch_dispatch_error(state, &error);
                    *needs_redraw = true;
                }
            }
        }
        crate::console::adapter::InputOutcome::NewSessionWithAccount {
            container,
            agent,
            instance_id,
        } => {
            return Ok(ConsoleLoopFlow::Exit(Some(
                ConsoleOutcome::NewSessionWithAccount {
                    container,
                    agent,
                    instance_id,
                },
            )));
        }
        crate::console::adapter::InputOutcome::LaunchWithAccount {
            selector,
            agent,
            selection,
        } => {
            let Some(input) = take_pending_launch_plan(state) else {
                return Ok(ConsoleLoopFlow::Exit(None));
            };
            let workspace = match jackin_console::services::launch::resolve_account_launch_workspace(
                inputs.config,
                inputs.cwd,
                &input,
                &selector,
            ) {
                Ok(workspace) => workspace,
                Err(error) => {
                    show_launch_dispatch_error(state, &error);
                    *needs_redraw = true;
                    return Ok(ConsoleLoopFlow::Continue);
                }
            };
            let Some(workspace) = workspace else {
                return Ok(ConsoleLoopFlow::Exit(None));
            };
            return Ok(ConsoleLoopFlow::Exit(Some(
                ConsoleOutcome::LaunchWithAccount {
                    selector,
                    workspace,
                    agent,
                    selection,
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
    mouse_state: &mut ConsoleMouseState,
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

    if state.base_surface_unblocked()
        && matches!(key.code, KeyCode::Esc)
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
            QuitConfirmPlan::Dismiss => {
                let ConsoleStage::Manager(manager) = &mut state.stage;
                manager.clear_pointer_state();
                reset_modal_mouse_state(mouse_state, needs_redraw);
            }
            QuitConfirmPlan::Continue => {}
        }
        return Ok(ConsoleLoopFlow::Continue);
    }

    if should_intercept_root_quit(state, key) {
        state.open_quit_confirm();
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.clear_pointer_state();
        reset_modal_mouse_state(mouse_state, needs_redraw);
        return Ok(ConsoleLoopFlow::Continue);
    }

    let owner_before = {
        let ConsoleStage::Manager(manager) = &state.stage;
        manager.painted_input_owner()
    };
    let outcome = {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        crate::console::adapter::handle_key(ms, inputs.config, inputs.paths, inputs.cwd, key)?
    };
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        *needs_redraw |= execute_usage_refresh_effect(ms, inputs.paths);
    }
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
    let ConsoleStage::Manager(manager) = &mut state.stage;
    if owner_before != manager.painted_input_owner() {
        manager.clear_pointer_state();
        reset_modal_mouse_state(mouse_state, needs_redraw);
    }
    handle_input_outcome(terminal, state, outcome, inputs, needs_redraw).await
}

fn should_intercept_root_quit(state: &ConsoleState, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    let ConsoleStage::Manager(manager) = &state.stage;
    let workflows_owns_bare_q = matches!(
        &manager.stage,
        jackin_console::tui::state::ManagerStage::Workflows(_)
    ) && matches!(key.code, KeyCode::Char('q'))
        && (key.modifiers - KeyModifiers::SHIFT).is_empty();

    state.base_surface_unblocked()
        && !workflows_owns_bare_q
        && should_open_quit_confirm(key, quit_intercept_state_for_console(state))
}

fn reset_modal_mouse_state(mouse_state: &mut ConsoleMouseState, needs_redraw: &mut bool) {
    let hover_changed = mouse_state.chrome_hover.is_some();
    if hover_changed {
        mouse_state.chrome_hover = None;
    }
    if mouse_state.pointer_shape != termrock::osc::PointerShape::Default {
        mouse_state.pointer_shape = termrock::osc::PointerShape::Default;
        let mut out = std::io::stdout();
        let seq = termrock::osc::encode_pointer(mouse_state.pointer_shape);
        let _unused = std::io::Write::write_all(&mut out, &seq);
        drop(std::io::Write::flush(&mut out));
    }
    *needs_redraw |= hover_changed;
}

fn update_console_pointer_shape(
    ms: &jackin_console::tui::state::ManagerState<'_>,
    config: &AppConfig,
    mouse: crossterm::event::MouseEvent,
    term_size: ratatui::layout::Rect,
    mouse_state: &mut ConsoleMouseState,
    needs_redraw: &mut bool,
    base_surface_unblocked: bool,
) {
    let next_chrome_hover = console_chrome_hover_at(mouse_state, mouse, base_surface_unblocked);
    if next_chrome_hover != mouse_state.chrome_hover {
        mouse_state.chrome_hover = next_chrome_hover;
        *needs_redraw = true;
    }
    let next_pointer_shape = console_pointer_shape_at(
        ms,
        config,
        mouse,
        term_size,
        mouse_state.chrome_hover.is_some(),
    );
    if next_pointer_shape != mouse_state.pointer_shape {
        mouse_state.pointer_shape = next_pointer_shape;
        let seq = termrock::osc::encode_pointer(mouse_state.pointer_shape);
        let mut out = std::io::stdout();
        drop(std::io::Write::write_all(&mut out, &seq));
        drop(std::io::Write::flush(&mut out));
    }
}

fn console_chrome_hover_at(
    mouse_state: &ConsoleMouseState,
    mouse: crossterm::event::MouseEvent,
    base_surface_unblocked: bool,
) -> Option<ConsoleChromeHover> {
    if !base_surface_unblocked {
        return None;
    }
    mouse_state
        .chrome_regions
        .iter()
        .find(|region| {
            region
                .area
                .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
        })
        .map(|region| region.id)
}

fn console_pointer_shape_at(
    ms: &jackin_console::tui::state::ManagerState<'_>,
    config: &AppConfig,
    mouse: crossterm::event::MouseEvent,
    term_size: ratatui::layout::Rect,
    chrome_hovered: bool,
) -> termrock::osc::PointerShape {
    console_pointer_shape(
        chrome_hovered,
        crate::console::adapter::input::clickable_at(ms, mouse, term_size, Some(config)),
    )
}

fn handle_terminal_resize<B>(
    terminal: &mut ratatui::Terminal<B>,
    width: u16,
    height: u16,
    state: &mut ConsoleState,
    mouse_state: &mut ConsoleMouseState,
    needs_redraw: &mut bool,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.usage.clear_painted_pointer();
    terminal.resize(ratatui::layout::Rect::new(0, 0, width, height))?;
    manager.clear_pointer_state();
    mouse_state.chrome_regions.clear();
    mouse_state.geometry_invalid = true;
    reset_modal_mouse_state(mouse_state, needs_redraw);
    *needs_redraw = true;
    Ok(())
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
    let position = ratatui::layout::Position::new(mouse.column, mouse.row);
    mouse_state.last_pointer_position = Some(position);
    if matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
    ) {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        if manager.drag_state.is_some() {
            crate::console::adapter::update_manager(
                manager,
                crate::console::adapter::ManagerMessage::SetDragState(None),
            );
            *needs_redraw = true;
        }
        return Ok(ConsoleLoopFlow::Continue);
    }
    let base_surface_unblocked = state.base_surface_unblocked();
    let (main_area, _) = split_debug_area(term_size, jackin_diagnostics::is_debug_mode());
    let (modal_plan, list_modal_painted) = {
        let quit_confirm_rect = state.quit_confirm_state().map(|confirm| {
            let (body, _) = quit_confirm_body_and_hint_area(main_area);
            quit_confirm_area(body, confirm)
        });
        let ConsoleStage::Manager(ms) = &state.stage;
        let list_modal_painted = state.quit_confirm.is_none()
            && matches!(
                ms.painted_input_owner(),
                ConsolePaintedInputOwner::Dispatch(
                    jackin_console::tui::model::ConsoleInputDispatchPlan::ListModal
                )
            );
        let list_modal_rect = list_modal_painted
            .then(|| ms.list_modal.as_ref().map(|modal| modal.rect(main_area)))
            .flatten();
        (
            modal_mouse_layer_plan(
                mouse,
                ConsoleModalMouseLayerFacts {
                    quit_confirm_rect,
                    list_modal_rect,
                    list_modal_container_info: list_modal_painted
                        && matches!(
                            ms.list_modal,
                            Some(crate::console::adapter::state::Modal::ContainerInfo { .. })
                        ),
                    startup_error_modal_active: list_modal_painted
                        && startup_error_modal_active_for_console(
                            state,
                            inputs.startup_error_pending,
                        ),
                },
            ),
            list_modal_painted,
        )
    };
    if modal_plan.dismiss_quit_confirm {
        state.dismiss_quit_confirm();
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.clear_pointer_state();
    }
    if modal_plan.dismiss_list_modal {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        crate::console::adapter::update_manager(
            ms,
            crate::console::adapter::ManagerMessage::DismissListModal,
        );
        ms.clear_pointer_state();
    }

    let dispatch_painted_list_modal = list_modal_painted && !modal_plan.dismiss_list_modal;
    if modal_plan.consumed && !dispatch_painted_list_modal {
        if startup_error_dismissed(state, inputs.startup_error_pending) {
            return Ok(ConsoleLoopFlow::Exit(None));
        }
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.clear_pointer_state();
        reset_modal_mouse_state(mouse_state, needs_redraw);
        return Ok(ConsoleLoopFlow::Continue);
    }

    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        let debug_chip_hovered = mouse_state.chrome_hover == Some(ConsoleChromeHover::DebugChip);
        let active_run = jackin_diagnostics::active_run();
        if debug_chip_activation_allowed(
            mouse,
            base_surface_unblocked,
            debug_chip_hovered,
            active_run.is_some(),
        ) && let Some(run) = active_run
        {
            crate::console::adapter::update_manager(
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
            main_area,
            Some(inputs.config),
        );
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
    let pointer_chrome_allowed = state.base_surface_unblocked();
    let ConsoleStage::Manager(ms) = &state.stage;
    update_console_pointer_shape(
        ms,
        inputs.config,
        mouse,
        main_area,
        mouse_state,
        needs_redraw,
        pointer_chrome_allowed,
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

fn pending_event_allowed_during_invalid_geometry(event: &crossterm::event::Event) -> bool {
    matches!(event, crossterm::event::Event::Resize(_, _))
        || matches!(
            event,
            crossterm::event::Event::Mouse(mouse)
                if matches!(
                    mouse.kind,
                    crossterm::event::MouseEventKind::Up(
                        crossterm::event::MouseButton::Left
                    )
                )
        )
}

fn pop_pending_event(
    pending_events: &mut std::collections::VecDeque<crossterm::event::Event>,
    geometry_invalid: bool,
) -> Option<crossterm::event::Event> {
    let allowed = !geometry_invalid
        || pending_events
            .front()
            .is_some_and(pending_event_allowed_during_invalid_geometry);
    allowed.then(|| pending_events.pop_front()).flatten()
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
    let paths_for_usage_startup = paths.clone();
    let mut startup_usage_rx = Some(jackin_console::tui::runtime::spawn_blocking_subscription(
        move || {
            load_console_startup_usage_state(&paths_for_usage_startup)
                .map_err(|error| error.to_string())
        },
    ));
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
    let mut event_stream = crossterm::event::EventStream::new();
    let mut animation_tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    animation_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pacer = jackin_console::tui::runtime::ConsoleFramePacer::new();
    let mut pending_events = std::collections::VecDeque::new();
    let mut needs_redraw = true;

    let mut container_info_overlay_active = false;

    let mut screen_tracker = jackin_telemetry::ui::ScreenVisitTracker::new();
    let mut widget_tracker = jackin_telemetry::ui::WidgetFocusTracker::default();
    let mut jank_monitor = jackin_telemetry::ui::JankMonitor::default();

    let result: anyhow::Result<Option<ConsoleOutcome>> = 'main: loop {
        let action_parent = jackin_telemetry::ui::take_action_parent();
        needs_redraw |= action_parent.is_some();
        sync_active_screen(&state, &mut screen_tracker, action_parent.as_ref());
        sync_widget_focus(&state, &mut widget_tracker, action_parent.as_ref());

        drain_background_messages(&mut state, &mut config, paths, cwd, &mut needs_redraw);
        needs_redraw |= poll_startup_usage(&mut state, &mut startup_usage_rx);

        present_owed_frame(
            &mut pacer,
            &mut needs_redraw,
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
        drop(action_parent);
        if jackin_telemetry::ui::has_pending_actions() {
            continue;
        }
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        let pending_event_ready = pending_events.front().is_some_and(|event| {
            !mouse_state.geometry_invalid || pending_event_allowed_during_invalid_geometry(event)
        });
        let (event_batch, tick_fired) = if pending_events.is_empty() || !pending_event_ready {
            next_event_batch(&mut event_stream, &mut animation_tick).await?
        } else {
            (Vec::new(), false)
        };
        pending_events.extend(event_batch);
        if tick_fired && let ConsoleStage::Manager(ms) = &mut state.stage {
            needs_redraw |= ms.tick_active_animation();
        }
        needs_redraw |= !pending_events.is_empty();
        if let Some(event) = pop_pending_event(&mut pending_events, mouse_state.geometry_invalid) {
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
                        &mut mouse_state,
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
                Event::Resize(width, height) => {
                    handle_terminal_resize(
                        &mut terminal,
                        width,
                        height,
                        &mut state,
                        &mut mouse_state,
                        &mut needs_redraw,
                    )?;
                }
                _ => {}
            }
        }
    };

    let _screen_result =
        screen_tracker.exit(jackin_telemetry::schema::enums::TransitionReason::Shutdown);
    let _focus_result = widget_tracker.unfocus();
    drop(owned_screen);
    Ok((result?, config))
}
