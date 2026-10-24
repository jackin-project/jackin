// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::screens::usage::{
    UsageRowTarget, UsageWheelAction, WheelDirection, list_geometry, usage_frame_layout,
    usage_wheel_action,
};
use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, MeterPercentPoints,
    UsageMeterSide,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

const REFRESHING: &str = "Refreshing usage…";

fn account(
    provider_id: &str,
    id: &str,
    provider: &str,
    label: &str,
    remaining: f64,
) -> UsageAccount {
    let mut account = test_account(provider_id, id, label);
    account.provider = provider.to_owned();
    account.status = format!("status-{id}");
    account.windows[0].value = format!("summary-{id}");
    account.windows[0].reset = format!("reset-{id}");
    account.windows[0].remaining_percent = Some(
        jackin_protocol::control::RawPercentPoints::new(remaining).expect("finite percentage"),
    );
    account.windows[0].meter_percent =
        Some(MeterPercentPoints::new(remaining.clamp(0.0, 100.0)).expect("meter percentage"));
    account.windows[0].meter_side = Some(UsageMeterSide::Remaining);
    account
}

fn fixture_screen() -> UsageScreenState {
    UsageScreenState {
        accounts: vec![
            account("px9", "a9", "PX9", "A9", 90.0),
            account("px9", "b9", "PX9", "B9", 50.0),
            account("py9", "c9", "PY9", "C9", 10.0),
        ],
        ..UsageScreenState::default()
    }
}

fn render_canvas(
    manager: &crate::tui::state::ManagerState<'_>,
    area: Rect,
    canvas: (u16, u16),
) -> Buffer {
    let backend = TestBackend::new(canvas.0, canvas.1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_at(frame, area, manager, TEST_NOW_EPOCH))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn render_detail_canvas(
    manager: &crate::tui::state::ManagerState<'_>,
    area: Rect,
    canvas: (u16, u16),
) -> Buffer {
    let backend = TestBackend::new(canvas.0, canvas.1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_detail(frame, area, manager, TEST_NOW_EPOCH))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn manual_list_inner(area: Rect) -> Rect {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    Block::default().borders(Borders::ALL).inner(columns[0])
}

fn physical_row(buffer: &Buffer, area: Rect, y: u16) -> String {
    (area.x..area.x.saturating_add(area.width))
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn row_with(buffer: &Buffer, area: Rect, needle: &str) -> u16 {
    (area.y..area.y.saturating_add(area.height))
        .find(|&y| physical_row(buffer, area, y).contains(needle))
        .unwrap_or_else(|| panic!("row containing {needle:?} not found in {area:?}"))
}

fn query(
    screen: &UsageScreenState,
    layout: crate::tui::screens::usage::UsageFrameLayout,
    x: u16,
    y: u16,
) -> Option<UsageRowTarget> {
    list_geometry(screen, layout.list_inner, TEST_NOW_EPOCH, true).target_at(x, y, screen.scroll)
}

fn assert_selected(
    screen: &mut UsageScreenState,
    target: UsageRowTarget,
    selected: usize,
    id: Option<&str>,
) {
    assert!(screen.select_rendered_target(&target));
    assert_eq!(screen.selected, selected);
    assert_eq!(screen.selected_id.as_deref(), id);
}

fn assert_row_target_and_selection(
    screen: &UsageScreenState,
    layout: crate::tui::screens::usage::UsageFrameLayout,
    y: u16,
    target: UsageRowTarget,
    selected: usize,
    id: Option<&str>,
) {
    let right = layout.list_inner.x + layout.list_inner.width - 1;
    for x in [layout.list_inner.x, right] {
        assert_eq!(query(screen, layout, x, y), Some(target.clone()));
        let original_scroll = screen.scroll;
        let original_accounts = screen.accounts.clone();
        let mut selected_screen = screen.clone();
        assert_selected(&mut selected_screen, target.clone(), selected, id);
        assert_eq!(
            selected_screen.scroll, original_scroll,
            "selection preserves scroll"
        );
        assert_eq!(
            selected_screen.accounts, original_accounts,
            "selection preserves inventory"
        );
    }
}

fn assert_visible_wide_continuations(
    screen: &UsageScreenState,
    layout: crate::tui::screens::usage::UsageFrameLayout,
    buffer: &Buffer,
    y: u16,
    target: &UsageRowTarget,
) -> bool {
    let mut found = false;
    for x in layout.list_inner.x..layout.list_inner.x + layout.list_inner.width {
        if ["界", "表", "漢"].contains(&buffer[(x, y)].symbol()) {
            let continuation_x = x + 1;
            assert!(
                continuation_x < layout.list_inner.x + layout.list_inner.width,
                "the painted wide character has a follower inside the list"
            );
            assert_eq!(buffer[(continuation_x, y)].symbol(), " ");
            found = true;
            assert_eq!(
                query(screen, layout, continuation_x, y),
                Some((*target).clone())
            );
        }
    }
    found
}

fn meter_row_between(buffer: &Buffer, area: Rect, start: u16, end: u16) -> Option<u16> {
    (start..end).find(|&y| {
        let row = physical_row(buffer, area, y);
        row.contains('█') || row.contains('░')
    })
}

#[test]
fn t02_rendered_overview_account_summary_and_meter_rows_are_click_targets() {
    let area = Rect::new(9, 5, 150, 46);
    let mut manager = s8_manager(fixture_screen());
    let layout = usage_frame_layout(area);
    assert_eq!(layout.list_inner, manual_list_inner(area));
    let buffer = render_canvas(&manager, area, (180, 65));

    let overview_y = row_with(&buffer, layout.list_inner, "Overview");
    let overview = query(
        manager.usage.screen.as_ref().unwrap(),
        layout,
        layout.list_inner.x + layout.list_inner.width - 1,
        overview_y,
    )
    .expect("painted overview row is a target");
    assert_eq!(overview, UsageRowTarget::Overview);
    assert_selected(manager.usage.screen.as_mut().unwrap(), overview, 0, None);

    let screen = manager.usage.screen.as_ref().unwrap();
    let labels = [
        ("A9", "summary-a9", "px9:a9", 1),
        ("B9", "summary-b9", "px9:b9", 2),
        ("C9", "summary-c9", "py9:c9", 3),
    ];
    let label_rows = labels
        .iter()
        .map(|(label, _, _, _)| row_with(&buffer, layout.list_inner, label))
        .collect::<Vec<_>>();
    for (position, (label, summary, id, ordinal)) in labels.iter().enumerate() {
        let target = UsageRowTarget::Account((*id).to_owned());
        let label_y = label_rows[position];
        let summary_y = row_with(&buffer, layout.list_inner, summary);
        assert!(label_y < summary_y, "account label precedes its summary");
        let next_label = label_rows
            .get(position + 1)
            .copied()
            .unwrap_or(layout.list_inner.y + layout.list_inner.height);
        let meter_y = meter_row_between(&buffer, layout.list_inner, summary_y + 1, next_label)
            .unwrap_or_else(|| panic!("the rendered meter after {label} is missing"));

        for (marker, y) in [
            ("label", label_y),
            ("summary", summary_y),
            ("meter", meter_y),
        ] {
            assert_row_target_and_selection(screen, layout, y, target.clone(), *ordinal, Some(*id));
            assert!(
                !physical_row(&buffer, layout.list_inner, y)
                    .trim()
                    .is_empty(),
                "painted {marker} row for {label} is nonempty"
            );
        }
    }
}

#[test]
fn t02_wrapped_unicode_resize_and_scrolled_rows_select_rendered_stable_ids() {
    let mut screen = fixture_screen();
    screen.accounts[0].account = format!("A9界e\u{301}{}", "界A9e\u{301}".repeat(8));
    screen.accounts[0].windows[0].value = format!("summary-a9:{}", "界".repeat(16));
    screen.accounts[1].account = format!("B9表{}", "表B9".repeat(8));
    screen.accounts[2].account = format!("C9e\u{301}{}", "漢C9".repeat(7));
    let mut seen = std::collections::BTreeSet::new();
    let mut saw_visible_wide_continuation = false;
    let mut narrow_a_rows = std::collections::BTreeSet::new();

    for width in [40, 63, 100, 180] {
        for scroll in [0, 5, 13, 25, 40] {
            let area = Rect::new(4, 3, width, 42);
            let mut current = screen.clone();
            current.scroll = scroll;
            let manager = s8_manager(current);
            let layout = usage_frame_layout(area);
            let buffer = render_canvas(&manager, area, (width + 10, 50));
            let screen = manager.usage.screen.as_ref().unwrap();
            for y in layout.list_inner.y..layout.list_inner.y + layout.list_inner.height {
                let row = physical_row(&buffer, layout.list_inner, y);
                let expected = if row.contains("A9") || row.contains("a9") || row.contains('界') {
                    Some((UsageRowTarget::Account("px9:a9".to_owned()), 1, "px9:a9"))
                } else if row.contains("B9") || row.contains("b9") || row.contains('表') {
                    Some((UsageRowTarget::Account("px9:b9".to_owned()), 2, "px9:b9"))
                } else if row.contains("C9") || row.contains("c9") || row.contains('漢') {
                    Some((UsageRowTarget::Account("py9:c9".to_owned()), 3, "py9:c9"))
                } else {
                    None
                };
                let Some((target, ordinal, id)) = expected else {
                    continue;
                };
                assert_row_target_and_selection(
                    screen,
                    layout,
                    y,
                    target.clone(),
                    ordinal,
                    Some(id),
                );
                if width == 40 && screen.scroll == 0 && id == "px9:a9" {
                    narrow_a_rows.insert(y.saturating_sub(layout.list_inner.y));
                }
                if width == 180 {
                    saw_visible_wide_continuation |=
                        assert_visible_wide_continuations(screen, layout, &buffer, y, &target);
                }
                seen.insert(id.to_owned());
            }
        }
    }
    assert_eq!(
        seen,
        ["px9:a9", "px9:b9", "py9:c9"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert!(
        narrow_a_rows.len() > 2,
        "the narrow A9 label paints more than two token rows"
    );
    assert!(
        saw_visible_wide_continuation,
        "wide Unicode continuation cells are painted and queried"
    );
}

#[test]
fn t02_provider_spacers_borders_seam_status_and_offscreen_are_inert() {
    let area = Rect::new(6, 4, 150, 44);
    let manager = s8_manager(fixture_screen());
    let layout = usage_frame_layout(area);
    let buffer = render_canvas(&manager, area, (170, 60));
    let screen = manager.usage.screen.as_ref().unwrap();
    let provider_y = row_with(&buffer, layout.list_inner, "PX9");
    assert_eq!(query(screen, layout, layout.list_inner.x, provider_y), None);
    let overview_y = row_with(&buffer, layout.list_inner, "Overview");
    assert_eq!(
        query(screen, layout, layout.list_inner.x, overview_y + 1),
        None,
        "the blank separator under Overview is not selectable"
    );
    assert_eq!(
        query(screen, layout, layout.list_outer.x, layout.list_inner.y),
        None,
        "list border is outside the inner hit area"
    );
    assert_eq!(
        query(
            screen,
            layout,
            layout.list_outer.x + layout.list_outer.width,
            layout.list_inner.y
        ),
        None,
        "the panel seam is inert"
    );
    assert_eq!(
        query(
            screen,
            layout,
            layout.list_inner.x,
            area.y + area.height - 1
        ),
        None,
        "workspace footer is outside the painted list"
    );
    assert_eq!(
        usage_wheel_action(
            layout,
            screen,
            layout.detail_inner.x,
            layout.detail_status.y,
            WheelDirection::Down,
        ),
        None,
        "the reserved status row is outside detail body wheel geometry"
    );

    let mut offscreen = screen.clone();
    offscreen.scroll = u16::MAX;
    assert_eq!(
        query(&offscreen, layout, layout.list_inner.x, layout.list_inner.y),
        None,
        "scroll beyond all rendered content has no account target"
    );
}

#[test]
fn t02_empty_filtered_empty_and_tiny_viewports_have_no_account_targets() {
    let mut empty = UsageScreenState::default();
    let area = Rect::new(0, 0, 80, 24);
    let layout = usage_frame_layout(area);
    let empty_geometry = list_geometry(&empty, layout.list_inner, TEST_NOW_EPOCH, true);
    assert!(empty_geometry.row_targets.iter().all(Option::is_none));
    let before = empty.clone();
    assert!(!empty.select_rendered_target(&UsageRowTarget::Overview));
    assert_eq!(empty, before);

    let mut filtered = fixture_screen();
    filtered.filter = UsageFilter::Stale;
    let filtered_geometry = list_geometry(&filtered, layout.list_inner, TEST_NOW_EPOCH, true);
    assert!(filtered_geometry.row_targets.iter().all(Option::is_none));
    for direction in [WheelDirection::Up, WheelDirection::Down] {
        assert_eq!(
            usage_wheel_action(
                layout,
                &filtered,
                layout.list_inner.x,
                layout.list_inner.y,
                direction
            ),
            None
        );
    }
    let before = filtered.clone();
    assert!(!filtered.select_rendered_target(&UsageRowTarget::Account("px9:a9".to_owned())));
    assert_eq!(
        filtered, before,
        "a filtered account cannot change selection state"
    );

    let zero = Rect::new(3, 4, 0, 0);
    let zero_geometry = list_geometry(&fixture_screen(), zero, TEST_NOW_EPOCH, true);
    assert!(zero_geometry.row_targets.is_empty());
    let tiny = usage_frame_layout(Rect::new(3, 4, 1, 1));
    let tiny_geometry = list_geometry(&fixture_screen(), tiny.list_inner, TEST_NOW_EPOCH, true);
    assert!(tiny_geometry.row_targets.is_empty());
    assert_eq!(
        usage_wheel_action(
            tiny,
            &fixture_screen(),
            tiny.list_inner.x,
            tiny.list_inner.y,
            WheelDirection::Down
        ),
        None
    );
}

#[test]
fn t02_wheel_list_moves_one_detail_moves_three_and_status_is_reserved() {
    let area = Rect::new(5, 3, 150, 44);
    let layout = usage_frame_layout(area);
    let mut screen = fixture_screen();
    screen.scroll = 0;
    let manager = s8_manager(screen.clone());
    let buffer = render_canvas(&manager, area, (170, 60));
    let overview_y = row_with(&buffer, layout.list_inner, "Overview");
    let blank_point = (
        layout.list_inner.x + layout.list_inner.width - 1,
        overview_y + 1,
    );
    assert_eq!(
        query(&screen, layout, blank_point.0, blank_point.1),
        None,
        "the rendered spacer is not a selectable row"
    );
    let provider_y = row_with(&buffer, layout.list_inner, "PX9");
    let header_point = (layout.list_inner.x, provider_y);
    assert_eq!(
        query(&screen, layout, header_point.0, header_point.1),
        None,
        "provider heading is a painted nonselectable row"
    );
    let apply_queried_wheel =
        |screen: &mut UsageScreenState, point: (u16, u16), direction: WheelDirection| {
            let action = usage_wheel_action(layout, screen, point.0, point.1, direction)
                .expect("the queried usage pane captures its wheel input");
            screen.apply_wheel_action(action);
            action
        };

    assert_eq!(
        apply_queried_wheel(&mut screen, blank_point, WheelDirection::Down),
        UsageWheelAction::MoveSelection(1)
    );
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id.as_deref(), Some("px9:a9"));
    assert_eq!(
        apply_queried_wheel(&mut screen, header_point, WheelDirection::Down),
        UsageWheelAction::MoveSelection(1)
    );
    assert_eq!(screen.selected, 2);
    assert_eq!(screen.selected_id.as_deref(), Some("px9:b9"));
    assert_eq!(
        apply_queried_wheel(&mut screen, blank_point, WheelDirection::Up),
        UsageWheelAction::MoveSelection(-1)
    );
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id.as_deref(), Some("px9:a9"));
    assert_eq!(
        apply_queried_wheel(&mut screen, blank_point, WheelDirection::Up),
        UsageWheelAction::MoveSelection(-1)
    );
    assert_eq!(screen.selected, 0);
    assert_eq!(
        apply_queried_wheel(&mut screen, header_point, WheelDirection::Up),
        UsageWheelAction::MoveSelection(-1)
    );
    assert_eq!(screen.selected, 0, "selection saturates at Overview");
    assert_eq!(screen.selected_id, None);

    for (ordinal, id) in [(1, "px9:a9"), (2, "px9:b9"), (3, "py9:c9")] {
        assert_eq!(
            apply_queried_wheel(&mut screen, header_point, WheelDirection::Down),
            UsageWheelAction::MoveSelection(1)
        );
        assert_eq!(screen.selected, ordinal);
        assert_eq!(screen.selected_id.as_deref(), Some(id));
    }
    assert_eq!(
        apply_queried_wheel(&mut screen, header_point, WheelDirection::Down),
        UsageWheelAction::MoveSelection(1)
    );
    assert_eq!(
        screen.selected, 3,
        "selection saturates at the last visible account"
    );
    assert_eq!(screen.selected_id.as_deref(), Some("py9:c9"));

    screen.scroll = u16::MAX;
    assert_eq!(
        query(&screen, layout, header_point.0, header_point.1),
        None,
        "the unpainted viewport at MAX scroll has no row target"
    );
    assert_eq!(
        apply_queried_wheel(&mut screen, header_point, WheelDirection::Down),
        UsageWheelAction::MoveSelection(1),
        "wheel handling still uses the list pane rather than painted row authority"
    );
    assert_eq!(screen.selected, 3);
    assert_eq!(screen.selected_id.as_deref(), Some("py9:c9"));
    assert_eq!(
        screen.scroll,
        u16::MAX,
        "list-wheel selection preserves the offscreen scroll"
    );

    let body_point = (layout.detail_body.x, layout.detail_body.y);
    screen.scroll = 0;
    assert_eq!(
        apply_queried_wheel(&mut screen, body_point, WheelDirection::Down),
        UsageWheelAction::ScrollDetail(3)
    );
    assert_eq!(screen.scroll, 3);
    assert_eq!(
        apply_queried_wheel(&mut screen, body_point, WheelDirection::Up),
        UsageWheelAction::ScrollDetail(-3)
    );
    assert_eq!(screen.scroll, 0, "detail scroll saturates at zero");
    screen.scroll = 1;
    assert_eq!(
        apply_queried_wheel(&mut screen, body_point, WheelDirection::Up),
        UsageWheelAction::ScrollDetail(-3)
    );
    assert_eq!(screen.scroll, 0, "one-unit detail offset saturates at zero");
    screen.scroll = u16::MAX - 1;
    assert_eq!(
        apply_queried_wheel(&mut screen, body_point, WheelDirection::Down),
        UsageWheelAction::ScrollDetail(3)
    );
    assert_eq!(
        screen.scroll,
        u16::MAX,
        "detail scroll saturates at u16::MAX"
    );
    assert_noncontent_wheel_areas_are_inert(layout, &screen, area);

    assert_one_row_detail_status_has_no_body_wheel_target();
}

fn assert_noncontent_wheel_areas_are_inert(
    layout: crate::tui::screens::usage::UsageFrameLayout,
    screen: &UsageScreenState,
    area: Rect,
) {
    for direction in [WheelDirection::Up, WheelDirection::Down] {
        assert_eq!(
            usage_wheel_action(
                layout,
                screen,
                layout.detail_status.x,
                layout.detail_status.y,
                direction
            ),
            None
        );
        assert_eq!(
            usage_wheel_action(
                layout,
                screen,
                layout.detail_outer.x,
                layout.detail_outer.y,
                direction
            ),
            None
        );
        assert_eq!(
            usage_wheel_action(
                layout,
                screen,
                layout.list_outer.x,
                layout.list_outer.y,
                direction
            ),
            None
        );
        assert_eq!(
            usage_wheel_action(
                layout,
                screen,
                layout.list_outer.x + layout.list_outer.width,
                layout.list_inner.y,
                direction,
            ),
            None,
            "the panel seam is outside both wheel panes"
        );
        assert_eq!(
            usage_wheel_action(
                layout,
                screen,
                layout.list_inner.x,
                area.y + area.height - 1,
                direction
            ),
            None,
            "workspace footer is outside both wheel panes"
        );
    }
}

fn assert_one_row_detail_status_has_no_body_wheel_target() {
    let mut one_row_screen = fixture_screen();
    one_row_screen.detail = true;
    let refresh = snapshot(one_row_screen.accounts.clone(), None);
    one_row_screen.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        one_row_screen.refresh_generation,
        Ok(refresh),
    )));
    let one_row_manager = s8_manager(one_row_screen);
    let detail_outer = Rect::new(20, 4, 40, 3);
    let detail_inner = super::super::panel("Overview").inner(detail_outer);
    let detail_areas = super::super::geometry::detail_areas(detail_inner);
    let one_row = crate::tui::screens::usage::UsageFrameLayout {
        list_outer: Rect::new(0, 0, 0, 0),
        list_inner: Rect::new(0, 0, 0, 0),
        detail_outer,
        detail_inner,
        detail_status: detail_areas.status,
        detail_body: detail_areas.body,
    };
    let one_row_buffer = render_detail_canvas(&one_row_manager, detail_outer, (80, 12));
    assert_eq!(
        one_row.detail_inner.height, 1,
        "the real panel leaves one inner row"
    );
    assert_eq!(one_row.detail_status.height, 1);
    assert_eq!(
        one_row.detail_body.height, 0,
        "the reserved status consumes the only inner row"
    );
    assert!(
        physical_row(
            &one_row_buffer,
            one_row.detail_status,
            one_row.detail_status.y
        )
        .contains(REFRESHING)
    );
    assert_ne!(
        one_row_buffer[(detail_outer.x, detail_outer.y)].symbol(),
        " ",
        "panel border is painted"
    );
    for direction in [WheelDirection::Up, WheelDirection::Down] {
        let one_row_screen = one_row_manager.usage.screen.as_ref().unwrap();
        assert_eq!(
            usage_wheel_action(
                one_row,
                one_row_screen,
                one_row.detail_status.x,
                one_row.detail_status.y,
                direction
            ),
            None
        );
        assert_eq!(
            usage_wheel_action(
                one_row,
                one_row_screen,
                one_row.detail_inner.x,
                one_row.detail_inner.y,
                direction
            ),
            None,
            "a real one-inner-row detail panel has no body wheel target"
        );
    }
}

#[test]
fn t02_sorted_and_filtered_rendered_rows_keep_stable_account_targets() {
    let area = Rect::new(7, 4, 150, 44);
    let mut screen = fixture_screen();
    screen.sort = UsageSort::Remaining;
    screen.scroll = 0;
    let manager = s8_manager(screen);
    let layout = usage_frame_layout(area);
    let buffer = render_canvas(&manager, area, (170, 60));
    let screen = manager.usage.screen.as_ref().unwrap();
    for (label, id, position) in [
        ("C9", "py9:c9", 1),
        ("B9", "px9:b9", 2),
        ("A9", "px9:a9", 3),
    ] {
        let y = row_with(&buffer, layout.list_inner, label);
        assert_row_target_and_selection(
            screen,
            layout,
            y,
            UsageRowTarget::Account(id.to_owned()),
            position,
            Some(id),
        );
    }

    let mut filtered = screen.clone();
    filtered.accounts[0].is_stale = false;
    filtered.accounts[1].is_stale = true;
    filtered.accounts[1].freshness_phase = UsageFreshnessPhaseV1::Stale;
    filtered.accounts[2].is_stale = true;
    filtered.accounts[2].freshness_phase = UsageFreshnessPhaseV1::Stale;
    filtered.filter = UsageFilter::Stale;
    filtered.scroll = 0;
    let filtered_manager = s8_manager(filtered);
    let filtered_buffer = render_canvas(&filtered_manager, area, (170, 60));
    let filtered_layout = usage_frame_layout(area);
    let filtered = filtered_manager.usage.screen.as_ref().unwrap();
    assert!(
        (filtered_layout.list_inner.y
            ..filtered_layout.list_inner.y + filtered_layout.list_inner.height)
            .all(|y| !physical_row(&filtered_buffer, filtered_layout.list_inner, y).contains("A9")),
        "the fresh A9 account is not painted by the stale filter"
    );
    let filtered_rows = [("C9", "py9:c9", 1), ("B9", "px9:b9", 2)];
    for (label, id, ordinal) in filtered_rows {
        let y = row_with(&filtered_buffer, filtered_layout.list_inner, label);
        assert_row_target_and_selection(
            filtered,
            filtered_layout,
            y,
            UsageRowTarget::Account(id.to_owned()),
            ordinal,
            Some(id),
        );
    }

    let mut tested_filtered = filtered.clone();
    let before = tested_filtered.clone();
    assert!(!tested_filtered.select_rendered_target(&UsageRowTarget::Account("px9:a9".to_owned())));
    assert_eq!(
        tested_filtered, before,
        "hidden stable IDs do not mutate selection"
    );
    assert!(
        !tested_filtered.select_rendered_target(&UsageRowTarget::Account("px9:removed".to_owned()))
    );
    assert_eq!(
        tested_filtered, before,
        "removed stable IDs do not mutate selection"
    );
}

#[test]
fn t02_shared_paragraph_geometry_matches_translated_paint() {
    let area = Rect::new(11, 6, 160, 48);
    let mut screen = fixture_screen();
    screen.accounts[0].account = format!("A9界e\u{301}{}", "界".repeat(40));
    screen.accounts[0].windows[0].value = format!("summary-a9 {}", "界".repeat(24));
    screen.accounts[1].account = format!("B9{}", "表".repeat(14));
    let manager = s8_manager(screen);
    let layout = usage_frame_layout(area);
    assert_eq!(layout.list_inner, manual_list_inner(area));
    let buffer = render_canvas(&manager, area, (190, 70));
    let screen = manager.usage.screen.as_ref().unwrap();
    let geometry = list_geometry(screen, layout.list_inner, TEST_NOW_EPOCH, true);
    let mut observed = std::collections::BTreeSet::new();
    let mut saw_visible_wide_continuation = false;

    for y in layout.list_inner.y..layout.list_inner.y + layout.list_inner.height {
        let row = physical_row(&buffer, layout.list_inner, y);
        let expected = if row.contains("A9") || row.contains("a9") || row.contains('界') {
            Some((UsageRowTarget::Account("px9:a9".to_owned()), 1, "px9:a9"))
        } else if row.contains("B9") || row.contains("b9") || row.contains('表') {
            Some((UsageRowTarget::Account("px9:b9".to_owned()), 2, "px9:b9"))
        } else if row.contains("C9") || row.contains("c9") || row.contains('漢') {
            Some((UsageRowTarget::Account("py9:c9".to_owned()), 3, "py9:c9"))
        } else {
            None
        };
        if let Some((target, ordinal, id)) = expected {
            for x in [
                layout.list_inner.x,
                layout.list_inner.x + layout.list_inner.width - 1,
            ] {
                assert_eq!(
                    geometry.target_at(x, y, screen.scroll),
                    Some(target.clone()),
                    "painted physical row={row:?}, x={x}, y={y}"
                );
            }
            assert_row_target_and_selection(screen, layout, y, target.clone(), ordinal, Some(id));
            for x in layout.list_inner.x..layout.list_inner.x + layout.list_inner.width {
                if ["界", "表", "漢"].contains(&buffer[(x, y)].symbol()) {
                    let continuation_x = x + 1;
                    assert!(
                        continuation_x < layout.list_inner.x + layout.list_inner.width,
                        "the painted wide character has a follower inside the list"
                    );
                    assert_eq!(buffer[(continuation_x, y)].symbol(), " ");
                    saw_visible_wide_continuation = true;
                    assert_eq!(
                        geometry.target_at(continuation_x, y, screen.scroll),
                        Some(target.clone())
                    );
                }
            }
            observed.insert(id.to_owned());
        }
    }
    assert_eq!(
        observed,
        ["px9:a9", "px9:b9", "py9:c9"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert!(
        saw_visible_wide_continuation,
        "translated paint includes wide continuation cells"
    );
    assert_eq!(
        geometry.target_at(layout.list_outer.x, layout.list_inner.y, screen.scroll),
        None
    );
    assert_eq!(
        geometry.target_at(layout.list_inner.x, layout.list_inner.y, u16::MAX),
        None
    );
}

#[test]
fn t02_typed_count_and_meter_side_rows_keep_stable_targets() {
    let mut count = account("px9", "a9", "PX9", "A9", 90.0);
    count.windows[0].value = "90 of 100 requests".to_owned();
    count.windows[0].count_quota = Some(CountQuota::new(
        Some(90),
        Some(100),
        Some(10),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    count.windows[0].meter_side = Some(UsageMeterSide::Used);
    count.windows[0].meter_percent = Some(MeterPercentPoints::new(90.0).unwrap());
    let mut no_meter = account("px9", "b9", "PX9", "B9", 0.0);
    no_meter.windows[0].value = "exact-count-only".to_owned();
    no_meter.windows[0].meter_percent = None;
    no_meter.windows[0].meter_side = None;
    no_meter.windows[0].count_quota = Some(CountQuota::new(
        Some(12),
        None,
        None,
        CountQuotaUnit::Unknown,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    let screen = UsageScreenState {
        accounts: vec![count, no_meter],
        ..UsageScreenState::default()
    };
    let area = Rect::new(5, 3, 150, 44);
    let layout = usage_frame_layout(area);
    let manager = s8_manager(screen);
    let buffer = render_canvas(&manager, area, (170, 60));
    let screen = manager.usage.screen.as_ref().unwrap();
    let a_label_y = row_with(&buffer, layout.list_inner, "A9");
    let a_summary_y = row_with(&buffer, layout.list_inner, "90 of 100 requests");
    let b_label_y = row_with(&buffer, layout.list_inner, "B9");
    let b_summary_y = row_with(&buffer, layout.list_inner, "exact-count-only");
    let used_meter_y = meter_row_between(&buffer, layout.list_inner, a_summary_y + 1, b_label_y)
        .expect("used-side typed quota paints its actual meter row");
    for y in [a_label_y, a_summary_y, used_meter_y] {
        assert_row_target_and_selection(
            screen,
            layout,
            y,
            UsageRowTarget::Account("px9:a9".to_owned()),
            1,
            Some("px9:a9"),
        );
    }
    for y in [b_label_y, b_summary_y] {
        assert_row_target_and_selection(
            screen,
            layout,
            y,
            UsageRowTarget::Account("px9:b9".to_owned()),
            2,
            Some("px9:b9"),
        );
    }
    assert!(
        meter_row_between(
            &buffer,
            layout.list_inner,
            b_summary_y + 1,
            layout.list_inner.y + layout.list_inner.height,
        )
        .is_none(),
        "count-only B9 paints no fabricated meter before the list ends"
    );
    let screen_copy = screen.clone();
    assert_eq!(
        screen_copy.accounts[0].windows[0].count_quota,
        Some(CountQuota::new(
            Some(90),
            Some(100),
            Some(10),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        ))
    );
}

#[test]
fn t02_shared_layout_resize_preserves_status_and_body_wheel_geometry() {
    let mut screen = fixture_screen();
    screen.detail = true;
    let result = snapshot(screen.accounts.clone(), None);
    screen.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        screen.refresh_generation,
        Ok(result),
    )));
    let mut manager = s8_manager(screen);
    let area = Rect::new(8, 5, 150, 45);
    let layout = usage_frame_layout(area);
    let active = render_canvas(&manager, area, (180, 65));
    assert!(
        physical_row(&active, layout.detail_status, layout.detail_status.y).contains(REFRESHING)
    );
    let active_screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(
        usage_wheel_action(
            layout,
            active_screen,
            layout.detail_status.x,
            layout.detail_status.y,
            WheelDirection::Down,
        ),
        None,
        "refresh status remains reserved from body scrolling"
    );
    assert_eq!(
        usage_wheel_action(
            layout,
            active_screen,
            layout.detail_body.x,
            layout.detail_body.y,
            WheelDirection::Down,
        ),
        Some(UsageWheelAction::ScrollDetail(3))
    );

    let screen = manager.usage.screen.as_mut().unwrap();
    let completed = screen.poll_refresh().unwrap().unwrap();
    screen.apply_refresh(completed, Instant::now());
    let idle = render_canvas(&manager, area, (180, 65));
    assert_eq!(layout, usage_frame_layout(area));
    assert!(
        !physical_row(&idle, layout.detail_status, layout.detail_status.y).contains(REFRESHING)
    );
    assert!(
        (layout.detail_body.y..layout.detail_body.y + layout.detail_body.height)
            .any(|y| physical_row(&idle, layout.detail_body, y).contains("A9"))
    );

    for resized_area in [Rect::new(8, 5, 110, 35), Rect::new(8, 5, 170, 50)] {
        let resized = usage_frame_layout(resized_area);
        let buffer = render_canvas(&manager, resized_area, (190, 65));
        assert!(resized.detail_status.height <= 1);
        assert_eq!(
            usage_wheel_action(
                resized,
                manager.usage.screen.as_ref().unwrap(),
                resized.detail_status.x,
                resized.detail_status.y,
                WheelDirection::Down,
            ),
            None
        );
        if resized.detail_body.height > 0 {
            assert_eq!(
                usage_wheel_action(
                    resized,
                    manager.usage.screen.as_ref().unwrap(),
                    resized.detail_body.x,
                    resized.detail_body.y,
                    WheelDirection::Down,
                ),
                Some(UsageWheelAction::ScrollDetail(3))
            );
            assert!(
                (resized.detail_body.y..resized.detail_body.y + resized.detail_body.height)
                    .any(|y| physical_row(&buffer, resized.detail_body, y).contains("A9"))
            );
        }
    }
}
