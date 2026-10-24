// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

const NOW: i64 = 1_800_000_000;

fn usage_account(id: &str, label: &str) -> crate::tui::screens::usage::UsageAccount {
    crate::tui::screens::usage::UsageAccount {
        provider_id: "provider".to_owned(),
        canonical_account_id: id.to_owned(),
        unresolved: false,
        provider: "Provider".to_owned(),
        account: label.to_owned(),
        status: "available".to_owned(),
        lifecycle: jackin_protocol::usage_broker::UsageLifecycleV1::Available,
        freshness_phase: jackin_protocol::usage_broker::UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(NOW),
        retry_at_epoch: None,
        is_stale: false,
        identity_kind: Some(
            jackin_protocol::usage_broker::UsageIdentityKindV1::ProviderStableHandle,
        ),
        plan_label: None,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
        provider_issues: Vec::new(),
        windows: Vec::new(),
        metric_groups: Vec::new(),
    }
}

fn long_usage_screen() -> crate::tui::screens::usage::UsageScreenState {
    let accounts = vec![
        usage_account("a", "Alpha account"),
        usage_account("b", "Beta account"),
        usage_account("c", "Gamma account"),
    ];
    crate::tui::screens::usage::UsageScreenState {
        accounts,
        selected: 1,
        selected_id: Some("provider:a".to_owned()),
        detail: true,
        scroll: u16::MAX,
        notice: Some(format!(
            "wrapped detail {} TAIL-END",
            "界 detail ".repeat(500)
        )),
        ..crate::tui::screens::usage::UsageScreenState::default()
    }
}

fn manager_with_usage(
    screen: crate::tui::screens::usage::UsageScreenState,
) -> ManagerState<'static> {
    let mut state = list_state();
    state.usage.visible = true;
    state.usage.screen = Some(screen);
    state
}

fn paint_with_receipt(
    state: &mut ManagerState<'_>,
    area: Rect,
) -> (Buffer, crate::tui::screens::usage::UsagePaintReceipt) {
    let backend = TestBackend::new(area.right(), area.bottom());
    let mut terminal = Terminal::new(backend).expect("Usage TestBackend");
    let mut receipt = None;
    terminal
        .draw(|frame| {
            receipt =
                crate::tui::screens::usage::render_at_with_pointer_receipt(frame, area, state, NOW);
        })
        .expect("actual Usage frame paint");
    let receipt = receipt.expect("visible complete Usage paint has a receipt");
    state
        .usage
        .publish_painted_pointer(area, Some(receipt.clone()));
    (terminal.backend().buffer().clone(), receipt)
}

fn body_text(buffer: &Buffer, area: Rect) -> String {
    (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn body_row(buffer: &Buffer, area: Rect, y: u16) -> String {
    (area.x..area.x.saturating_add(area.width))
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn row_y(buffer: &Buffer, area: Rect, needle: &str) -> u16 {
    (area.y..area.y.saturating_add(area.height))
        .find(|&y| body_row(buffer, area, y).contains(needle))
        .unwrap_or_else(|| panic!("painted row {needle:?} missing from {area:?}"))
}

fn assert_independent_tail_bound(
    full: &Buffer,
    full_body: Rect,
    visible_body: Rect,
    receipt_bounds: (u16, u16),
) -> u16 {
    let tail_y = row_y(full, full_body, "TAIL-END");
    let last_content_y = (full_body.y..full_body.y.saturating_add(full_body.height))
        .rev()
        .find(|&y| !body_row(full, full_body, y).trim().is_empty())
        .expect("independent full-height paint contains content");
    let full_body_end = full_body.y.saturating_add(full_body.height);
    assert_eq!(
        tail_y, last_content_y,
        "TAIL-END is the final content marker"
    );
    assert!(
        tail_y.saturating_add(1) < full_body_end,
        "the independent full paint has room below its complete tail"
    );
    assert!(
        (tail_y.saturating_add(1)..full_body_end)
            .all(|y| body_row(full, full_body, y).trim().is_empty()),
        "the independent full paint is not clipped at its canvas edge"
    );
    let rendered_rows = usize::from(tail_y.saturating_sub(full_body.y)) + 1;
    let expected_max = rendered_rows
        .saturating_sub(usize::from(visible_body.height))
        .min(usize::from(u16::MAX)) as u16;
    assert_eq!(receipt_bounds, (expected_max, expected_max));
    expected_max
}

fn list_inert_points(buffer: &Buffer, area: Rect) -> [(u16, u16); 3] {
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    let header_y = row_y(buffer, inner, "s:sort");
    let provider_y = row_y(buffer, inner, "Provider");
    let spacer_y = (inner.y..inner.y.saturating_add(inner.height))
        .find(|&y| body_row(buffer, inner, y).trim().is_empty())
        .expect("actual painted list spacer");
    [
        (inner.x, header_y),
        (inner.x, provider_y),
        (inner.x, spacer_y),
    ]
}

fn independent_full_detail(
    screen: &crate::tui::screens::usage::UsageScreenState,
    detail_outer: Rect,
) -> (Buffer, Rect) {
    let mut full_screen = screen.clone();
    full_screen.scroll = 0;
    let manager = manager_with_usage(full_screen);
    let height: u16 = 512;
    let outer = Rect::new(
        detail_outer.x,
        detail_outer.y,
        detail_outer.width,
        height.saturating_sub(detail_outer.y),
    );
    let backend = TestBackend::new(outer.right(), height);
    let mut terminal = Terminal::new(backend).expect("independent full detail TestBackend");
    terminal
        .draw(|frame| {
            crate::tui::screens::usage::render_detail(frame, outer, &manager, NOW);
        })
        .expect("independent full detail paint");
    let inner = crate::tui::screens::usage::panel("Account").inner(outer);
    let status_height = inner.height.min(1);
    let body = Rect::new(
        inner.x,
        inner.y.saturating_add(status_height),
        inner.width,
        inner.height.saturating_sub(status_height),
    );
    (terminal.backend().buffer().clone(), body)
}

fn dispatch_with_modifiers(
    state: &mut ManagerState<'_>,
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    x: u16,
    y: u16,
    area: Rect,
) {
    let event = MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers,
    };
    let _outcome = handle_mouse_with_config(state, event, area, None);
}

fn assert_nonvertical_wheel_inputs_preserve_state(
    state: &mut ManagerState<'_>,
    area: Rect,
    points: &[(u16, u16)],
) {
    let events = [
        (MouseEventKind::ScrollUp, KeyModifiers::SHIFT),
        (MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
        (MouseEventKind::ScrollLeft, KeyModifiers::NONE),
        (MouseEventKind::ScrollRight, KeyModifiers::NONE),
    ];
    for &(x, y) in points {
        for (kind, modifiers) in events {
            let (_, _receipt) = paint_with_receipt(state, area);
            let before_usage = state.usage.screen.clone();
            let before_refresh = state
                .usage
                .screen
                .as_ref()
                .is_some_and(|screen| screen.refresh_rx.is_some());
            let before_visible = state.usage.visible;
            let before_background = case_09::background_snapshot(state);
            let up_before = state.usage.painted_wheel_action(
                area,
                x,
                y,
                crate::tui::screens::usage::WheelDirection::Up,
            );
            let down_before = state.usage.painted_wheel_action(
                area,
                x,
                y,
                crate::tui::screens::usage::WheelDirection::Down,
            );
            let target_before = state.usage.painted_row_target(area, x, y);
            dispatch_with_modifiers(state, kind, modifiers, x, y, area);
            assert_eq!(
                state.usage.screen, before_usage,
                "{kind:?} with {modifiers:?} at ({x},{y}) is not a vertical Usage action"
            );
            assert_eq!(
                state
                    .usage
                    .screen
                    .as_ref()
                    .is_some_and(|screen| screen.refresh_rx.is_some()),
                before_refresh
            );
            assert_eq!(state.usage.visible, before_visible);
            assert_eq!(
                case_09::background_snapshot(state),
                before_background,
                "hidden Manager navigation/focus/scroll/picker state is unchanged"
            );
            assert_eq!(
                state.usage.painted_wheel_action(
                    area,
                    x,
                    y,
                    crate::tui::screens::usage::WheelDirection::Up,
                ),
                up_before
            );
            assert_eq!(
                state.usage.painted_wheel_action(
                    area,
                    x,
                    y,
                    crate::tui::screens::usage::WheelDirection::Down,
                ),
                down_before
            );
            assert_eq!(state.usage.painted_row_target(area, x, y), target_before);
            assert!(state.drain_effects().is_empty());
        }
    }
}

fn assert_vertical_wheel_controls(
    state: &mut ManagerState<'_>,
    area: Rect,
    list_point: (u16, u16),
    detail_point: (u16, u16),
) {
    let (_, receipt) = paint_with_receipt(state, area);
    assert_eq!(
        receipt.wheel_action(
            state.usage.screen.as_ref().unwrap(),
            area,
            list_point.0,
            list_point.1,
            crate::tui::screens::usage::WheelDirection::Down,
        ),
        Some(crate::tui::screens::usage::UsageWheelAction::MoveSelection(
            1
        ))
    );
    dispatch_with_modifiers(
        state,
        MouseEventKind::ScrollDown,
        KeyModifiers::NONE,
        list_point.0,
        list_point.1,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().selected, 3);
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, 4);
    assert_eq!(state.usage.screen.as_ref().unwrap().list_scroll, 0);

    let (_, receipt) = paint_with_receipt(state, area);
    assert_eq!(
        receipt.wheel_action(
            state.usage.screen.as_ref().unwrap(),
            area,
            list_point.0,
            list_point.1,
            crate::tui::screens::usage::WheelDirection::Up,
        ),
        Some(crate::tui::screens::usage::UsageWheelAction::MoveSelection(
            -1
        ))
    );
    dispatch_with_modifiers(
        state,
        MouseEventKind::ScrollUp,
        KeyModifiers::NONE,
        list_point.0,
        list_point.1,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().selected, 2);

    state.usage.screen.as_mut().unwrap().scroll = 0;
    let (_, receipt) = paint_with_receipt(state, area);
    assert_eq!(
        receipt.wheel_action(
            state.usage.screen.as_ref().unwrap(),
            area,
            detail_point.0,
            detail_point.1,
            crate::tui::screens::usage::WheelDirection::Down,
        ),
        Some(crate::tui::screens::usage::UsageWheelAction::ScrollDetail(
            3
        ))
    );
    dispatch_with_modifiers(
        state,
        MouseEventKind::ScrollDown,
        KeyModifiers::NONE,
        detail_point.0,
        detail_point.1,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, 3);

    let (_, receipt) = paint_with_receipt(state, area);
    assert_eq!(
        receipt.wheel_action(
            state.usage.screen.as_ref().unwrap(),
            area,
            detail_point.0,
            detail_point.1,
            crate::tui::screens::usage::WheelDirection::Up,
        ),
        Some(crate::tui::screens::usage::UsageWheelAction::ScrollDetail(
            -3
        ))
    );
    dispatch_with_modifiers(
        state,
        MouseEventKind::ScrollUp,
        KeyModifiers::NONE,
        detail_point.0,
        detail_point.1,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, 0);
}

#[test]
fn t04_detail_wheel_bounds_preserve_painted_list_and_row_identity() {
    let area = Rect::new(0, 0, 120, 40);
    let mut screen = long_usage_screen();
    screen.list_scroll = 1;
    let mut state = manager_with_usage(screen);
    let (first, receipt) = paint_with_receipt(&mut state, area);
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    let list_before = body_text(&first, layout.list_inner);
    let list_scroll_before = state.usage.screen.as_ref().unwrap().list_scroll;
    assert_ne!(list_scroll_before, 0);
    let (painted, maximum) = receipt.detail_scroll_bounds();
    let screen = state.usage.screen.as_ref().unwrap();
    let (full, full_body) = independent_full_detail(screen, layout.detail_outer);
    let independently_measured_max = assert_independent_tail_bound(
        &full,
        full_body,
        layout.detail_body,
        receipt.detail_scroll_bounds(),
    );
    assert_eq!(painted, independently_measured_max);
    assert_eq!(maximum, independently_measured_max);
    assert!(maximum > 3);
    assert!(body_text(&first, layout.detail_body).contains("TAIL-END"));
    let gamma_initial_y = row_y(&first, layout.list_inner, "Gamma account");
    let gamma_initial_target = state
        .usage
        .painted_row_target(area, layout.list_inner.x, gamma_initial_y)
        .expect("nonzero-scrolled painted Gamma row has a receipt-backed target");
    assert_eq!(
        gamma_initial_target,
        crate::tui::screens::usage::UsageRowTarget::Account("provider:c".to_owned())
    );

    for row in 0..layout.detail_body.height {
        assert_eq!(
            body_row(&first, layout.detail_body, layout.detail_body.y + row),
            body_row(
                &full,
                full_body,
                full_body.y + independently_measured_max + row,
            ),
            "raw MAX paints the independently verified tail row {row}"
        );
    }

    for attempt in 0..3 {
        if attempt > 0 {
            let (_, fresh_receipt) = paint_with_receipt(&mut state, area);
            assert_eq!(fresh_receipt.detail_scroll_bounds(), (maximum, maximum));
        }
        dispatch_with_modifiers(
            &mut state,
            MouseEventKind::ScrollDown,
            KeyModifiers::NONE,
            layout.detail_body.x,
            layout.detail_body.y,
            area,
        );
        assert_eq!(
            state.usage.screen.as_ref().unwrap().scroll,
            independently_measured_max,
            "repeated fresh-paint wheel-down events remain at the measured bound"
        );
        assert_eq!(
            state.usage.screen.as_ref().unwrap().list_scroll,
            list_scroll_before
        );
    }

    let (repainted, _) = paint_with_receipt(&mut state, area);
    assert_eq!(body_text(&repainted, layout.list_inner), list_before);
    dispatch_with_modifiers(
        &mut state,
        MouseEventKind::ScrollUp,
        KeyModifiers::NONE,
        layout.detail_body.x,
        layout.detail_body.y,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().scroll,
        independently_measured_max.saturating_sub(3)
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().list_scroll,
        list_scroll_before
    );

    let (repainted, _) = paint_with_receipt(&mut state, area);
    assert_eq!(body_text(&repainted, layout.list_inner), list_before);
    let gamma_y = row_y(&repainted, layout.list_inner, "Gamma account");
    let gamma_target_after_wheel = state
        .usage
        .painted_row_target(area, layout.list_inner.x, gamma_y)
        .expect("same painted Gamma row retains a receipt-backed target");
    assert_eq!(gamma_target_after_wheel, gamma_initial_target);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: layout.list_inner.x,
        row: gamma_y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(clickable_at(&state, click, area, None));
    let _outcome = handle_mouse_with_config(&mut state, click, area, None);
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some("provider:c")
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().list_scroll,
        list_scroll_before
    );
}

#[test]
fn t04_shifted_and_native_horizontal_wheel_leave_usage_vertical_state_unchanged() {
    let area = Rect::new(7, 5, 120, 40);
    let mut screen = long_usage_screen();
    screen.scroll = 4;
    screen.list_scroll = 0;
    screen.selected = 2;
    screen.selected_id = Some("provider:b".to_owned());
    let mut state = manager_with_usage(screen);
    state.selected = 2;
    state.list_split_pct = 42;
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_names_scroll, 3);
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_mounts_scroll, 5);
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_mounts_scroll, 6);
    let layout = crate::tui::screens::usage::usage_frame_layout(area);

    let (initial_buffer, _) = paint_with_receipt(&mut state, area);
    let list_row_y = row_y(&initial_buffer, layout.list_inner, "Beta account");
    let detail_point = (layout.detail_body.x, layout.detail_body.y);
    let mut points = list_inert_points(&initial_buffer, area).to_vec();
    points.push((layout.list_inner.x, list_row_y));
    points.push(detail_point);
    points.push((layout.detail_status.x, layout.detail_status.y));
    points.push((layout.list_outer.x, layout.list_outer.y));
    let beta_y = row_y(&initial_buffer, layout.list_inner, "Beta account");
    assert_nonvertical_wheel_inputs_preserve_state(&mut state, area, &points);
    assert_vertical_wheel_controls(
        &mut state,
        area,
        (layout.list_inner.x, beta_y),
        detail_point,
    );
}
