// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::input::mouse::clickable_at;

fn rendered_list_target(state: &ManagerState<'_>, area: Rect) -> Option<(MouseEvent, usize)> {
    let rows = state.visual_rows_vec();
    (0..area.height)
        .flat_map(|row| (0..area.width).map(move |column| mouse_down_at(column, row)))
        .find_map(|mouse| {
            let row =
                crate::tui::screens::workspaces::update::workspace_list_hover_row_at_position(
                    rows.as_slice(),
                    mouse.column,
                    mouse.row,
                    area,
                    crate::tui::layout::split_seam_column(state.list_split_pct, area.width),
                    |row| state.index_of_row(row).is_some(),
                )?;
            let selected = state.index_of_row(row)?;
            (selected != state.selected).then_some((mouse, selected))
        })
}

#[test]
fn left_release_clears_capture_before_narrow_outside_and_owner_gates() {
    for width in [0, 1, 39, 40] {
        let area = term(width);
        let mut state = list_state_with_saved(3);
        state.selected = 1;
        let original_split = state.list_split_pct;
        state.drag_state = Some(crate::tui::split::DragState {
            anchor_pct: original_split,
            anchor_x: 40,
        });

        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::Up(MouseButton::Left), 80, 40),
            area,
        );
        assert!(state.drag_state.is_none(), "release at width {width}");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), original_split, 4),
            term(40),
        );
        assert_eq!(state.list_split_pct, original_split);
        assert_eq!(state.selected, 1);
        assert!(state.drain_effects().is_empty());
    }

    for owner in ["help", "status", "usage", "workflows"] {
        let mut state = list_state_with_saved(3);
        state.selected = 1;
        let original_split = state.list_split_pct;
        state.drag_state = Some(crate::tui::split::DragState {
            anchor_pct: original_split,
            anchor_x: 40,
        });
        match owner {
            "help" => state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal()),
            "status" => {
                state.status_overlay = Some(crate::tui::components::StatusPopupState::new(
                    "Working",
                    "Please wait",
                ));
            }
            "usage" => state.usage.visible = true,
            "workflows" => {
                state.stage = ManagerStage::Workflows(
                    crate::tui::screens::workflows::model::WorkflowScreen::default(),
                );
            }
            _ => unreachable!(),
        }

        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::Up(MouseButton::Left), 80, 40),
            term(1),
        );
        assert!(state.drag_state.is_none(), "release before {owner} gate");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), 20, 4),
            term(40),
        );
        assert_eq!(
            state.list_split_pct, original_split,
            "{owner} cannot restart drag"
        );
        assert!(state.drain_effects().is_empty());
    }
}

#[test]
fn narrow_painted_row_click_and_hover_remain_available() {
    let area = term(39);
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    let config = jackin_config::AppConfig::default();
    let cwd = tempfile::tempdir().unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(39, 30)).unwrap();
    terminal
        .draw(|frame| crate::tui::view::render(frame, area, &state, &config, cwd.path()))
        .unwrap();

    let (click, selected) = rendered_list_target(&state, area).expect("painted list row target");
    let line = (0..area.width)
        .map(|column| terminal.backend().buffer()[(column, click.row)].symbol())
        .collect::<String>();
    assert!(
        line.chars().any(|character| !character.is_whitespace()),
        "{line:?}"
    );
    assert!(clickable_at(&state, click, area, Some(&config)));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, click.column, click.row),
        area,
        Some(&config),
    );
    assert!(
        state.hover_target.is_some(),
        "the narrow painted row hovers"
    );
    assert!(
        state.drag_state.is_none(),
        "row hover does not acquire the seam"
    );
    handle_mouse_with_config(&mut state, click, area, Some(&config));
    assert_eq!(state.selected, selected);
    assert!(
        state.drag_state.is_none(),
        "row click does not acquire the seam"
    );
}

#[test]
fn narrow_painted_wheel_and_scrollbar_remain_available() {
    let area = term(39);
    let mut config = config_with_scrollable_workspace_and_global_mounts();
    for index in 0..6 {
        config.add_mount(
            &format!("narrow-extra-{index}"),
            MountConfig {
                src: format!("/host/narrow/{index}"),
                dst: format!("/container/narrow/{index}"),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
            None,
        );
    }
    let mut state = selected_demo_state(&config);
    let cwd = tempfile::tempdir().unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(39, 30)).unwrap();
    terminal
        .draw(|frame| crate::tui::view::render(frame, area, &state, &config, cwd.path()))
        .unwrap();
    let areas = list_scroll_areas(&state, area, Some(&config)).expect("painted scroll areas");
    assert!(areas.global.area.width > 0 && areas.global.area.height > 0);
    let wheel = mouse_kind_at(
        MouseEventKind::ScrollDown,
        areas.global.area.x + 1,
        areas.global.area.y + 1,
    );
    handle_mouse_with_config(&mut state, wheel, area, Some(&config));
    assert_eq!(state.list_global_mounts_scroll.offset_y(), 1);

    state.list_global_mounts_scroll.set_offset_y(0);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    let track = termrock::scroll::vertical_scrollbar_area(areas.global.area);
    assert!(
        track.height > 0,
        "the fixture must paint a vertical scrollbar"
    );
    let drag = mouse_kind_at(
        MouseEventKind::Drag(MouseButton::Left),
        track.x,
        track.y + track.height - 1,
    );
    handle_mouse_with_config(&mut state, drag, area, Some(&config));
    assert!(
        state.list_global_mounts_scroll.offset_y() > 0,
        "visible scrollbar drag changes the real scroll offset at width 39"
    );
}

#[test]
fn seam_acquisition_and_update_use_only_the_forty_column_boundary() {
    let mut narrow = list_state_with_saved(3);
    let narrow_seam = crate::tui::layout::split_seam_column(narrow.list_split_pct, 39);
    handle_mouse(
        &mut narrow,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), narrow_seam, 4),
        term(39),
    );
    assert!(narrow.drag_state.is_none());
    let original_split = narrow.list_split_pct;
    narrow.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: original_split,
        anchor_x: narrow_seam,
    });
    handle_mouse(
        &mut narrow,
        mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), 35, 4),
        term(39),
    );
    assert_eq!(narrow.list_split_pct, original_split);

    let mut wide = list_state_with_saved(3);
    let wide_seam = crate::tui::layout::split_seam_column(wide.list_split_pct, 40);
    handle_mouse(
        &mut wide,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), wide_seam, 4),
        term(40),
    );
    assert!(wide.drag_state.is_some());
    handle_mouse(
        &mut wide,
        mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), wide_seam + 5, 4),
        term(40),
    );
    assert_ne!(wide.list_split_pct, DEFAULT_SPLIT_PCT);
    assert!(wide.list_split_pct >= MIN_SPLIT_PCT && wide.list_split_pct <= MAX_SPLIT_PCT);
}

#[test]
fn narrow_blocked_owner_never_reaches_hidden_actions() {
    let area = term(39);
    let mut uncovered = list_state_with_saved(3);
    uncovered.selected = 1;
    let (click, _) = rendered_list_target(&uncovered, area).expect("real narrow row");
    let config = jackin_config::AppConfig::default();

    for owner in ["help", "status", "usage", "workflows"] {
        let mut covered = list_state_with_saved(3);
        covered.selected = 1;
        match owner {
            "help" => covered.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal()),
            "status" => {
                covered.status_overlay = Some(crate::tui::components::StatusPopupState::new(
                    "Working",
                    "Please wait",
                ));
            }
            "usage" => covered.usage.visible = true,
            "workflows" => {
                covered.stage = ManagerStage::Workflows(
                    crate::tui::screens::workflows::model::WorkflowScreen::default(),
                );
            }
            _ => unreachable!(),
        }
        let selected = covered.selected;
        let scroll = covered.list_global_mounts_scroll.offset_y();
        assert!(
            !clickable_at(&covered, click, area, Some(&config)),
            "{owner}"
        );
        handle_mouse_with_config(&mut covered, click, area, Some(&config));
        handle_mouse_with_config(
            &mut covered,
            mouse_kind_at(MouseEventKind::ScrollDown, click.column, click.row),
            area,
            Some(&config),
        );
        assert_eq!(covered.selected, selected, "{owner} blocks row selection");
        assert_eq!(covered.list_global_mounts_scroll.offset_y(), scroll);
        assert!(covered.drain_effects().is_empty(), "{owner} blocks effects");
    }
}

#[test]
fn viewport_boundaries_have_no_invisible_pointer_targets() {
    let state = list_state_with_saved(3);
    for (width, height) in [
        (0, 0),
        (1, 1),
        (2, 3),
        (23, 6),
        (24, 10),
        (39, 30),
        (40, 30),
        (120, 40),
    ] {
        let area = Rect::new(0, 0, width, height);
        for mouse in [
            mouse_kind_at(MouseEventKind::Moved, width, 0),
            mouse_kind_at(MouseEventKind::Moved, 0, height),
            mouse_kind_at(MouseEventKind::Down(MouseButton::Left), width, height),
        ] {
            assert!(
                !clickable_at(&state, mouse, area, None),
                "{width}x{height}: {mouse:?}"
            );
        }
        if width > 0 && height > 0 {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            let config = jackin_config::AppConfig::default();
            let cwd = tempfile::tempdir().unwrap();
            terminal
                .draw(|frame| crate::tui::view::render(frame, area, &state, &config, cwd.path()))
                .unwrap();
            assert_eq!(terminal.backend().buffer().area, area);
        }
        for row in [0, height.saturating_sub(1)] {
            if height == 0 {
                continue;
            }
            for column in 0..width {
                assert!(
                    !crate::tui::screens::workspaces::update::workspace_list_clickable_at_position(
                        column,
                        row,
                        area,
                        state.list_split_pct,
                        state.list_modal.is_some(),
                        state.visual_rows_vec().as_slice(),
                        |item| state.index_of_row(item).is_some(),
                    ),
                    "header/footer target at {width}x{height} ({column},{row})"
                );
            }
        }
    }

    let viewport = term(120);
    let mut moving = list_state_with_saved(3);
    let (target, _) = rendered_list_target(&moving, viewport).expect("painted row target");
    handle_mouse(
        &mut moving,
        mouse_kind_at(MouseEventKind::Moved, target.column, target.row),
        viewport,
    );
    assert!(moving.hover_target.is_some());
    moving.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: moving.list_split_pct,
        anchor_x: target.column,
    });
    handle_mouse(
        &mut moving,
        mouse_kind_at(MouseEventKind::Moved, viewport.width, target.row),
        viewport,
    );
    assert!(
        moving.hover_target.is_none(),
        "outside movement has no pointer target"
    );
    assert!(
        moving.drag_state.is_some(),
        "outside movement preserves capture"
    );
}

#[test]
fn pure_workspace_plan_releases_before_policy_and_never_restarts_capture() {
    let state = list_state_with_saved(3);
    let rows = state.visual_rows_vec();
    let selectable = |row| state.index_of_row(row).is_some();
    let captured = Some(crate::tui::split::DragState {
        anchor_pct: DEFAULT_SPLIT_PCT,
        anchor_x: 20,
    });
    assert_eq!(
        crate::tui::screens::workspaces::update::workspace_list_mouse_plan(
            mouse_kind_at(MouseEventKind::Up(MouseButton::Left), 100, 100),
            Rect::new(0, 0, 0, 0),
            DEFAULT_SPLIT_PCT,
            captured,
            true,
            &[],
            |_| false,
        ),
        crate::tui::screens::workspaces::update::WorkspaceListMousePlan::EndDrag
    );
    let seam39 = crate::tui::layout::split_seam_column(DEFAULT_SPLIT_PCT, 39);
    assert_eq!(
        crate::tui::screens::workspaces::update::workspace_list_mouse_plan(
            mouse_kind_at(MouseEventKind::Down(MouseButton::Left), seam39, 4),
            term(39),
            DEFAULT_SPLIT_PCT,
            None,
            false,
            rows.as_slice(),
            selectable,
        ),
        crate::tui::screens::workspaces::update::WorkspaceListMousePlan::Continue
    );
    assert_eq!(
        crate::tui::screens::workspaces::update::workspace_list_mouse_plan(
            mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), 35, 4),
            term(39),
            DEFAULT_SPLIT_PCT,
            captured,
            false,
            rows.as_slice(),
            |_| true,
        ),
        crate::tui::screens::workspaces::update::WorkspaceListMousePlan::Continue
    );
    assert_eq!(
        crate::tui::screens::workspaces::update::workspace_list_mouse_plan(
            mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), 35, 4),
            term(40),
            DEFAULT_SPLIT_PCT,
            None,
            false,
            rows.as_slice(),
            |_| true,
        ),
        crate::tui::screens::workspaces::update::WorkspaceListMousePlan::Continue
    );
    let seam40 = crate::tui::layout::split_seam_column(DEFAULT_SPLIT_PCT, 40);
    assert!(matches!(
        crate::tui::screens::workspaces::update::workspace_list_mouse_plan(
            mouse_kind_at(MouseEventKind::Down(MouseButton::Left), seam40, 4),
            term(40),
            DEFAULT_SPLIT_PCT,
            None,
            false,
            rows.as_slice(),
            |_| true,
        ),
        crate::tui::screens::workspaces::update::WorkspaceListMousePlan::StartDrag(_)
    ));
}
