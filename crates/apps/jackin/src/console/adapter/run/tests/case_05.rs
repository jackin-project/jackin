// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1};
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};
use std::io;

fn usage_account() -> jackin_console::tui::screens::usage::UsageAccount {
    jackin_console::tui::screens::usage::UsageAccount {
        provider_id: "provider".to_owned(),
        canonical_account_id: "account".to_owned(),
        unresolved: false,
        provider: "Provider".to_owned(),
        account: "Adapter account".to_owned(),
        status: "available".to_owned(),
        lifecycle: UsageLifecycleV1::Available,
        freshness_phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
        identity_kind: Some(UsageIdentityKindV1::ProviderStableHandle),
        plan_label: None,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
        provider_issues: Vec::new(),
        windows: Vec::new(),
        metric_groups: Vec::new(),
    }
}

struct UsageDrawHarness {
    temp: tempfile::TempDir,
    config: AppConfig,
    state: ConsoleState,
    terminal: ratatui::Terminal<ratatui::backend::TestBackend>,
    mouse: ConsoleMouseState,
}

impl UsageDrawHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = AppConfig::default();
        let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())
            .expect("console state");
        let ConsoleStage::Manager(manager) = &mut state.stage;
        manager.usage.visible = true;
        manager.usage.screen = Some(jackin_console::tui::screens::usage::UsageScreenState {
            accounts: vec![usage_account()],
            ..Default::default()
        });
        Self {
            temp,
            config,
            state,
            terminal: ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40))
                .expect("test terminal"),
            mouse: ConsoleMouseState::new(),
        }
    }

    fn area(&self) -> Rect {
        self.terminal.size().expect("terminal size").into()
    }

    fn manager(&self) -> &jackin_console::tui::state::ManagerState<'static> {
        let ConsoleStage::Manager(manager) = &self.state.stage;
        manager
    }

    fn manager_mut(&mut self) -> &mut jackin_console::tui::state::ManagerState<'static> {
        let ConsoleStage::Manager(manager) = &mut self.state.stage;
        manager
    }

    fn draw(&mut self) -> anyhow::Result<bool> {
        let mut overlay_active = false;
        let mut jank = jackin_telemetry::ui::JankMonitor::default();
        draw_console_frame(
            &mut self.terminal,
            &mut self.state,
            DrawConsoleContext {
                config: &self.config,
                cwd: self.temp.path(),
                mouse_state: &mut self.mouse,
                container_info_overlay_active: &mut overlay_active,
                action_parent: None,
                jank_monitor: &mut jank,
            },
        )
    }
}

fn install_non_usage_owner(harness: &mut UsageDrawHarness, owner: &str) {
    if owner == "quit" {
        harness.state.open_quit_confirm();
        return;
    }
    let manager = harness.manager_mut();
    match owner {
        "help" => manager.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal()),
        "status" => {
            manager.status_overlay = Some(jackin_console::tui::components::StatusPopupState::new(
                "Busy", "Working",
            ));
        }
        "list-modal" => {
            manager.list_modal = Some(jackin_console::tui::state::Modal::ContainerInfo {
                state:
                    jackin_console::tui::components::container_info_surface::ContainerInfoState::new(
                        "Container information",
                        Vec::new(),
                    ),
            });
        }
        "editor" => {
            manager.stage =
                jackin_console::tui::state::ManagerStage::Editor(EditorState::new_edit(
                    "workspace".to_owned(),
                    jackin_config::WorkspaceConfig::default(),
                ));
        }
        "settings" => {
            manager.stage = jackin_console::tui::state::ManagerStage::Settings(
                jackin_console::tui::state::SettingsState::from_config(&AppConfig::default()),
            );
        }
        "create-prelude" => {
            manager.stage = jackin_console::tui::state::ManagerStage::CreatePrelude(
                jackin_console::tui::state::CreatePreludeState::default(),
            );
        }
        "workflows" => {
            manager.stage = jackin_console::tui::state::ManagerStage::Workflows(
                jackin_console::tui::screens::workflows::model::WorkflowScreen::default(),
            );
        }
        "absent" => manager.usage.screen = None,
        _ => unreachable!("known non-Usage owner"),
    }
}

fn assert_non_usage_owner_revokes(harness: &mut UsageDrawHarness, owner: &str) {
    harness.draw().expect("initial Usage frame");
    let area = harness.area();
    let y = overview_row(harness.terminal.backend().buffer(), area);
    let click = click_at(1, y);
    assert!(crate::console::adapter::input::clickable_at(
        harness.manager(),
        click,
        area,
        Some(&harness.config)
    ));
    let selected = harness.manager().usage.screen.as_ref().unwrap().selected;
    let usage_before = harness.manager().usage.screen.clone();
    harness.mouse.last_pointer_position = Some(Position::new(1, y));
    harness
        .draw()
        .expect("stationary Usage pointer obtains its painted shape");
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Pointer
    );
    install_non_usage_owner(harness, owner);
    if owner == "quit" {
        assert!(harness.state.quit_confirm.is_some());
    } else {
        assert!(!crate::console::adapter::input::clickable_at(
            harness.manager(),
            click,
            area,
            Some(&harness.config)
        ));
    }
    harness
        .draw()
        .expect("actual occluded or non-Usage owner frame");
    assert!(!crate::console::adapter::input::clickable_at(
        harness.manager(),
        click,
        area,
        Some(&harness.config)
    ));
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Default
    );
    if owner == "absent" {
        assert!(harness.manager().usage.screen.is_none());
    } else {
        assert_eq!(harness.manager().usage.screen, usage_before);
        assert_eq!(
            harness
                .manager()
                .usage
                .screen
                .as_ref()
                .map(|screen| screen.selected),
            Some(selected)
        );
    }
    assert!(harness.manager_mut().drain_effects().is_empty());
}

fn overview_row(buffer: &ratatui::buffer::Buffer, area: Rect) -> u16 {
    let list_inner = jackin_console::tui::screens::usage::usage_frame_layout(area).list_inner;
    (list_inner.y..list_inner.y + list_inner.height)
        .find(|&y| {
            (list_inner.x..list_inner.x + list_inner.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .contains("Overview")
        })
        .expect("painted Overview row")
}

fn click_at(x: u16, y: u16) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

#[test]
fn t03_adapter_publishes_usage_only_after_successful_draw() {
    let mut harness = UsageDrawHarness::new();
    let area = harness.area();
    let unpainted_list = jackin_console::tui::screens::usage::usage_frame_layout(area).list_inner;
    let unpainted_probe = click_at(unpainted_list.x, unpainted_list.y);
    assert!(
        !crate::console::adapter::input::clickable_at(
            harness.manager(),
            unpainted_probe,
            area,
            Some(&harness.config),
        ),
        "no adapter receipt exists before the first draw"
    );
    let mut unowned =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(area.width, area.height))
            .expect("unowned view terminal");
    let config = harness.config.clone();
    let cwd = harness.temp.path().to_path_buf();
    {
        let ConsoleStage::Manager(manager) = &mut harness.state.stage;
        crate::console::adapter::prepare_for_render(manager, &config, &cwd, area);
    }
    assert!(
        !crate::console::adapter::input::clickable_at(
            harness.manager(),
            unpainted_probe,
            area,
            Some(&harness.config),
        ),
        "prepare_for_render alone does not authorize a pointer target"
    );
    unowned
        .draw(|frame| {
            jackin_console::tui::view::render(frame, area, harness.manager(), &config, &cwd);
        })
        .expect("ordinary render without adapter publication");
    let unowned_y = overview_row(unowned.backend().buffer(), area);
    assert!(
        !crate::console::adapter::input::clickable_at(
            harness.manager(),
            click_at(1, unowned_y),
            area,
            Some(&harness.config),
        ),
        "prepare plus a plain view render is not adapter authority"
    );

    harness.draw().expect("successful terminal draw");
    let y = overview_row(harness.terminal.backend().buffer(), area);
    assert!(crate::console::adapter::input::clickable_at(
        harness.manager(),
        click_at(1, y),
        area,
        Some(&harness.config),
    ));
    for owner in [
        "help",
        "status",
        "list-modal",
        "editor",
        "settings",
        "create-prelude",
        "workflows",
        "absent",
        "quit",
    ] {
        let mut other_owner = UsageDrawHarness::new();
        assert_non_usage_owner_revokes(&mut other_owner, owner);
    }
}

struct FailingDrawBackend {
    size: Size,
}

impl Backend for FailingDrawBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, _content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        Err(io::Error::other("injected backend draw failure"))
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(Position::new(0, 0))
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, _position: P) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> Result<(), Self::Error> {
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.size)
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.size,
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn t03_adapter_failed_draw_revokes_usage_pointer_authority() {
    let mut harness = UsageDrawHarness::new();
    harness.draw().expect("initial successful draw");
    let area = harness.area();
    let y = overview_row(harness.terminal.backend().buffer(), area);
    let ConsoleStage::Manager(manager) = &harness.state.stage;
    assert!(crate::console::adapter::input::clickable_at(
        manager,
        click_at(1, y),
        area,
        Some(&harness.config),
    ));

    let mut failing = ratatui::Terminal::new(FailingDrawBackend {
        size: Size::new(area.width, area.height),
    })
    .expect("failing backend terminal");
    let mut overlay_active = false;
    let mut jank = jackin_telemetry::ui::JankMonitor::default();
    let result = draw_console_frame(
        &mut failing,
        &mut harness.state,
        DrawConsoleContext {
            config: &harness.config,
            cwd: harness.temp.path(),
            mouse_state: &mut harness.mouse,
            container_info_overlay_active: &mut overlay_active,
            action_parent: None,
            jank_monitor: &mut jank,
        },
    );
    assert!(
        result.is_err(),
        "backend draw failure reaches production path"
    );
    let ConsoleStage::Manager(manager) = &harness.state.stage;
    assert!(!crate::console::adapter::input::clickable_at(
        manager,
        click_at(1, y),
        area,
        Some(&harness.config),
    ));
}

#[test]
fn t03_adapter_requeries_stationary_usage_pointer_without_actions() {
    let mut harness = UsageDrawHarness::new();
    let _hover_frame_owed = harness.draw().expect("initial successful draw");
    let area = harness.area();
    let y = overview_row(harness.terminal.backend().buffer(), area);
    let before = {
        let ConsoleStage::Manager(manager) = &harness.state.stage;
        manager.usage.screen.as_ref().unwrap().selected
    };
    harness.mouse.last_pointer_position = Some(Position::new(1, y));
    let _hover_frame_owed = harness.draw().expect("stationary pointer repaint");
    let ConsoleStage::Manager(manager) = &mut harness.state.stage;
    assert_eq!(manager.usage.screen.as_ref().unwrap().selected, before);
    assert!(manager.drain_effects().is_empty());
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Pointer,
        "stationary pointer is recomputed as a hand over a painted row"
    );
    assert!(crate::console::adapter::input::clickable_at(
        manager,
        click_at(1, y),
        area,
        Some(&harness.config),
    ));

    let layout = jackin_console::tui::screens::usage::usage_frame_layout(area);
    harness.mouse.last_pointer_position = Some(Position::new(
        layout.detail_status.x,
        layout.detail_status.y,
    ));
    harness.draw().expect("stationary inert-region repaint");
    {
        let ConsoleStage::Manager(manager) = &harness.state.stage;
        assert_eq!(manager.usage.screen.as_ref().unwrap().selected, before);
    }
    assert!(harness.manager_mut().drain_effects().is_empty());
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Default
    );

    let mut redraw = false;
    handle_terminal_resize(
        &mut harness.terminal,
        100,
        36,
        &mut harness.state,
        &mut harness.mouse,
        &mut redraw,
    )
    .expect("production resize clears the old Usage receipt");
    let resized_area = Rect::new(0, 0, 100, 36);
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Default
    );
    assert!(!crate::console::adapter::input::clickable_at(
        harness.manager(),
        click_at(1, y),
        resized_area,
        Some(&harness.config),
    ));
    harness.draw().expect("fresh resized Usage frame");
    let resized_y = overview_row(harness.terminal.backend().buffer(), resized_area);
    harness.mouse.last_pointer_position = Some(Position::new(1, resized_y));
    harness
        .draw()
        .expect("stationary pointer is queried after resized paint");
    assert_eq!(
        harness.mouse.pointer_shape,
        termrock::osc::PointerShape::Pointer
    );
    {
        let ConsoleStage::Manager(manager) = &harness.state.stage;
        assert_eq!(manager.usage.screen.as_ref().unwrap().selected, before);
    }
    assert!(harness.manager_mut().drain_effects().is_empty());
}
