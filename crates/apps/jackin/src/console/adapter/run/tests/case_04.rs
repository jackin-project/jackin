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

const CONTAINER_INFO_OSC52_CHILD: &str = "JACKIN_CONSOLE_CONTAINER_INFO_OSC52_CHILD";
const CONTAINER_INFO_OSC52_MARKER: &str = "JACKIN_CONTAINER_INFO_OSC52_CHILD_COMPLETE";
const CONTAINER_INFO_OSC52_TEST_ID: &str = "console::adapter::run::tests::case_04::container_info_copy_emits_one_osc52_for_painted_24_column_row_and_rejects_23_column_cells";

#[test]
fn container_info_copy_emits_one_osc52_for_painted_24_column_row_and_rejects_23_column_cells()
-> anyhow::Result<()> {
    if std::env::var_os(CONTAINER_INFO_OSC52_CHILD).is_some() {
        exercise_container_info_osc52_child()?;
        print!("{CONTAINER_INFO_OSC52_MARKER}");
        std::io::Write::flush(&mut std::io::stdout())?;
        return Ok(());
    }

    verify_container_info_osc52_child_output()
}

fn exercise_container_info_osc52_child() -> anyhow::Result<()> {
    let _debug_guard = DebugModeGuard::enabled();
    exercise_container_info_24_column_copy_and_scroll()?;
    exercise_container_info_23_column_rejection()?;
    Ok(())
}

fn verify_container_info_osc52_child_output() -> anyhow::Result<()> {
    let mut capture = ContainerInfoOsc52Child::spawn()?;
    let status = match capture.wait_for_exit() {
        Ok(status) => status,
        Err(cause) => return Err(capture.abort_with(cause)),
    };
    let (stdout, stderr) = capture.read_capped_outputs()?;
    let stdout_text = String::from_utf8_lossy(&stdout);
    let test_line = stdout_text
        .lines()
        .find(|line| line.contains(CONTAINER_INFO_OSC52_TEST_ID));
    anyhow::ensure!(
        status.success(),
        "OSC 52 child pid={} exited with {status}; stdout={stdout_text:?}; stderr={:?}",
        capture.pid,
        String::from_utf8_lossy(&stderr)
    );
    anyhow::ensure!(
        test_line.is_some_and(|line| line.ends_with("ok")),
        "OSC 52 child pid={} omitted the exact passing test-harness line; stdout={stdout_text:?}",
        capture.pid
    );
    anyhow::ensure!(
        stdout_text.contains("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;"),
        "OSC 52 child pid={} did not report exactly one passing selected test; stdout={stdout_text:?}",
        capture.pid
    );
    anyhow::ensure!(
        count_byte_occurrences(&stdout, CONTAINER_INFO_OSC52_MARKER.as_bytes()) == 1,
        "OSC 52 child pid={} must emit exactly one completion marker; stdout={stdout_text:?}",
        capture.pid
    );
    anyhow::ensure!(
        count_byte_occurrences(&stdout, b"\x1b]52;c;cjM5\x07") == 1,
        "OSC 52 child pid={} must emit exactly the literal clipboard frame for r39; stdout={stdout_text:?}",
        capture.pid
    );
    anyhow::ensure!(
        count_byte_occurrences(&stdout, b"\x1b]52;") == 1,
        "OSC 52 child pid={} emitted an unexpected OSC 52 prefix count; stdout={stdout_text:?}",
        capture.pid
    );
    Ok(())
}

fn exercise_container_info_24_column_copy_and_scroll() -> anyhow::Result<()> {
    use crossterm::event::{MouseButton, MouseEventKind};

    let mut harness = ResizeHarness::new(24, 20);
    install_container_info_rows(&mut harness);
    let _ = harness.draw();
    let main_area = split_debug_area(harness.area(), true).0;
    let modal_area = manager(&harness.state)
        .list_modal
        .as_ref()
        .expect("painted ContainerInfo modal")
        .rect(main_area);
    anyhow::ensure!(
        modal_area.width == 20,
        "24-column surface paints the exact 20-column ContainerInfo modal"
    );
    let buffer = harness.terminal.backend().buffer();
    anyhow::ensure!(
        buffer_contains_text(buffer, modal_area, "r39"),
        "24-column TestBackend did not paint r39"
    );
    let position = find_painted_ascii_text(buffer, modal_area, "r39")
        .expect("painted r39 has adjacent TestBackend cells");
    let down = stage5_mouse_event(MouseEventKind::Down(MouseButton::Left), position);
    anyhow::ensure!(
        jackin_console::tui::input::mouse::container_info_copyable_row_at(
            manager(&harness.state),
            down,
            main_area,
        ),
        "painted r39 coordinate is accepted by the real copy-row predicate"
    );
    harness.mouse(stage5_mouse_event(MouseEventKind::Moved, position));
    anyhow::ensure!(
        container_info_state(&harness).hovered_row() == Some(0),
        "real adapter move hovers the painted r39 row"
    );
    anyhow::ensure!(
        harness.mouse_state.pointer_shape == termrock::osc::PointerShape::Pointer,
        "painted copyable row uses the pointer shape"
    );
    anyhow::ensure!(manager_mut(&mut harness.state).drain_effects().is_empty());
    harness.mouse(down);
    anyhow::ensure!(
        container_info_state(&harness).copied_row() == Some(0),
        "real app adapter copy effect marks row zero after writing"
    );
    anyhow::ensure!(manager_mut(&mut harness.state).drain_effects().is_empty());

    for _ in 0..80 {
        harness.mouse(stage5_mouse_event(MouseEventKind::ScrollDown, position));
    }
    anyhow::ensure!(
        container_info_state(&harness).scroll.scroll_y > 0,
        "80 real adapter wheel events scroll the ContainerInfo body"
    );
    let _ = harness.draw();
    let buffer = harness.terminal.backend().buffer();
    anyhow::ensure!(
        buffer_contains_text(buffer, modal_area, "value-39"),
        "scrolled 24-column paint exposes value-39"
    );
    anyhow::ensure!(
        !buffer_contains_text(buffer, modal_area, "r39"),
        "scrolled 24-column paint no longer exposes r39"
    );
    Ok(())
}

fn exercise_container_info_23_column_rejection() -> anyhow::Result<()> {
    use crossterm::event::{MouseButton, MouseEventKind};

    let mut harness = ResizeHarness::new(23, 20);
    install_container_info_rows(&mut harness);
    let _ = harness.draw();
    let main_area = split_debug_area(harness.area(), true).0;
    let modal_area = manager(&harness.state)
        .list_modal
        .as_ref()
        .expect("painted ContainerInfo modal")
        .rect(main_area);
    anyhow::ensure!(
        modal_area.width == 19,
        "23-column modal uses the real 19-column narrow boundary"
    );
    anyhow::ensure!(
        !buffer_contains_text(harness.terminal.backend().buffer(), modal_area, "r39"),
        "23-column TestBackend does not paint r39"
    );
    for row in modal_area.y..modal_area.bottom() {
        for column in modal_area.x..modal_area.right() {
            let position = ratatui::layout::Position::new(column, row);
            let down = stage5_mouse_event(MouseEventKind::Down(MouseButton::Left), position);
            anyhow::ensure!(
                !jackin_console::tui::input::mouse::container_info_copyable_row_at(
                    manager(&harness.state),
                    down,
                    main_area,
                ),
                "unpainted 23-column modal cell is not copyable at {position:?}"
            );
            for kind in [
                MouseEventKind::Moved,
                MouseEventKind::Down(MouseButton::Left),
            ] {
                assert_narrow_container_info_event(
                    &mut harness,
                    stage5_mouse_event(kind, position),
                )?;
            }
        }
    }
    Ok(())
}

fn assert_narrow_container_info_event(
    harness: &mut ResizeHarness,
    event: crossterm::event::MouseEvent,
) -> anyhow::Result<()> {
    harness.mouse(event);
    anyhow::ensure!(
        matches!(
            manager(&harness.state).list_modal.as_ref(),
            Some(crate::console::adapter::state::Modal::ContainerInfo { .. })
        ),
        "23-column modal remains after {event:?}"
    );
    anyhow::ensure!(
        container_info_state(harness).hovered_row().is_none(),
        "23-column event cannot hover an unpainted row: {event:?}"
    );
    anyhow::ensure!(
        container_info_state(harness).copied_row().is_none(),
        "23-column event cannot copy an unpainted row: {event:?}"
    );
    anyhow::ensure!(manager_mut(&mut harness.state).drain_effects().is_empty());
    Ok(())
}

fn install_container_info_rows(harness: &mut ResizeHarness) {
    use jackin_console::tui::components::container_info_surface::{
        ContainerInfoRow, ContainerInfoState,
    };

    let mut rows = Vec::with_capacity(41);
    rows.push(ContainerInfoRow::new("ID", "r39").copyable().emphasised());
    for index in 0..40 {
        rows.push(ContainerInfoRow::new("V", format!("value-{index:02}")));
    }
    manager_mut(&mut harness.state).list_modal =
        Some(crate::console::adapter::state::Modal::ContainerInfo {
            state: ContainerInfoState::new("Container info", rows),
        });
}

fn container_info_state(
    harness: &ResizeHarness,
) -> &jackin_console::tui::components::container_info_surface::ContainerInfoState {
    let Some(crate::console::adapter::state::Modal::ContainerInfo { state }) =
        manager(&harness.state).list_modal.as_ref()
    else {
        panic!("ContainerInfo modal remains installed");
    };
    state
}

fn stage5_mouse_event(
    kind: crossterm::event::MouseEventKind,
    position: ratatui::layout::Position,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: position.x,
        row: position.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

fn buffer_contains_text(
    buffer: &ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    needle: &str,
) -> bool {
    (area.y..area.bottom()).any(|row| {
        let line = (area.x..area.right())
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        line.contains(needle)
    })
}

fn find_painted_ascii_text(
    buffer: &ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    needle: &str,
) -> Option<ratatui::layout::Position> {
    let characters = needle
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>();
    for row in area.y..area.bottom() {
        for column in area.x..area.right() {
            let end = column.saturating_add(u16::try_from(characters.len()).ok()?);
            if end > area.right() {
                continue;
            }
            if characters.iter().enumerate().all(|(offset, character)| {
                buffer[(column + offset as u16, row)].symbol() == character
            }) {
                return Some(ratatui::layout::Position::new(column, row));
            }
        }
    }
    None
}

fn count_byte_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

struct ContainerInfoOsc52Child {
    child: Option<std::process::Child>,
    temp: Option<tempfile::TempDir>,
    output_root: std::path::PathBuf,
    stdout_path: std::path::PathBuf,
    stderr_path: std::path::PathBuf,
    stdout_reader: std::fs::File,
    stderr_reader: std::fs::File,
    pid: u32,
    termination_attempted: bool,
}

impl ContainerInfoOsc52Child {
    fn spawn() -> anyhow::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        use std::process::{Command, Stdio};

        let temp = tempfile::Builder::new()
            .prefix("jackin-container-info-osc52-")
            .tempdir()?;
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700))?;
        anyhow::ensure!(
            std::fs::metadata(temp.path())?.permissions().mode() & 0o777 == 0o700,
            "OSC 52 child output directory is not private"
        );
        let output_root = temp.path().to_path_buf();
        let stdout_path = output_root.join("stdout.log");
        let stderr_path = output_root.join("stderr.log");
        let (stdout, stdout_reader) = private_child_log(&stdout_path)?;
        let (stderr, stderr_reader) = private_child_log(&stderr_path)?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", CONTAINER_INFO_OSC52_TEST_ID, "--nocapture"])
            .env(CONTAINER_INFO_OSC52_CHILD, "1")
            .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
            .env_remove("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT")
            .env_remove("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT")
            .env_remove("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let child = command.spawn()?;
        let pid = child.id();
        Ok(Self {
            child: Some(child),
            temp: Some(temp),
            output_root,
            stdout_path,
            stderr_path,
            stdout_reader,
            stderr_reader,
            pid,
            termination_attempted: false,
        })
    }

    fn wait_for_exit(&mut self) -> anyhow::Result<std::process::ExitStatus> {
        use std::time::{Duration, Instant};

        let started = Instant::now();
        loop {
            let status = self
                .child
                .as_mut()
                .expect("child is owned until exit is confirmed")
                .try_wait()
                .map_err(|error| anyhow::anyhow!("poll OSC 52 child pid={}: {error}", self.pid))?;
            if let Some(status) = status {
                let _child = self.child.take();
                return Ok(status);
            }
            self.check_output_lengths()?;
            anyhow::ensure!(
                started.elapsed() < Duration::from_secs(45),
                "OSC 52 child pid={} exceeded the 45-second deadline",
                self.pid
            );
            std::thread::park_timeout(Duration::from_millis(20));
        }
    }

    fn check_output_lengths(&self) -> anyhow::Result<()> {
        const STREAM_CAP: u64 = 64 * 1024;
        for path in [&self.stdout_path, &self.stderr_path] {
            let length = std::fs::metadata(path)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "inspect child pid={} output {}: {error}",
                        self.pid,
                        path.display()
                    )
                })?
                .len();
            anyhow::ensure!(
                length <= STREAM_CAP,
                "OSC 52 child pid={} exceeded the 64 KiB detection threshold at {} (len={length}); polling detects oversize but is not a hard disk cap",
                self.pid,
                path.display()
            );
        }
        Ok(())
    }

    fn read_capped_outputs(&mut self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let stdout = read_child_capture(&mut self.stdout_reader, &self.stdout_path, self.pid)?;
        let stderr = read_child_capture(&mut self.stderr_reader, &self.stderr_path, self.pid)?;
        Ok((stdout, stderr))
    }

    fn abort_with(&mut self, cause: anyhow::Error) -> anyhow::Error {
        let cleanup = self.terminate_and_reap();
        let stdout = read_child_preview(&mut self.stdout_reader);
        let stderr = read_child_preview(&mut self.stderr_reader);
        cause.context(format!(
            "OSC 52 child pid={} failed; cleanup={cleanup:?}; output root={} stdout_prefix={stdout:?} stderr_prefix={stderr:?}",
            self.pid,
            self.output_root.display()
        ))
    }

    fn terminate_and_reap(&mut self) -> Result<Vec<String>, String> {
        use std::time::Duration;

        if self.termination_attempted {
            return Ok(Vec::new());
        }
        self.termination_attempted = true;
        let Some(child) = self.child.as_mut() else {
            return Ok(Vec::new());
        };
        let mut notes = Vec::new();
        match child.try_wait() {
            Ok(Some(_)) => {
                let _child = self.child.take();
                return Ok(notes);
            }
            Ok(None) => {}
            Err(error) => notes.push(format!("initial status poll: {error}")),
        }
        if let Err(error) = self.child.as_mut().expect("owned child").kill() {
            notes.push(format!("kill: {error}"));
        }
        match wait_child_bounded(
            self.child.as_mut().expect("owned child"),
            Duration::from_secs(2),
        ) {
            Ok(_) => {
                let _child = self.child.take();
                Ok(notes)
            }
            Err(error) => {
                let retained_root = self
                    .temp
                    .take()
                    .map_or_else(|| self.output_root.clone(), tempfile::TempDir::keep);
                let pid = self.pid;
                let _child = self.child.take();
                Err(format!(
                    "child pid={pid} was not confirmed reaped within two seconds; {} retained at {}; prior cleanup details={notes:?}",
                    error,
                    retained_root.display()
                ))
            }
        }
    }
}

impl Drop for ContainerInfoOsc52Child {
    fn drop(&mut self) {
        if self.child.is_some()
            && !self.termination_attempted
            && let Err(error) = self.terminate_and_reap()
        {
            let _write_result = std::io::Write::write_all(
                &mut std::io::stderr().lock(),
                format!("unresolved OSC 52 child cleanup: {error}\n").as_bytes(),
            );
        }
    }
}

fn wait_child_bounded(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::ExitStatus, String> {
    let deadline = std::time::Instant::now() + timeout;
    let mut poll_errors = Vec::new();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => poll_errors.push(error.to_string()),
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "bounded reap deadline expired; status-poll errors={poll_errors:?}"
            ));
        }
        std::thread::park_timeout(std::time::Duration::from_millis(20));
    }
}

fn private_child_log(path: &std::path::Path) -> anyhow::Result<(std::fs::File, std::fs::File)> {
    use std::os::unix::fs::PermissionsExt;

    let parent = path
        .parent()
        .expect("private child log has a parent directory");
    let file = tempfile::NamedTempFile::new_in(parent)?;
    anyhow::ensure!(
        file.as_file().metadata()?.permissions().mode() & 0o777 == 0o600,
        "OSC 52 child output file is not private: {}",
        path.display()
    );
    let reader = file.reopen()?;
    let writer = file.persist_noclobber(path).map_err(|error| error.error)?;
    Ok((writer, reader))
}

fn read_child_capture(
    reader: &mut std::fs::File,
    path: &std::path::Path,
    pid: u32,
) -> anyhow::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    const STREAM_CAP: usize = 64 * 1024;
    reader.seek(SeekFrom::Start(0)).map_err(|error| {
        anyhow::anyhow!("seek child pid={pid} output {}: {error}", path.display())
    })?;
    let mut bytes = Vec::with_capacity(STREAM_CAP + 1);
    reader
        .take((STREAM_CAP + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= STREAM_CAP,
        "child pid={pid} output {} is oversized; read was bounded to cap+1",
        path.display()
    );
    Ok(bytes)
}

fn read_child_preview(reader: &mut std::fs::File) -> String {
    use std::io::{Read, Seek, SeekFrom};

    if reader.seek(SeekFrom::Start(0)).is_err() {
        return "<unreadable child output>".to_owned();
    }
    let mut bytes = Vec::new();
    let _unused = reader.take(512).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}
