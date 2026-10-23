// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

struct NoopInstanceActionHandler;

impl InstanceActionHandler<jackin_core::Agent> for NoopInstanceActionHandler {
    fn run_in_place(
        &mut self,
        _container: &str,
        _action: jackin_console::tui::message::ConsoleInstanceAction<jackin_core::Agent>,
    ) -> impl Future<Output = anyhow::Result<()>> {
        std::future::ready(Err(anyhow::anyhow!(
            "unexpected instance action in quit-routing test"
        )))
    }
}

struct NoopCommandRunner;

impl jackin_docker::CommandRunner for NoopCommandRunner {
    fn run(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&std::path::Path>,
        _opts: &jackin_docker::RunOptions,
    ) -> impl Future<Output = anyhow::Result<()>> {
        std::future::ready(Err(anyhow::anyhow!(
            "unexpected command in quit-routing test"
        )))
    }

    fn capture(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> impl Future<Output = anyhow::Result<String>> {
        std::future::ready(Err(anyhow::anyhow!(
            "unexpected command capture in quit-routing test"
        )))
    }

    fn capture_secret(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> impl Future<Output = anyhow::Result<String>> {
        std::future::ready(Err(anyhow::anyhow!(
            "unexpected secret capture in quit-routing test"
        )))
    }
}

struct AppKeyHarness {
    temp: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    terminal: ratatui::Terminal<ratatui::backend::TestBackend>,
    config: AppConfig,
    paths: JackinPaths,
    state: ConsoleState,
    mouse_state: ConsoleMouseState,
}

impl AppKeyHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = AppConfig::default();
        let state = jackin_console::tui::console::new_console_state(&config, temp.path())
            .expect("console state");
        Self {
            paths: JackinPaths::for_tests(temp.path()),
            temp,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime"),
            terminal: ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30))
                .expect("test terminal"),
            config,
            state,
            mouse_state: ConsoleMouseState::new(),
        }
    }

    fn dispatch(&mut self, key: crossterm::event::KeyEvent) -> ConsoleLoopFlow {
        let mut action_handler = NoopInstanceActionHandler;
        let mut runner = NoopCommandRunner;
        let mut inputs = ConsoleLoopInputs {
            config: &mut self.config,
            paths: &self.paths,
            cwd: self.temp.path(),
            action_handler: &mut action_handler,
            runner: &mut runner,
            startup_error_pending: false,
        };
        let mut needs_redraw = false;
        self.runtime
            .block_on(handle_key_event(
                &mut self.terminal,
                &mut self.state,
                key,
                &mut inputs,
                &mut self.mouse_state,
                &mut needs_redraw,
            ))
            .expect("app key handler")
    }
}

fn workflow_detail() -> jackin_workflow::RunDetail {
    jackin_workflow::RunDetail {
        summary: jackin_workflow::RunSummary {
            run_id: "root-quit-workflow".to_owned(),
            plan_hash: "plan".to_owned(),
            status: "active".to_owned(),
            stage: "implement".to_owned(),
            round: 0,
            candidate: None,
            candidate_generation: 0,
            version: 0,
            owner_epoch: 1,
            pause_kind: None,
            max_repair_turns: 3,
        },
        sessions: Vec::new(),
        effects: Vec::new(),
        events: Vec::new(),
        evidence: Vec::new(),
        checkpoints: Vec::new(),
    }
}

#[test]
fn root_quit_interception_requires_the_current_manager_owner_to_allow_input() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let temp = tempfile::tempdir().expect("tempdir");
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())
        .expect("console state");
    let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    let bare_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let shifted_q = KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT);

    assert!(should_intercept_root_quit(&state, ctrl_q));

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.stage = jackin_console::tui::state::ManagerStage::Editor(EditorState::new_edit(
            "workspace".to_owned(),
            jackin_config::WorkspaceConfig::default(),
        ));
    }
    assert!(should_intercept_root_quit(&state, bare_q));

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    }
    assert!(!should_intercept_root_quit(&state, bare_q));
    assert!(!should_intercept_root_quit(&state, ctrl_q));

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.keyboard_help = None;
        manager.status_overlay = Some(jackin_console::tui::components::StatusPopupState::new(
            "Working",
            "Please wait",
        ));
    }
    assert!(!should_intercept_root_quit(&state, ctrl_q));

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.status_overlay = None;
        manager.usage.visible = true;
    }
    assert!(
        should_intercept_root_quit(&state, ctrl_q),
        "stale Usage on Editor does not suppress an allowed root Ctrl-Q"
    );

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.stage = jackin_console::tui::state::ManagerStage::List;
    }
    assert!(
        !should_intercept_root_quit(&state, ctrl_q),
        "painted List Usage suppresses root Ctrl-Q"
    );

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.usage.visible = false;
        manager.stage = jackin_console::tui::state::ManagerStage::Workflows(
            jackin_console::tui::screens::workflows::model::WorkflowScreen::default(),
        );
    }
    assert!(should_intercept_root_quit(&state, ctrl_q));
    assert!(
        !should_intercept_root_quit(&state, bare_q),
        "bare q belongs to the Workflows key handler"
    );
    assert!(!should_intercept_root_quit(&state, shifted_q));

    {
        let ConsoleStage::Manager(manager) = &mut state.stage;
        let jackin_console::tui::state::ManagerStage::Workflows(screen) = &mut manager.stage else {
            unreachable!();
        };
        screen.open_confirm(jackin_console::tui::screens::workflows::model::WorkflowAction::Cancel);
    }
    assert!(!should_intercept_root_quit(&state, ctrl_q));
    assert!(!should_intercept_root_quit(&state, bare_q));
}

#[test]
fn app_key_handler_routes_bare_q_to_workflow_detail_and_list() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut harness = AppKeyHarness::new();
    let bare_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    let mut workflow = jackin_console::tui::screens::workflows::model::WorkflowScreen::default();
    workflow.set_detail(workflow_detail());
    manager.stage = jackin_console::tui::state::ManagerStage::Workflows(workflow);
    manager.usage.visible = true;
    manager.inline_new_session_picker = Some((
        "hidden-workflow-owner".to_owned(),
        jackin_console::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
        ]),
        Vec::new(),
    ));

    assert!(matches!(
        harness.dispatch(bare_q),
        ConsoleLoopFlow::Continue
    ));
    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    let jackin_console::tui::state::ManagerStage::Workflows(workflow) = &mut manager.stage else {
        panic!("bare q on workflow detail must remain on Workflows");
    };
    assert!(workflow.detail().is_none(), "bare q closes workflow detail");
    assert!(
        harness.state.quit_confirm.is_none(),
        "bare q does not open root quit"
    );
    assert!(manager.usage.visible, "stale Usage state is retained");
    assert!(manager.inline_new_session_picker.is_some());

    harness.dispatch(bare_q);
    let ConsoleStage::Manager(manager) = &harness.state.stage;
    assert_eq!(
        manager.stage.route(),
        jackin_console::tui::model::ConsoleManagerStageRoute::List,
        "bare q on the workflow list returns to List"
    );
    assert!(harness.state.quit_confirm.is_none());
}

#[test]
fn app_key_handler_keeps_workflow_confirmation_above_root_quit() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut harness = AppKeyHarness::new();
    let bare_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    let mut workflow = jackin_console::tui::screens::workflows::model::WorkflowScreen::default();
    workflow.set_detail(workflow_detail());
    workflow.open_confirm(jackin_console::tui::screens::workflows::model::WorkflowAction::Cancel);
    manager.stage = jackin_console::tui::state::ManagerStage::Workflows(workflow);
    harness.dispatch(bare_q);
    let ConsoleStage::Manager(manager) = &harness.state.stage;
    let jackin_console::tui::state::ManagerStage::Workflows(workflow) = &manager.stage else {
        panic!("q under workflow confirmation must preserve the Workflows route");
    };
    assert_eq!(
        workflow.confirm(),
        Some(jackin_console::tui::screens::workflows::model::WorkflowAction::Cancel)
    );
    assert!(harness.state.quit_confirm.is_none());

    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    manager.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    harness.dispatch(bare_q);
    let ConsoleStage::Manager(manager) = &harness.state.stage;
    assert!(manager.keyboard_help.is_none(), "q dismisses Help first");
    let jackin_console::tui::state::ManagerStage::Workflows(workflow) = &manager.stage else {
        panic!("Help dismissal must preserve Workflows");
    };
    assert_eq!(
        workflow.confirm(),
        Some(jackin_console::tui::screens::workflows::model::WorkflowAction::Cancel)
    );
    assert!(harness.state.quit_confirm.is_none());

    harness.dispatch(ctrl_q);
    assert!(
        harness.state.quit_confirm.is_none(),
        "Ctrl-Q is blocked while the Workflow confirmation owns input"
    );

    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    let jackin_console::tui::state::ManagerStage::Workflows(workflow) = &mut manager.stage else {
        unreachable!();
    };
    workflow.dismiss_confirm();
    harness.dispatch(ctrl_q);
    assert!(
        harness.state.quit_confirm.is_some(),
        "uncovered Workflows retains its explicit Ctrl-Q root-quit path"
    );
}

#[test]
fn draw_cancels_capture_once_when_a_background_modal_becomes_owner() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let temp = tempfile::tempdir().expect("tempdir");
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())
        .expect("console state");
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).expect("test terminal");
    let mut mouse_state = ConsoleMouseState::new();
    let mut overlay_active = false;
    let mut jank = jackin_telemetry::ui::JankMonitor::default();
    let term = Rect::new(0, 0, 120, 40);

    let ConsoleStage::Manager(manager) = &mut state.stage;
    mouse_state.last_painted_owner = Some((false, manager.painted_input_owner()));
    let seam = jackin_console::tui::layout::split_seam_column(manager.list_split_pct, term.width);
    jackin_console::tui::input::mouse::handle_mouse_with_config(
        manager,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: seam,
            row: 4,
            modifiers: KeyModifiers::NONE,
        },
        term,
        Some(&config),
    );
    assert!(
        manager.drag_state.is_some(),
        "the List input captures the seam"
    );
    manager.list_modal = Some(jackin_console::tui::state::Modal::RolePicker {
        state: jackin_console::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    let original_split = manager.list_split_pct;
    let draw_context = DrawConsoleContext {
        config: &config,
        cwd: temp.path(),
        mouse_state: &mut mouse_state,
        container_info_overlay_active: &mut overlay_active,
        action_parent: None,
        jank_monitor: &mut jank,
    };
    draw_console_frame(&mut terminal, &mut state, draw_context)
        .expect("draw the newly installed modal without pointer movement");

    let ConsoleStage::Manager(manager) = &mut state.stage;
    assert!(
        manager.drag_state.is_none(),
        "new modal owner clears the old capture"
    );
    assert!(
        manager.list_modal.is_some(),
        "the visible modal remains installed"
    );
    assert_eq!(
        mouse_state.last_painted_owner,
        Some((false, manager.painted_input_owner()))
    );

    jackin_console::tui::input::mouse::handle_mouse_with_config(
        manager,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 60,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        term,
        Some(&config),
    );
    let Some(jackin_console::tui::state::Modal::RolePicker { state: picker }) =
        manager.list_modal.as_ref()
    else {
        panic!("the visible role picker must remain usable after frame ownership changes");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));

    manager.list_modal = None;
    jackin_console::tui::input::mouse::handle_mouse_with_config(
        manager,
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 80,
            row: 4,
            modifiers: KeyModifiers::NONE,
        },
        term,
        Some(&config),
    );
    assert_eq!(manager.list_split_pct, original_split);
    assert!(manager.drag_state.is_none());
}

#[test]
fn painted_inert_owner_clears_stale_chrome_hover_and_pointer_shape() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use termrock::interaction::HitRegion;

    let temp = tempfile::tempdir().expect("tempdir");
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())
        .expect("console state");
    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    let mut mouse_state = ConsoleMouseState::new();
    mouse_state.chrome_regions.push(HitRegion {
        id: ConsoleChromeHover::DebugChip,
        area: Rect::new(4, 0, 12, 1),
    });
    mouse_state.chrome_hover = Some(ConsoleChromeHover::DebugChip);
    mouse_state.pointer_shape = termrock::osc::PointerShape::Pointer;
    let mouse = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 6,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    let next_chrome_hover = console_chrome_hover_at(&mouse_state, mouse, false);
    let next_pointer_shape = console_pointer_shape_at(
        manager,
        &config,
        mouse,
        Rect::new(0, 0, 100, 30),
        next_chrome_hover.is_some(),
    );

    assert!(
        next_chrome_hover.is_none(),
        "Help blocks stale debug-chip hover"
    );
    assert_eq!(next_pointer_shape, termrock::osc::PointerShape::Default);
    assert!(!debug_chip_activation_allowed(
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 6,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        false,
        true,
        true,
    ));
}

#[test]
fn app_mouse_adapter_keeps_help_above_a_stale_list_modal_and_usage() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig::default();
    let paths = JackinPaths::for_tests(temp.path());
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())
        .expect("console state");
    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.usage.visible = true;
    manager.usage.screen = Some(jackin_console::tui::state::UsageScreenState::default());
    manager.list_modal = Some(jackin_console::tui::state::Modal::ContainerInfo {
        state: jackin_console::tui::components::container_info_surface::ContainerInfoState::new(
            "Debug info",
            vec![
                jackin_console::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID",
                    "run-hidden",
                )
                .copyable()
                .emphasised(),
            ],
        ),
    });
    let term = Rect::new(0, 0, 120, 40);
    let copy_click = (0..term.height)
        .flat_map(|row| {
            (0..term.width).map(move |column| MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            })
        })
        .find(|event| {
            jackin_console::tui::input::mouse::container_info_copyable_row_at(manager, *event, term)
        })
        .expect("painted List modal fixture has a copyable row");
    manager.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());

    let mut handler = ();
    let mut runner = ();
    let mut inputs = ConsoleLoopInputs {
        config: &mut config,
        paths: &paths,
        cwd: temp.path(),
        action_handler: &mut handler,
        runner: &mut runner,
        startup_error_pending: false,
    };
    let mut mouse_state = ConsoleMouseState::new();
    let mut needs_redraw = false;
    handle_mouse_event(
        &mut state,
        copy_click,
        term,
        &mut inputs,
        &mut mouse_state,
        &mut needs_redraw,
    )
    .expect("Help-owned adapter mouse event");

    let ConsoleStage::Manager(manager) = &mut state.stage;
    assert!(manager.keyboard_help.is_some());
    assert!(manager.usage.visible);
    assert!(
        manager.list_modal.is_some(),
        "hidden modal remains preserved"
    );
    assert!(
        manager.drain_effects().is_empty(),
        "Help blocks hidden copy"
    );
    assert_eq!(
        mouse_state.pointer_shape,
        termrock::osc::PointerShape::Default
    );
}
