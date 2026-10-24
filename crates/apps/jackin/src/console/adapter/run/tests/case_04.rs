// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

struct ResizeHarness {
    temp: tempfile::TempDir,
    terminal: ratatui::Terminal<ratatui::backend::TestBackend>,
    config: AppConfig,
    paths: JackinPaths,
    state: ConsoleState,
    mouse_state: ConsoleMouseState,
}

impl ResizeHarness {
    fn new(width: u16, height: u16) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "stage2-workspace".to_owned(),
            jackin_config::WorkspaceConfig {
                workdir: temp.path().display().to_string(),
                ..Default::default()
            },
        );
        let state = jackin_console::tui::console::new_console_state(&config, temp.path())
            .expect("console state");
        Self {
            paths: JackinPaths::for_tests(temp.path()),
            temp,
            terminal: ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("test terminal"),
            config,
            state,
            mouse_state: ConsoleMouseState::new(),
        }
    }

    fn area(&self) -> ratatui::layout::Rect {
        self.terminal.size().expect("terminal size").into()
    }

    fn draw(&mut self) -> bool {
        let mut overlay_active = false;
        let mut jank = jackin_telemetry::ui::JankMonitor::default();
        draw_console_frame(
            &mut self.terminal,
            &mut self.state,
            DrawConsoleContext {
                config: &self.config,
                cwd: self.temp.path(),
                mouse_state: &mut self.mouse_state,
                container_info_overlay_active: &mut overlay_active,
                action_parent: None,
                jank_monitor: &mut jank,
            },
        )
        .expect("draw console frame")
    }

    fn present(
        &mut self,
        pacer: &mut jackin_console::tui::runtime::ConsoleFramePacer,
        needs_redraw: &mut bool,
    ) {
        let mut overlay_active = false;
        let mut jank = jackin_telemetry::ui::JankMonitor::default();
        present_owed_frame(
            pacer,
            needs_redraw,
            &mut self.terminal,
            &mut self.state,
            DrawConsoleContext {
                config: &self.config,
                cwd: self.temp.path(),
                mouse_state: &mut self.mouse_state,
                container_info_overlay_active: &mut overlay_active,
                action_parent: None,
                jank_monitor: &mut jank,
            },
        )
        .expect("present owed frame");
    }

    fn resize(&mut self, width: u16, height: u16) -> bool {
        self.terminal.backend_mut().resize(width, height);
        let mut needs_redraw = false;
        handle_terminal_resize(
            &mut self.terminal,
            width,
            height,
            &mut self.state,
            &mut self.mouse_state,
            &mut needs_redraw,
        )
        .expect("terminal resize");
        needs_redraw
    }

    fn mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        let area = self.area();
        let mut action_handler = ();
        let mut runner = ();
        let mut inputs = ConsoleLoopInputs {
            config: &mut self.config,
            paths: &self.paths,
            cwd: self.temp.path(),
            action_handler: &mut action_handler,
            runner: &mut runner,
            startup_error_pending: false,
        };
        let mut needs_redraw = false;
        handle_mouse_event(
            &mut self.state,
            mouse,
            area,
            &mut inputs,
            &mut self.mouse_state,
            &mut needs_redraw,
        )
        .expect("handle mouse event");
    }
}

fn manager(state: &ConsoleState) -> &jackin_console::tui::state::ManagerState<'_> {
    let ConsoleStage::Manager(manager) = &state.stage;
    manager
}

fn manager_mut(state: &mut ConsoleState) -> &mut jackin_console::tui::state::ManagerState<'static> {
    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager
}

fn workspace_pointer(
    manager: &jackin_console::tui::state::ManagerState<'_>,
    area: ratatui::layout::Rect,
) -> (ratatui::layout::Position, usize) {
    let rows = manager.visual_rows_vec();
    for row in 0..area.height {
        for column in 0..area.width {
            let Some(list_row) = jackin_console::tui::screens::workspaces::update::workspace_list_hover_row_at_position(
                rows.as_slice(),
                column,
                row,
                area,
                jackin_console::tui::layout::split_seam_column(
                    manager.list_split_pct,
                    area.width,
                ),
                |row| manager.index_of_row(row).is_some(),
            ) else {
                continue;
            };
            if let Some(index) = manager.index_of_row(list_row)
                && index != manager.selected
            {
                return (ratatui::layout::Position::new(column, row), index);
            }
        }
    }
    panic!("workspace fixture has a painted selectable row at {area:?}");
}

#[test]
fn resize_cancels_capture_before_the_next_queued_pointer_action() {
    use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};

    let mut harness = ResizeHarness::new(120, 40);
    harness.draw();
    let original_split = manager(&harness.state).list_split_pct;
    manager_mut(&mut harness.state).drag_state = Some(jackin_console::tui::split::DragState {
        anchor_pct: original_split,
        anchor_x: 60,
    });
    let (click_position, target_selected) =
        workspace_pointer(manager(&harness.state), termrock_test_area(39, 30));
    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 38,
        row: 4,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_position.x,
        row: click_position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let release = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 100,
        row: 50,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let mut queue = std::collections::VecDeque::from([
        Event::Resize(39, 30),
        Event::Mouse(release),
        Event::Mouse(drag),
        Event::Mouse(click),
    ]);
    assert!(matches!(
        pop_pending_event(&mut queue, false),
        Some(Event::Resize(39, 30))
    ));
    assert!(harness.resize(39, 30));
    assert!(manager(&harness.state).drag_state.is_none());
    assert!(
        pop_pending_event(&mut queue, true).is_some_and(|event| matches!(event, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Up(MouseButton::Left))),
        "left release is serviced before the geometry gate"
    );
    harness.mouse(release);
    assert!(pop_pending_event(&mut queue, true).is_none());
    assert_eq!(
        queue.len(),
        2,
        "the drag and click remain ordered while paint is owed"
    );

    let mut pacer = jackin_console::tui::runtime::ConsoleFramePacer::new();
    let mut needs_redraw = true;
    harness.present(&mut pacer, &mut needs_redraw);
    assert!(
        !harness.mouse_state.geometry_invalid,
        "fresh paint validates geometry"
    );
    assert!(matches!(
        pop_pending_event(&mut queue, false),
        Some(Event::Mouse(mouse)) if mouse.kind == MouseEventKind::Drag(MouseButton::Left)
    ));
    harness.mouse(drag);
    assert_eq!(manager(&harness.state).list_split_pct, original_split);
    assert!(manager(&harness.state).drag_state.is_none());
    assert!(matches!(
        pop_pending_event(&mut queue, false),
        Some(Event::Mouse(mouse)) if mouse.kind == MouseEventKind::Down(MouseButton::Left)
    ));
    harness.mouse(click);
    assert_eq!(manager(&harness.state).selected, target_selected);
}

fn termrock_test_area(width: u16, height: u16) -> ratatui::layout::Rect {
    ratatui::layout::Rect::new(0, 0, width, height)
}

#[test]
fn resize_requeries_stationary_pointer_after_fresh_paint_without_actions() {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut harness = ResizeHarness::new(120, 40);
    harness.draw();
    let (position, _) = workspace_pointer(manager(&harness.state), harness.area());
    harness.mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: position.x,
        row: position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    assert!(manager(&harness.state).hover_target.is_some());
    let selected_before_resize = manager(&harness.state).selected;
    let physical_event_time = harness.mouse_state.last_event_at;

    assert!(harness.resize(39, 30));
    assert!(manager(&harness.state).hover_target.is_none());
    let redraw_owed = harness.draw();
    assert!(redraw_owed, "new painted hover needs one follow-up frame");
    assert!(manager(&harness.state).hover_target.is_some());
    assert_eq!(manager(&harness.state).selected, selected_before_resize);
    assert_eq!(harness.mouse_state.last_event_at, physical_event_time);
    assert!(manager_mut(&mut harness.state).drain_effects().is_empty());
}

#[test]
fn resize_outside_pointer_stays_inert_and_same_owner_pointer_recovers() {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut harness = ResizeHarness::new(120, 40);
    harness.draw();
    let (position, _) = workspace_pointer(manager(&harness.state), harness.area());
    harness.mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: position.x,
        row: position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    harness.resize(1, 1);
    let _ = harness.draw();
    assert_eq!(harness.mouse_state.last_pointer_position, Some(position));
    assert!(manager(&harness.state).hover_target.is_none());
    assert_eq!(
        harness.mouse_state.pointer_shape,
        termrock::osc::PointerShape::Default
    );

    harness.resize(120, 40);
    assert!(harness.draw());
    assert!(manager(&harness.state).hover_target.is_some());

    manager_mut(&mut harness.state).list_modal =
        Some(crate::console::adapter::state::Modal::ContainerInfo {
            state: jackin_console::tui::components::container_info_surface::ContainerInfoState::new(
                "Pointer owner",
                vec![
                    jackin_console::tui::components::container_info_surface::ContainerInfoRow::new(
                        "Run ID",
                        "run-pointer",
                    )
                    .copyable()
                    .emphasised(),
                ],
            ),
        });
    let _ = harness.draw();
    let main_area = split_debug_area(harness.area(), jackin_diagnostics::is_debug_mode()).0;
    let pointer = (0..main_area.height)
        .flat_map(|row| (0..main_area.width).map(move |column| (column, row)))
        .find(|(column, row)| {
            jackin_console::tui::input::mouse::container_info_copyable_row_at(
                manager(&harness.state),
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: *column,
                    row: *row,
                    modifiers: crossterm::event::KeyModifiers::NONE,
                },
                main_area,
            )
        })
        .expect("painted copyable row");
    harness.mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: pointer.0,
        row: pointer.1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    let Some(crate::console::adapter::state::Modal::ContainerInfo { state: info }) =
        manager(&harness.state).list_modal.as_ref()
    else {
        panic!("container info modal is retained");
    };
    assert_eq!(info.hovered_row(), Some(0));
    assert!(
        !harness.draw(),
        "same-owner frame does not owe another hover paint"
    );
    let Some(crate::console::adapter::state::Modal::ContainerInfo { state: info }) =
        manager(&harness.state).list_modal.as_ref()
    else {
        panic!("container info modal is retained");
    };
    assert_eq!(info.hovered_row(), Some(0), "same-owner hover is preserved");
}

struct DebugModeGuard(bool);

impl DebugModeGuard {
    fn enabled() -> Self {
        let previous = jackin_diagnostics::is_debug_mode();
        jackin_diagnostics::set_debug_mode(true);
        Self(previous)
    }
}

impl Drop for DebugModeGuard {
    fn drop(&mut self) {
        jackin_diagnostics::set_debug_mode(self.0);
    }
}

#[test]
fn debug_footer_and_quit_hint_use_the_same_painted_body_for_input() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let _debug_guard = DebugModeGuard::enabled();
    for debug_enabled in [false, true] {
        jackin_diagnostics::set_debug_mode(debug_enabled);
        let mut harness = ResizeHarness::new(120, 40);
        harness.draw();
        let (main_area, debug_bar) = split_debug_area(harness.area(), debug_enabled);
        assert_eq!(debug_bar.is_some(), debug_enabled);
        let (painted_target, target_selected) =
            workspace_pointer(manager(&harness.state), main_area);
        let selected = manager(&harness.state).selected;
        assert_ne!(selected, target_selected);
        assert!(jackin_console::tui::input::mouse::clickable_at(
            manager(&harness.state),
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: painted_target.x,
                row: painted_target.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            },
            main_area,
            Some(&harness.config),
        ));
        let painted_line = (0..main_area.width)
            .map(|column| harness.terminal.backend().buffer()[(column, painted_target.y)].symbol())
            .collect::<String>();
        assert!(
            painted_line
                .chars()
                .any(|character| !character.is_whitespace()),
            "the stationary pointer test uses painted content: {painted_line:?}"
        );
        harness.mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: harness.area().height - 1,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(manager(&harness.state).selected, selected);

        harness.mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: painted_target.x,
            row: painted_target.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(
            manager(&harness.state).hover_target.is_some(),
            "the real physical move hovers the painted underlying row"
        );
        assert!(manager_mut(&mut harness.state).drain_effects().is_empty());
        harness.state.open_quit_confirm();
        assert!(
            !harness.draw(),
            "fresh root-quit paint does not restore hidden-row hover"
        );
        assert!(harness.state.quit_confirm.is_some());
        assert!(manager(&harness.state).hover_target.is_none());
        assert_eq!(
            harness.mouse_state.pointer_shape,
            termrock::osc::PointerShape::Default
        );
        assert_eq!(manager(&harness.state).selected, selected);
        assert!(manager_mut(&mut harness.state).drain_effects().is_empty());
        assert!(!harness.draw(), "stable root-quit paint stays quiescent");
        assert!(manager(&harness.state).hover_target.is_none());
        assert_eq!(
            harness.mouse_state.pointer_shape,
            termrock::osc::PointerShape::Default
        );
        let split_pct = manager(&harness.state).list_split_pct;
        manager_mut(&mut harness.state).drag_state = Some(jackin_console::tui::split::DragState {
            anchor_pct: split_pct,
            anchor_x: painted_target.x,
        });
        harness.mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: painted_target.x,
            row: painted_target.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(harness.state.quit_confirm.is_some());
        assert!(manager(&harness.state).drag_state.is_none());
        assert!(manager(&harness.state).hover_target.is_none());
        assert_eq!(
            harness.mouse_state.pointer_shape,
            termrock::osc::PointerShape::Default
        );
        assert_eq!(manager(&harness.state).selected, selected);
        assert!(manager_mut(&mut harness.state).drain_effects().is_empty());

        let confirm = harness.state.quit_confirm_state().expect("quit modal");
        let (body, hint) = quit_confirm_body_and_hint_area(main_area);
        assert_eq!(body.height + hint.height, main_area.height);
        assert_eq!(body.bottom(), hint.y);
        assert_eq!(hint.height, 1);
        let dialog = quit_confirm_area(body, confirm);
        assert!(body.contains(ratatui::layout::Position::new(dialog.x, dialog.y)));
        assert!(dialog.bottom() <= body.bottom());
        assert!(!body.contains(ratatui::layout::Position::new(hint.x, hint.y)));

        harness.mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: dialog.x,
            row: dialog.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(
            harness.state.quit_confirm.is_some(),
            "dialog body remains modal-owned"
        );
        harness.mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hint.x,
            row: hint.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(
            harness.state.quit_confirm.is_none(),
            "the hint is outside the body and follows existing outside-dismiss policy"
        );
        assert_eq!(manager(&harness.state).selected, selected);
    }
}

#[test]
fn postpaint_hover_redraw_is_owed_once_then_quiescent() {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut harness = ResizeHarness::new(120, 40);
    let _ = harness.draw();
    let (position, _) = workspace_pointer(manager(&harness.state), harness.area());
    harness.mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: position.x,
        row: position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    harness.resize(120, 40);
    assert!(manager(&harness.state).hover_target.is_none());

    let mut pacer = jackin_console::tui::runtime::ConsoleFramePacer::new();
    let mut needs_redraw = true;
    harness.present(&mut pacer, &mut needs_redraw);
    assert!(
        needs_redraw,
        "post-paint hover change is propagated through pacing"
    );
    assert!(manager(&harness.state).hover_target.is_some());
    harness.present(&mut pacer, &mut needs_redraw);
    assert!(!needs_redraw, "stable hover does not create a redraw loop");
    assert!(manager(&harness.state).hover_target.is_some());
}
