// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::model::ConsolePaintedInputOwner;
use jackin_protocol::control::{
    MeterPercentPoints, PercentObservationOrigin, RawPercentPoints, UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1, UsageQuotaStateV1,
    UsageWindowCategoryV1,
};
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
        lifecycle: UsageLifecycleV1::Available,
        freshness_phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(NOW),
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

fn usage_manager() -> ManagerState<'static> {
    let mut state = list_state();
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::screens::usage::UsageScreenState {
        accounts: vec![
            usage_account("a", "Alpha 界 account with a long row"),
            usage_account("b", "Beta account"),
            usage_account("c", "Gamma account"),
        ],
        selected: 1,
        selected_id: Some("provider:a".to_owned()),
        ..Default::default()
    });
    state
}

fn rich_usage_manager() -> ManagerState<'static> {
    let mut state = usage_manager();
    let screen = state.usage.screen.as_mut().expect("Usage screen");
    screen.accounts = vec![
        metered_account(
            "a",
            "Alpha 界e\u{301} account with a deliberately long label",
            90.0,
            false,
        ),
        metered_account("b", "Beta account", 50.0, true),
        metered_account("c", "Gamma account", 10.0, true),
    ];
    screen.selected = 2;
    screen.selected_id = Some("provider:b".to_owned());
    state
}

fn metered_account(
    id: &str,
    label: &str,
    remaining: f64,
    stale: bool,
) -> crate::tui::screens::usage::UsageAccount {
    let mut account = usage_account(id, label);
    account.is_stale = stale;
    account.freshness_phase = if stale {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    let raw = RawPercentPoints::new(remaining).expect("finite remaining percentage");
    let meter = MeterPercentPoints::new(remaining).expect("valid meter percentage");
    account.windows = vec![crate::tui::screens::usage::UsageWindow {
        window_id: format!("{id}-weekly"),
        rank: 0,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly allowance".to_owned(),
        value: format!("summary-{id}"),
        reset: "resets soon".to_owned(),
        remaining_percent: Some(raw),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: None,
        used_percent_origin: None,
        meter_percent: Some(meter),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: Some(NOW),
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
    }];
    account
}

fn paint_usage(state: &mut ManagerState<'_>, area: Rect) -> Buffer {
    let backend = TestBackend::new(area.right(), area.bottom());
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut receipt = None;
    terminal
        .draw(|frame| {
            receipt =
                crate::tui::screens::usage::render_at_with_pointer_receipt(frame, area, state, NOW);
        })
        .expect("actual TestBackend paint");
    state.usage.publish_painted_pointer(area, receipt);
    terminal.backend().buffer().clone()
}

fn row_y(buffer: &Buffer, needle: &str, area: Rect) -> u16 {
    row_y_option(buffer, needle, area).unwrap_or_else(|| panic!("painted row {needle:?} not found"))
}

fn row_y_option(buffer: &Buffer, needle: &str, area: Rect) -> Option<u16> {
    let list_inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    (list_inner.y..list_inner.y + list_inner.height).find(|&y| {
        (list_inner.x..list_inner.x + list_inner.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .contains(needle)
    })
}

fn row_text(buffer: &Buffer, area: Rect, y: u16) -> String {
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    (inner.x..inner.x + inner.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn meter_row(buffer: &Buffer, area: Rect, token: &str) -> u16 {
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    let summary = row_y(buffer, &format!("summary-{token}"), area);
    let next_label = match token {
        "a" => row_y(buffer, "Beta account", area),
        "b" => row_y(buffer, "Gamma account", area),
        _ => inner.y + inner.height,
    };
    (summary + 1..next_label)
        .find(|&y| {
            let text = row_text(buffer, area, y);
            text.contains('█') || text.contains('░')
        })
        .unwrap_or_else(|| panic!("painted meter row for {token} not found"))
}

fn rect_text(buffer: &Buffer, area: Rect) -> String {
    (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct UsageSnapshot {
    screen: Option<crate::tui::screens::usage::UsageScreenState>,
    refresh_rx_present: bool,
    visible: bool,
}

fn usage_snapshot(state: &ManagerState<'_>) -> UsageSnapshot {
    UsageSnapshot {
        screen: state.usage.screen.clone(),
        refresh_rx_present: state
            .usage
            .screen
            .as_ref()
            .is_some_and(|screen| screen.refresh_rx.is_some()),
        visible: state.usage.visible,
    }
}

fn assert_usage_unchanged(state: &ManagerState<'_>, expected: &UsageSnapshot) {
    let actual_rx = state
        .usage
        .screen
        .as_ref()
        .is_some_and(|screen| screen.refresh_rx.is_some());
    assert_eq!(state.usage.screen, expected.screen);
    assert_eq!(actual_rx, expected.refresh_rx_present);
    assert_eq!(state.usage.visible, expected.visible);
}

fn assert_row_clicks(
    state: &mut ManagerState<'_>,
    area: Rect,
    needle: &str,
    stable_id: Option<&str>,
    selected: usize,
) {
    for use_right_edge in [false, true] {
        let buffer = paint_usage(state, area);
        let layout = crate::tui::screens::usage::usage_frame_layout(area);
        let y = row_y(&buffer, needle, area);
        let x = if use_right_edge {
            layout.list_inner.x + layout.list_inner.width - 1
        } else {
            layout.list_inner.x
        };
        let mouse = mouse_at_kind(MouseEventKind::Down(MouseButton::Left), x, y);
        assert!(
            clickable_at(state, mouse, area, None),
            "painted row {needle:?}"
        );
        dispatch(state, mouse.kind, x, y, area);
        let screen = state.usage.screen.as_ref().unwrap();
        assert_eq!(screen.selected, selected, "row {needle:?}");
        assert_eq!(screen.selected_id.as_deref(), stable_id, "row {needle:?}");
    }
}

fn assert_meter_clicks(
    state: &mut ManagerState<'_>,
    area: Rect,
    token: &str,
    stable_id: &str,
    selected: usize,
) {
    for use_right_edge in [false, true] {
        let buffer = paint_usage(state, area);
        let layout = crate::tui::screens::usage::usage_frame_layout(area);
        let y = meter_row(&buffer, area, token);
        let x = if use_right_edge {
            layout.list_inner.x + layout.list_inner.width - 1
        } else {
            layout.list_inner.x
        };
        assert!(clickable_at(
            state,
            mouse_at_kind(MouseEventKind::Down(MouseButton::Left), x, y),
            area,
            None,
        ));
        dispatch(state, MouseEventKind::Down(MouseButton::Left), x, y, area);
        let screen = state.usage.screen.as_ref().unwrap();
        assert_eq!(screen.selected, selected);
        assert_eq!(screen.selected_id.as_deref(), Some(stable_id));
    }
}

fn assert_mutation_repaints_before_dispatch(
    area: Rect,
    change: impl FnOnce(&mut crate::tui::screens::usage::UsageScreenState),
    token: &str,
    expected_id: &str,
) {
    let mut state = rich_usage_manager();
    let first = paint_usage(&mut state, area);
    let old_y = row_y(&first, "Alpha", area);
    change(state.usage.screen.as_mut().unwrap());
    let changed = usage_snapshot(&state);
    let old_click = mouse_at_kind(
        MouseEventKind::Down(MouseButton::Left),
        crate::tui::screens::usage::usage_frame_layout(area)
            .list_inner
            .x,
        old_y,
    );
    assert!(
        !clickable_at(&state, old_click, area, None),
        "old paint is revoked after direct mutation"
    );
    dispatch(
        &mut state,
        old_click.kind,
        old_click.column,
        old_click.row,
        area,
    );
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        old_click.column,
        old_click.row,
        area,
    );
    assert_usage_unchanged(&state, &changed);

    let fresh = paint_usage(&mut state, area);
    let y = row_y(&fresh, token, area);
    let x = crate::tui::screens::usage::usage_frame_layout(area)
        .list_inner
        .x;
    dispatch(
        &mut state,
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some(expected_id)
    );
}

fn mouse_at_kind(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

fn dispatch(state: &mut ManagerState<'_>, kind: MouseEventKind, x: u16, y: u16, area: Rect) {
    let _outcome = handle_mouse_with_config(state, mouse_at_kind(kind, x, y), area, None);
}

fn background_snapshot(state: &ManagerState<'_>) -> impl std::fmt::Debug + PartialEq + use<> {
    (
        state.selected,
        state.list_split_pct,
        state.list_focus_owner.clone(),
        state.drag_state,
        (
            state.list_names_scroll.offset_x(),
            state.list_names_scroll.offset_y(),
        ),
        (
            state.list_mounts_scroll.offset_x(),
            state.list_mounts_scroll.offset_y(),
        ),
        state.hover_target,
        (
            state.inline_role_picker.is_some(),
            state.inline_agent_picker.is_some(),
            state.inline_new_session_picker.is_some(),
            state.inline_account_picker.is_some(),
            state.launch_account_picker.is_some(),
        ),
    )
}

fn painted_list_nonrow_points(buffer: &Buffer, area: Rect) -> [(u16, u16); 3] {
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    let header_y = row_y(buffer, "s:sort", area);
    let provider_y = row_y(buffer, "Provider", area);
    let spacer_y = (inner.y..inner.y + inner.height)
        .find(|&y| row_text(buffer, area, y).trim().is_empty())
        .expect("actual painted blank spacer row");
    [
        (inner.x, header_y),
        (inner.x, provider_y),
        (inner.x, spacer_y),
    ]
}

fn assert_list_nonrows_are_click_inert_but_wheelable(
    state: &mut ManagerState<'_>,
    area: Rect,
    points: [(u16, u16); 3],
) {
    for (x, y) in points {
        let usage_before = usage_snapshot(state);
        let background_before = background_snapshot(state);
        paint_usage(state, area);
        let click = mouse_at_kind(MouseEventKind::Down(MouseButton::Left), x, y);
        assert!(!clickable_at(state, click, area, None));
        dispatch(state, click.kind, x, y, area);
        assert_usage_unchanged(state, &usage_before);
        assert_eq!(background_snapshot(state), background_before);
        assert!(state.drain_effects().is_empty());

        paint_usage(state, area);
        dispatch(state, MouseEventKind::Moved, x, y, area);
        assert_usage_unchanged(state, &usage_before);
        assert_eq!(background_snapshot(state), background_before);
        assert!(state.drain_effects().is_empty());

        let screen = state.usage.screen.as_mut().expect("Usage screen");
        screen.selected = 1;
        screen.selected_id = Some("provider:a".to_owned());
        paint_usage(state, area);
        dispatch(state, MouseEventKind::ScrollUp, x, y, area);
        assert_eq!(state.usage.screen.as_ref().unwrap().selected, 0);
        assert_eq!(state.usage.screen.as_ref().unwrap().selected_id, None);
        paint_usage(state, area);
        dispatch(state, MouseEventKind::ScrollDown, x, y, area);
        assert_eq!(state.usage.screen.as_ref().unwrap().selected, 1);
        assert_eq!(
            state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
            Some("provider:a")
        );
        assert!(state.drain_effects().is_empty());
    }
}

fn assert_tiny_and_zero_area_receipts_recover() {
    let one_status_area = (1..=14)
        .map(|height| Rect::new(7, 4, 40, height))
        .find(|area| {
            let layout = crate::tui::screens::usage::usage_frame_layout(*area);
            layout.detail_status.height == 1 && layout.detail_body.height == 0
        })
        .expect("find a tiny area with one status row and no detail body");
    let mut one_status = rich_usage_manager();
    one_status.usage.screen.as_mut().unwrap().detail = true;
    let one_status_before = usage_snapshot(&one_status);
    paint_usage(&mut one_status, one_status_area);
    let one_status_layout = crate::tui::screens::usage::usage_frame_layout(one_status_area);
    assert_eq!(one_status_layout.detail_status.height, 1);
    assert_eq!(one_status_layout.detail_body.height, 0);
    let status_point = (
        one_status_layout.detail_status.x,
        one_status_layout.detail_status.y,
    );
    assert!(!clickable_at(
        &one_status,
        mouse_at_kind(
            MouseEventKind::Down(MouseButton::Left),
            status_point.0,
            status_point.1
        ),
        one_status_area,
        None,
    ));
    dispatch(
        &mut one_status,
        MouseEventKind::ScrollUp,
        status_point.0,
        status_point.1,
        one_status_area,
    );
    assert_usage_unchanged(&one_status, &one_status_before);

    let mut recovering = rich_usage_manager();
    for empty_area in [Rect::new(7, 4, 0, 7), Rect::new(7, 4, 40, 0)] {
        let before = usage_snapshot(&recovering);
        paint_usage(&mut recovering, empty_area);
        let probe = mouse_at_kind(
            MouseEventKind::Down(MouseButton::Left),
            empty_area.x,
            empty_area.y,
        );
        assert!(!clickable_at(&recovering, probe, empty_area, None));
        dispatch(
            &mut recovering,
            MouseEventKind::ScrollDown,
            empty_area.x,
            empty_area.y,
            empty_area,
        );
        assert_usage_unchanged(&recovering, &before);
    }
    let recovered_area = term_120x40();
    let recovered = paint_usage(&mut recovering, recovered_area);
    let recovered_y = row_y(&recovered, "Alpha", recovered_area);
    assert!(clickable_at(
        &recovering,
        mouse_at_kind(
            MouseEventKind::Down(MouseButton::Left),
            crate::tui::screens::usage::usage_frame_layout(recovered_area)
                .list_inner
                .x,
            recovered_y,
        ),
        recovered_area,
        None,
    ));
}

#[test]
fn t02_inactive_usage_guards_click_hover_and_wheel_dispatch() {
    let area = term_120x40();
    let mut state = usage_manager();
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_names_scroll, 4);
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 5);
    state.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: state.list_split_pct,
        anchor_x: 40,
    });
    state.usage.screen.as_mut().unwrap().refresh_rx = Some(Box::new(
        crate::tui::runtime::ready_blocking_subscription((
            7,
            Ok(crate::tui::screens::usage::UsageScreenState::default()),
        )),
    ));
    let buffer = paint_usage(&mut state, area);
    let y = row_y(&buffer, "Alpha", area);
    assert!(row_text(&buffer, area, y).contains('界'));
    let selected_before = state.usage.screen.as_ref().unwrap().selected;
    let workspace_before = state.selected;
    let split_before = state.list_split_pct;
    let focus_before = state.list_focus_owner.clone();
    assert!(state.drag_state.is_some());
    let list_names_scroll_before = (
        state.list_names_scroll.offset_x(),
        state.list_names_scroll.offset_y(),
    );
    let usage_before = usage_snapshot(&state);
    dispatch(&mut state, MouseEventKind::Moved, 20, y, area);
    dispatch(
        &mut state,
        MouseEventKind::Down(MouseButton::Right),
        20,
        y,
        area,
    );
    dispatch(
        &mut state,
        MouseEventKind::Drag(MouseButton::Left),
        20,
        y,
        area,
    );
    dispatch(&mut state, MouseEventKind::ScrollLeft, 20, y, area);
    dispatch(&mut state, MouseEventKind::ScrollRight, 20, y, area);
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected,
        selected_before
    );
    assert_eq!(state.selected, workspace_before);
    assert_eq!(state.list_split_pct, split_before);
    assert_eq!(state.list_focus_owner, focus_before);
    assert!(state.drag_state.is_none());
    assert_eq!(
        (
            state.list_names_scroll.offset_x(),
            state.list_names_scroll.offset_y(),
        ),
        list_names_scroll_before
    );
    assert!(state.hover_target.is_none());
    assert_usage_unchanged(&state, &usage_before);

    let mut absent = usage_manager();
    absent.usage.screen = None;
    let absent_before = usage_snapshot(&absent);
    let absent_selected_before = absent.selected;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ] {
        assert!(!clickable_at(
            &absent,
            mouse_at_kind(kind, 20, y),
            area,
            None
        ));
        dispatch(&mut absent, kind, 20, y, area);
    }
    assert_usage_unchanged(&absent, &absent_before);
    assert_eq!(absent.selected, absent_selected_before);
    assert!(absent.drain_effects().is_empty());

    let mut hidden = usage_manager();
    hidden.usage.screen.as_mut().unwrap().refresh_rx = Some(Box::new(
        crate::tui::runtime::ready_blocking_subscription((
            8,
            Ok(crate::tui::screens::usage::UsageScreenState::default()),
        )),
    ));
    paint_usage(&mut hidden, area);
    let detail = crate::tui::screens::usage::usage_frame_layout(area).detail_body;
    let x = detail.x + 1;
    let y = detail.y;
    hidden.usage.visible = false;
    let after_hide = usage_snapshot(&hidden);
    let hidden_selected_before = hidden.selected;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ] {
        assert!(!clickable_at(
            &hidden,
            mouse_at_kind(kind, x, y),
            area,
            None
        ));
        dispatch(&mut hidden, kind, x, y, area);
    }
    assert_usage_unchanged(&hidden, &after_hide);
    assert_eq!(hidden.selected, hidden_selected_before);
    assert!(hidden.drain_effects().is_empty());
}

fn assert_reserved_usage_regions_are_inert(area: Rect) {
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    let mut inert = rich_usage_manager();
    let before = usage_snapshot(&inert);
    let points = [
        (layout.list_outer.x, layout.list_outer.y),
        (layout.detail_outer.x, layout.detail_outer.y),
        (layout.detail_status.x, layout.detail_status.y),
        (area.x + 1, area.y),
        (area.x + 1, area.bottom() - 1),
        (area.right(), area.bottom() - 1),
    ];
    for (x, y) in points {
        paint_usage(&mut inert, area);
        let click = mouse_at_kind(MouseEventKind::Down(MouseButton::Left), x, y);
        assert!(!clickable_at(&inert, click, area, None));
        dispatch(&mut inert, click.kind, x, y, area);
        paint_usage(&mut inert, area);
        dispatch(&mut inert, MouseEventKind::Moved, x, y, area);
        dispatch(&mut inert, MouseEventKind::ScrollDown, x, y, area);
        assert_usage_unchanged(&inert, &before);
    }

    let mut overscrolled = rich_usage_manager();
    overscrolled.usage.screen.as_mut().unwrap().scroll = u16::MAX;
    let _overscroll_buffer = paint_usage(&mut overscrolled, area);
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    for y in inner.y..inner.y + inner.height {
        for x in inner.x..inner.x + inner.width {
            assert!(!clickable_at(
                &overscrolled,
                mouse_at_kind(MouseEventKind::Down(MouseButton::Left), x, y),
                area,
                None
            ));
        }
    }

    for filtered_empty in [false, true] {
        let mut empty = rich_usage_manager();
        let screen = empty.usage.screen.as_mut().unwrap();
        if filtered_empty {
            screen.filter = crate::tui::screens::usage::UsageFilter::Issues;
        } else {
            screen.accounts.clear();
            screen.selected = 0;
            screen.selected_id = None;
        }
        let empty_before = usage_snapshot(&empty);
        let empty_buffer = paint_usage(&mut empty, area);
        let list = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
        if filtered_empty {
            let normalized = rect_text(&empty_buffer, list)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(normalized.contains("No accounts match filter 'issues'."));
        }
        assert!(!clickable_at(
            &empty,
            mouse_at_kind(MouseEventKind::Down(MouseButton::Left), list.x, list.y),
            area,
            None
        ));
        dispatch(&mut empty, MouseEventKind::ScrollUp, list.x, list.y, area);
        assert_usage_unchanged(&empty, &empty_before);
    }

    let tiny_area = Rect::new(7, 4, 40, 8);
    let mut tiny = rich_usage_manager();
    let tiny_before = usage_snapshot(&tiny);
    paint_usage(&mut tiny, tiny_area);
    let tiny_layout = crate::tui::screens::usage::usage_frame_layout(tiny_area);
    assert!(!clickable_at(
        &tiny,
        mouse_at_kind(
            MouseEventKind::Down(MouseButton::Left),
            tiny_layout.detail_status.x,
            tiny_layout.detail_status.y
        ),
        tiny_area,
        None
    ));
    dispatch(
        &mut tiny,
        MouseEventKind::ScrollUp,
        tiny_layout.detail_status.x,
        tiny_layout.detail_status.y,
        tiny_area,
    );
    assert_usage_unchanged(&tiny, &tiny_before);
    assert_tiny_and_zero_area_receipts_recover();
}

fn assert_direct_quota_change_repaints_before_dispatch(area: Rect) {
    let mut quota_changed = rich_usage_manager();
    let old_buffer = paint_usage(&mut quota_changed, area);
    let old_alpha_y = row_y(&old_buffer, "Alpha", area);
    {
        let screen = quota_changed.usage.screen.as_mut().unwrap();
        screen.sort = crate::tui::screens::usage::UsageSort::Remaining;
        screen.accounts[1].windows[0].remaining_percent =
            Some(RawPercentPoints::new(5.0).expect("valid changed quota"));
        screen.accounts[1].windows[0].meter_percent =
            Some(MeterPercentPoints::new(5.0).expect("valid changed meter"));
        screen.accounts[2].windows[0].remaining_percent =
            Some(RawPercentPoints::new(50.0).expect("valid changed quota"));
        screen.accounts[2].windows[0].meter_percent =
            Some(MeterPercentPoints::new(50.0).expect("valid changed meter"));
    }
    let changed = usage_snapshot(&quota_changed);
    let stale_click = mouse_at_kind(
        MouseEventKind::Down(MouseButton::Left),
        crate::tui::screens::usage::usage_frame_layout(area)
            .list_inner
            .x,
        old_alpha_y,
    );
    assert!(!clickable_at(&quota_changed, stale_click, area, None));
    dispatch(
        &mut quota_changed,
        stale_click.kind,
        stale_click.column,
        stale_click.row,
        area,
    );
    assert_usage_unchanged(&quota_changed, &changed);
    let sorted = paint_usage(&mut quota_changed, area);
    let beta_y = row_y(&sorted, "Beta account", area);
    let gamma_y = row_y(&sorted, "Gamma account", area);
    let alpha_y = row_y(&sorted, "Alpha", area);
    assert!(
        beta_y < gamma_y && gamma_y < alpha_y,
        "painted rows reflect direct quota changes and Remaining sort"
    );
    let alpha_click = mouse_at_kind(
        MouseEventKind::Down(MouseButton::Left),
        crate::tui::screens::usage::usage_frame_layout(area)
            .list_inner
            .x,
        alpha_y,
    );
    dispatch(
        &mut quota_changed,
        alpha_click.kind,
        alpha_click.column,
        alpha_click.row,
        area,
    );
    assert_eq!(
        quota_changed
            .usage
            .screen
            .as_ref()
            .unwrap()
            .selected_id
            .as_deref(),
        Some("provider:a")
    );
}

#[test]
fn t03_published_rows_dispatch_stable_ids_and_detail_end_to_end() {
    let area = term_120x40();
    let mut state = rich_usage_manager();
    let stale_buffer = paint_usage(&mut state, area);
    let stale_y = row_y(&stale_buffer, "Gamma", area);
    let stale_click = mouse_at_kind(
        MouseEventKind::Down(MouseButton::Left),
        crate::tui::screens::usage::usage_frame_layout(area)
            .list_inner
            .x,
        stale_y,
    );
    assert!(clickable_at(&state, stale_click, area, None));
    dispatch(
        &mut state,
        stale_click.kind,
        stale_click.column,
        stale_click.row,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some("provider:c")
    );
    assert!(
        !clickable_at(&state, stale_click, area, None),
        "a second event cannot reuse the prior paint receipt"
    );
    dispatch(
        &mut state,
        stale_click.kind,
        stale_click.column,
        stale_click.row,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some("provider:c")
    );

    assert_row_clicks(&mut state, area, "Overview", None, 0);
    for (token, label, ordinal) in [("a", "Alpha", 1), ("b", "Beta", 2), ("c", "Gamma", 3)] {
        let stable_id = format!("provider:{token}");
        assert_row_clicks(&mut state, area, label, Some(&stable_id), ordinal);
        assert_row_clicks(
            &mut state,
            area,
            &format!("summary-{token}"),
            Some(&stable_id),
            ordinal,
        );
        assert_meter_clicks(&mut state, area, token, &stable_id, ordinal);
    }

    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some("provider:c")
    );
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    crate::tui::screens::usage::handle_key(&mut state, key);
    assert!(state.usage.screen.as_ref().unwrap().detail);
    let detail_buffer = paint_usage(&mut state, area);
    let detail_body = crate::tui::screens::usage::usage_frame_layout(area).detail_body;
    assert!(rect_text(&detail_buffer, detail_body).contains("Account   Gamma account"));

    let before = state.usage.screen.as_ref().unwrap().scroll;
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        layout.detail_body.x,
        layout.detail_body.y,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().scroll,
        before.saturating_add(3)
    );
    assert!(state.drain_effects().is_empty());
}

#[test]
fn t03_published_unicode_scroll_resize_rows_match_actual_buffer() {
    let mut state = rich_usage_manager();
    let first_area = Rect::new(7, 4, 120, 36);
    let first_buffer = paint_usage(&mut state, first_area);
    let first_layout = crate::tui::screens::usage::usage_frame_layout(first_area);
    let first_y = row_y(&first_buffer, "Alpha", first_area);
    let before_resize = usage_snapshot(&state);
    let resized_area = Rect::new(7, 4, 90, 28);
    assert!(!clickable_at(
        &state,
        mouse_at_kind(
            MouseEventKind::Down(MouseButton::Left),
            first_layout.list_inner.x,
            first_y
        ),
        resized_area,
        None,
    ));
    dispatch(
        &mut state,
        MouseEventKind::Down(MouseButton::Left),
        first_layout.list_inner.x,
        first_y,
        resized_area,
    );
    assert_usage_unchanged(&state, &before_resize);

    let mut seen = [false; 3];
    let mut saw_narrow_wrap = false;
    let mut saw_continuation_dispatch = false;
    let mut saw_combining_mark = false;
    for width in [40, 63, 100, 180] {
        for scroll in [0, 5, 13, 25, 40] {
            let area = Rect::new(7, 4, width, 36);
            state.usage.screen.as_mut().unwrap().scroll = scroll;
            let buffer = paint_usage(&mut state, area);
            let layout = crate::tui::screens::usage::usage_frame_layout(area);
            let visible_text = rect_text(&buffer, layout.list_inner);
            saw_combining_mark |= visible_text.contains('\u{301}');
            if width == 40 && scroll == 0 {
                let alpha_y = row_y(&buffer, "Alpha", area);
                saw_narrow_wrap =
                    !row_text(&buffer, area, alpha_y).contains("界") && visible_text.contains("界");
                if let Some(y) = row_y_option(&buffer, "界", area) {
                    let wide_x = (layout.list_inner.x
                        ..layout.list_inner.x + layout.list_inner.width)
                        .find(|&x| buffer[(x, y)].symbol() == "界")
                        .expect("actual painted wide cell");
                    let continuation_x = wide_x + 1;
                    assert!(continuation_x < layout.list_inner.x + layout.list_inner.width);
                    assert_eq!(buffer[(continuation_x, y)].symbol(), " ");
                    dispatch(
                        &mut state,
                        MouseEventKind::Down(MouseButton::Left),
                        continuation_x,
                        y,
                        area,
                    );
                    assert_eq!(
                        state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
                        Some("provider:a"),
                        "the actual CJK continuation dispatch selects its painted stable ID"
                    );
                    saw_continuation_dispatch = true;
                }
            }
            for (slot, (token, id)) in [
                ("Alpha", "provider:a"),
                ("Beta", "provider:b"),
                ("Gamma", "provider:c"),
            ]
            .into_iter()
            .enumerate()
            {
                let fresh = paint_usage(&mut state, area);
                let fresh_layout = crate::tui::screens::usage::usage_frame_layout(area);
                let Some(y) = row_y_option(&fresh, token, area) else {
                    continue;
                };
                let x = fresh_layout.list_inner.x;
                dispatch(
                    &mut state,
                    MouseEventKind::Down(MouseButton::Left),
                    x,
                    y,
                    area,
                );
                assert_eq!(
                    state.usage.screen.as_ref().unwrap().selected_id.as_deref(),
                    Some(id)
                );
                seen[slot] = true;
            }
        }
    }
    assert_eq!(
        seen, [true; 3],
        "all fixture IDs were observed in actual painted buffers"
    );
    assert!(
        saw_narrow_wrap,
        "the narrow list wraps the CJK label across painted rows"
    );
    assert!(saw_continuation_dispatch);
    assert!(
        saw_combining_mark,
        "the combining character survives actual buffer paint"
    );
}

#[test]
fn t03_published_panes_wheel_one_three_and_reserved_regions_are_inert() {
    let area = term_120x40();
    let mut state = rich_usage_manager();
    let buffer = paint_usage(&mut state, area);
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    let nonrow_points = painted_list_nonrow_points(&buffer, area);
    let blank = nonrow_points[2];
    assert_list_nonrows_are_click_inert_but_wheelable(&mut state, area, nonrow_points);
    state.usage.screen.as_mut().unwrap().selected = 1;
    state.usage.screen.as_mut().unwrap().selected_id = Some("provider:a".to_owned());
    paint_usage(&mut state, area);
    dispatch(&mut state, MouseEventKind::ScrollUp, blank.0, blank.1, area);
    assert_eq!(state.usage.screen.as_ref().unwrap().selected, 0);
    paint_usage(&mut state, area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        blank.0,
        blank.1,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().selected, 1);

    state.usage.screen.as_mut().unwrap().selected = 3;
    state.usage.screen.as_mut().unwrap().selected_id = Some("provider:c".to_owned());
    paint_usage(&mut state, area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        blank.0,
        blank.1,
        area,
    );
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected,
        3,
        "selection saturates at the last account"
    );
    state.usage.screen.as_mut().unwrap().selected = 0;
    state.usage.screen.as_mut().unwrap().selected_id = None;
    paint_usage(&mut state, area);
    dispatch(&mut state, MouseEventKind::ScrollUp, blank.0, blank.1, area);
    assert_eq!(
        state.usage.screen.as_ref().unwrap().selected,
        0,
        "selection saturates at Overview"
    );

    state.usage.screen.as_mut().unwrap().scroll = 0;
    paint_usage(&mut state, area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        layout.detail_body.x,
        layout.detail_body.y,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, 3);
    paint_usage(&mut state, area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollUp,
        layout.detail_body.x,
        layout.detail_body.y,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, 0);
    state.usage.screen.as_mut().unwrap().scroll = u16::MAX;
    paint_usage(&mut state, area);
    dispatch(
        &mut state,
        MouseEventKind::ScrollDown,
        layout.detail_body.x,
        layout.detail_body.y,
        area,
    );
    assert_eq!(state.usage.screen.as_ref().unwrap().scroll, u16::MAX);

    assert_reserved_usage_regions_are_inert(area);
}

#[test]
fn t03_changed_content_rejects_old_rows_until_successful_paint() {
    let area = term_120x40();
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| {
            screen.sort = crate::tui::screens::usage::UsageSort::Remaining;
            screen.selected = 0;
            screen.selected_id = None;
        },
        "Gamma",
        "provider:c",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| screen.filter = crate::tui::screens::usage::UsageFilter::Stale,
        "Beta",
        "provider:b",
    );
    let mut stale_filtered = rich_usage_manager();
    paint_usage(&mut stale_filtered, area);
    stale_filtered.usage.screen.as_mut().unwrap().filter =
        crate::tui::screens::usage::UsageFilter::Stale;
    let filtered = paint_usage(&mut stale_filtered, area);
    assert!(row_y_option(&filtered, "Beta account", area).is_some());
    assert!(row_y_option(&filtered, "Gamma account", area).is_some());
    assert!(row_y_option(&filtered, "Alpha", area).is_none());
    assert_row_clicks(
        &mut stale_filtered,
        area,
        "Beta account",
        Some("provider:b"),
        1,
    );
    assert_row_clicks(
        &mut stale_filtered,
        area,
        "Gamma account",
        Some("provider:c"),
        2,
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| screen.accounts[0].account = "Alpha revised".to_owned(),
        "Alpha revised",
        "provider:a",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| {
            screen.canonical_projection = Some(jackin_protocol::usage_broker::UsageProjectionV1 {
                schema_version: jackin_protocol::usage_broker::UsageProjectionSchemaV2,
                projection_id: "direct-projection-change".to_owned(),
                generated_at_epoch: NOW,
                discovery_revision: "fixture".to_owned(),
                broker_instance_id: "fixture".to_owned(),
                broker_generation: 1,
                refresh_state: jackin_protocol::usage_broker::UsageProjectionRefreshStateV1::Idle,
                providers: Vec::new(),
                unresolved: Vec::new(),
                issues: Vec::new(),
            });
        },
        "Beta",
        "provider:b",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| screen.refresh_generation = 9,
        "Beta",
        "provider:b",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| {
            screen.refresh_due = true;
            screen.force_refresh_pending = true;
        },
        "Beta",
        "provider:b",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| screen.scroll = 1,
        "Beta",
        "provider:b",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| screen.detail = true,
        "Gamma",
        "provider:c",
    );
    assert_mutation_repaints_before_dispatch(
        area,
        |screen| {
            screen.selected = 3;
            screen.selected_id = Some("provider:c".to_owned());
        },
        "Alpha",
        "provider:a",
    );

    assert_direct_quota_change_repaints_before_dispatch(area);
}

#[test]
fn t03_usage_owner_privacy_preserves_hidden_manager_and_effects() {
    let area = term_120x40();
    let mut active = rich_usage_manager();
    let painted = paint_usage(&mut active, area);
    let gamma_y = row_y(&painted, "Gamma", area);
    active.inline_account_picker = Some(crate::tui::state::AccountPickerState::new(
        "workspace".to_owned(),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));
    active.launch_account_picker = Some(crate::tui::state::AccountPickerState::new(
        jackin_core::RoleSelector::new(None, "architect"),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));
    assert_eq!(
        active.painted_input_owner(),
        ConsolePaintedInputOwner::Usage,
        "stale inline picker flags do not steal the painted Usage owner"
    );
    let active_click = mouse_at_kind(
        MouseEventKind::Down(MouseButton::Left),
        crate::tui::screens::usage::usage_frame_layout(area)
            .list_inner
            .x,
        gamma_y,
    );
    assert!(clickable_at(&active, active_click, area, None));
    dispatch(
        &mut active,
        active_click.kind,
        active_click.column,
        active_click.row,
        area,
    );
    assert_eq!(
        active.usage.screen.as_ref().unwrap().selected_id.as_deref(),
        Some("provider:c")
    );
    assert!(active.inline_account_picker.is_some());
    assert!(active.launch_account_picker.is_some());
    assert!(active.drain_effects().is_empty());
    assert_hidden_usage_owners_preserve_background(area);
}

fn assert_hidden_usage_owners_preserve_background(area: Rect) {
    let hidden_owners = [
        "help",
        "status",
        "list-modal",
        "editor",
        "settings",
        "create-prelude",
        "workflows",
    ];
    for owner in hidden_owners {
        let mut state = rich_usage_manager();
        state.list_split_pct = 42;
        crate::tui::scroll_block::scroll_area_set_x(&mut state.list_names_scroll, 3);
        crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
        crate::tui::scroll_block::scroll_area_set_x(&mut state.list_mounts_scroll, 5);
        crate::tui::scroll_block::scroll_area_set_y(&mut state.list_mounts_scroll, 6);
        paint_usage(&mut state, area);
        install_hidden_usage_owner(&mut state, owner);
        assert_ne!(state.painted_input_owner(), ConsolePaintedInputOwner::Usage);
        assert_hidden_usage_dispatch_is_inert(&mut state, area);
    }

    let mut absent = rich_usage_manager();
    absent.list_split_pct = 42;
    crate::tui::scroll_block::scroll_area_set_x(&mut absent.list_names_scroll, 3);
    crate::tui::scroll_block::scroll_area_set_y(&mut absent.list_names_scroll, 4);
    crate::tui::scroll_block::scroll_area_set_x(&mut absent.list_mounts_scroll, 5);
    crate::tui::scroll_block::scroll_area_set_y(&mut absent.list_mounts_scroll, 6);
    paint_usage(&mut absent, area);
    absent.usage.screen = None;
    assert_hidden_usage_dispatch_is_inert(&mut absent, area);
}

fn install_hidden_usage_owner(state: &mut ManagerState<'_>, owner: &str) {
    match owner {
        "help" => state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal()),
        "status" => {
            state.status_overlay = Some(crate::tui::components::StatusPopupState::new(
                "Busy", "Working",
            ));
        }
        "list-modal" => {
            state.list_modal = Some(Modal::ContainerInfo {
                state: crate::tui::components::container_info_surface::ContainerInfoState::new(
                    "Container information",
                    Vec::new(),
                ),
            });
        }
        "editor" => {
            state.stage = ManagerStage::Editor(EditorState::new_edit(
                "workspace".to_owned(),
                WorkspaceConfig::default(),
            ));
        }
        "settings" => {
            state.stage = ManagerStage::Settings(SettingsState::from_config(
                &jackin_config::AppConfig::default(),
            ));
        }
        "create-prelude" => {
            state.stage =
                ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState::default());
        }
        "workflows" => {
            state.stage = ManagerStage::Workflows(
                crate::tui::screens::workflows::model::WorkflowScreen::default(),
            );
        }
        _ => unreachable!("known hidden Usage owner"),
    }
}

fn assert_hidden_usage_dispatch_is_inert(state: &mut ManagerState<'_>, area: Rect) {
    let before = usage_snapshot(state);
    let selected = state.selected;
    let split = state.list_split_pct;
    let focus = state.list_focus_owner.clone();
    let names_scroll = (
        state.list_names_scroll.offset_x(),
        state.list_names_scroll.offset_y(),
    );
    let mounts_scroll = (
        state.list_mounts_scroll.offset_x(),
        state.list_mounts_scroll.offset_y(),
    );
    let picker_flags = (
        state.inline_role_picker.is_some(),
        state.inline_agent_picker.is_some(),
        state.inline_new_session_picker.is_some(),
        state.inline_account_picker.is_some(),
        state.launch_account_picker.is_some(),
    );
    let (x, y) = (area.x, area.y);
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ] {
        let mouse = mouse_at_kind(kind, x, y);
        assert!(!clickable_at(state, mouse, area, None));
        dispatch(state, kind, x, y, area);
        assert_usage_unchanged(state, &before);
        assert_eq!(state.selected, selected);
        assert_eq!(state.list_split_pct, split);
        assert_eq!(state.list_focus_owner, focus);
        assert_eq!(
            (
                state.list_names_scroll.offset_x(),
                state.list_names_scroll.offset_y()
            ),
            names_scroll,
        );
        assert_eq!(
            (
                state.list_mounts_scroll.offset_x(),
                state.list_mounts_scroll.offset_y()
            ),
            mounts_scroll,
        );
        assert_eq!(
            (
                state.inline_role_picker.is_some(),
                state.inline_agent_picker.is_some(),
                state.inline_new_session_picker.is_some(),
                state.inline_account_picker.is_some(),
                state.launch_account_picker.is_some(),
            ),
            picker_flags,
        );
    }
    assert!(state.drain_effects().is_empty());
}
