// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::screens::usage::list_geometry;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

fn route_screen() -> UsageScreenState {
    let mut first = test_account("provider", "a", "Alpha account");
    first.provider = "Provider".to_owned();
    let mut second = test_account("provider", "b", "Beta account");
    second.provider = "Provider".to_owned();
    UsageScreenState {
        accounts: vec![first, second],
        ..UsageScreenState::default()
    }
}

fn painted_receipt(
    manager: &crate::tui::state::ManagerState<'_>,
    area: Rect,
) -> (Buffer, crate::tui::screens::usage::UsagePaintReceipt) {
    let backend = TestBackend::new(area.right(), area.bottom());
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut receipt = None;
    terminal
        .draw(|frame| {
            receipt = crate::tui::screens::usage::render_at_with_pointer_receipt(
                frame,
                area,
                manager,
                TEST_NOW_EPOCH,
            );
        })
        .expect("test frame draw");
    (
        terminal.backend().buffer().clone(),
        receipt.expect("visible Usage screen produces actual-frame receipt"),
    )
}

fn assert_revision_change_rejected(change: impl FnOnce(&mut UsageScreenState)) {
    let area = Rect::new(0, 0, 120, 40);
    let manager = s8_manager(route_screen());
    let (_, receipt) = painted_receipt(&manager, area);
    let screen = manager.usage.screen.as_ref().expect("Usage screen");
    assert!(receipt.matches(area, screen));

    let mut changed = screen.clone();
    change(&mut changed);
    assert!(!receipt.matches(area, &changed));
}

fn painted_row_y(buffer: &Buffer, area: Rect, needle: &str) -> u16 {
    let inner = crate::tui::screens::usage::usage_frame_layout(area).list_inner;
    (inner.y..inner.y + inner.height)
        .find(|&y| {
            (inner.x..inner.x + inner.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .contains(needle)
        })
        .unwrap_or_else(|| panic!("painted list row {needle:?} not found"))
}

fn painted_detail_row(buffer: &Buffer, area: Rect, y: u16) -> String {
    (area.x..area.x.saturating_add(area.width))
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn independent_detail_buffer(
    manager: &crate::tui::state::ManagerState<'_>,
    outer: Rect,
    canvas_height: u16,
) -> Buffer {
    let backend = TestBackend::new(outer.right().max(1), canvas_height.max(outer.bottom()));
    let mut terminal = Terminal::new(backend).expect("independent full-height paint");
    terminal
        .draw(|frame| render_detail(frame, outer, manager, TEST_NOW_EPOCH))
        .expect("full-height detail paint");
    terminal.backend().buffer().clone()
}

fn assert_detail_scroll_matches_full_paint(screen: UsageScreenState, area: Rect) -> u16 {
    let mut screen = screen;
    screen.scroll = u16::MAX;
    let manager = s8_manager(screen);
    let (actual, receipt) = painted_receipt(&manager, area);
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    let body = layout.detail_body;
    if body.width == 0 || body.height == 0 {
        assert_eq!(receipt.detail_scroll_bounds(), (0, 0));
        return 0;
    }

    let mut full_screen = manager.usage.screen.as_ref().unwrap().clone();
    full_screen.scroll = 0;
    let full_manager = s8_manager(full_screen);
    let full_outer = Rect::new(
        layout.detail_outer.x,
        layout.detail_outer.y,
        layout.detail_outer.width,
        512_u16.saturating_sub(layout.detail_outer.y),
    );
    let full = independent_detail_buffer(&full_manager, full_outer, 512);
    let full_body =
        super::super::geometry::detail_areas(super::super::panel("Overview").inner(full_outer))
            .body;
    let last_content_y = (full_body.y..full_body.y.saturating_add(full_body.height))
        .rev()
        .find(|&y| !painted_detail_row(&full, full_body, y).trim().is_empty())
        .expect("full-height body contains rendered detail content");
    let full_body_end = full_body.y.saturating_add(full_body.height);
    assert!(
        last_content_y.saturating_add(1) < full_body_end,
        "full-height paint has blank room after its last content row"
    );
    assert!(
        (last_content_y.saturating_add(1)..full_body_end).all(|y| painted_detail_row(
            &full, full_body, y
        )
        .trim()
        .is_empty()),
        "independent full paint is complete, not clipped at its canvas edge"
    );
    let painted_screen = manager.usage.screen.as_ref().expect("Usage screen");
    if painted_screen
        .notice
        .as_deref()
        .is_some_and(|notice| notice.contains("TAIL-END"))
        && !(painted_screen.accounts.is_empty() && painted_screen.loading())
    {
        assert!(
            painted_detail_row(&full, full_body, last_content_y).contains("TAIL-END"),
            "a final notice marker is the actual last content row"
        );
    }
    if painted_screen.accounts.is_empty() && painted_screen.loading() {
        assert!(
            painted_detail_row(&full, full_body, last_content_y).contains("Press R to refresh."),
            "loading-empty paint counts the actual final refresh instruction"
        );
    }
    let rendered_lines = usize::from(last_content_y.saturating_sub(full_body.y)) + 1;
    let expected_max = rendered_lines
        .saturating_sub(usize::from(body.height))
        .min(usize::from(u16::MAX)) as u16;
    assert_eq!(receipt.detail_scroll_bounds(), (expected_max, expected_max));

    for row in 0..body.height {
        let actual_y = body.y + row;
        let source_y = full_body.y + expected_max + row;
        assert_eq!(
            painted_detail_row(&actual, body, actual_y),
            painted_detail_row(&full, full_body, source_y),
            "actual tail row {row} matches independently painted full content"
        );
    }
    expected_max
}

fn change_structural_field(screen: &mut UsageScreenState, field: usize) {
    match field {
        0 => screen.accounts[0].account.push('!'),
        1 => screen.canonical_projection = Some(metric_group_projection_fixture().0),
        2 => screen.selected = 1,
        3 => screen.selected_id = Some("provider:b".to_owned()),
        4 => screen.detail = true,
        5 => screen.sort = UsageSort::Name,
        6 => screen.filter = UsageFilter::Issues,
        7 => screen.scroll = 1,
        8 => screen.notice = Some("changed notice".to_owned()),
        9 => screen.generated_at_epoch = Some(TEST_NOW_EPOCH),
        10 => screen.projection_issues = metric_group_projection_fixture().0.issues,
        11 => screen.refresh_due = true,
        12 => screen.force_refresh_pending = true,
        13 => screen.refresh_generation = 1,
        14 => screen.last_refresh_at = Some(Instant::now()),
        15 => screen.list_scroll = 1,
        _ => unreachable!("fixture field index"),
    }
}

#[test]
fn t03_structural_revision_covers_public_usage_inputs() {
    for field in 0..16 {
        assert_revision_change_rejected(|screen| change_structural_field(screen, field));
    }
}

#[test]
fn t03_refresh_revision_preserves_live_subscription() {
    let area = Rect::new(0, 0, 120, 40);
    let manager = s8_manager(route_screen());
    let (_, receipt_without_subscription) = painted_receipt(&manager, area);
    let mut screen = manager.usage.screen.as_ref().unwrap().clone();
    screen.refresh_rx = Some(Box::new(crate::tui::runtime::ready_blocking_subscription(
        (7, Ok(UsageScreenState::default())),
    )));
    assert!(!receipt_without_subscription.matches(area, &screen));

    let manager = s8_manager(screen);
    let (_, receipt_with_subscription) = painted_receipt(&manager, area);
    let live = manager.usage.screen.as_ref().unwrap();
    assert!(live.refresh_rx.is_some());
    assert!(receipt_with_subscription.matches(area, live));
    let cloned = live.clone();
    assert!(cloned.refresh_rx.is_none());
    assert!(!receipt_with_subscription.matches(area, &cloned));
}

#[test]
fn t04_detail_scroll_geometry_matches_wrapped_paint_and_clamps_stale_offsets() {
    let mut long = route_screen();
    long.detail = true;
    long.selected = 1;
    long.selected_id = Some("provider:a".to_owned());
    long.accounts[0].account = format!("Alpha 界e\u{301}{}", "wrapped identity ".repeat(4));
    long.accounts[0].windows = (0..32)
        .map(|index| {
            let mut window = test_window(&format!("window-{index}-界"), Some(73));
            window.value = format!("value-{index} 界e\u{301}{}", "CJK detail ".repeat(5));
            window
        })
        .collect();
    long.notice = Some(format!(
        "last diagnostic {} TAIL-END",
        "界wrapped ".repeat(18)
    ));

    let narrow_max = assert_detail_scroll_matches_full_paint(long.clone(), Rect::new(0, 0, 63, 24));
    let wide_max = assert_detail_scroll_matches_full_paint(long.clone(), Rect::new(0, 0, 120, 40));
    assert!(narrow_max > 0 && wide_max > 0);
    assert_ne!(
        narrow_max, wide_max,
        "width and viewport size affect the painted bound"
    );

    let mut summary = long.clone();
    summary.detail = false;
    assert_detail_scroll_matches_full_paint(summary, Rect::new(0, 0, 120, 40));

    let mut overview = long.clone();
    overview.selected = 0;
    overview.selected_id = None;
    overview.detail = false;
    assert_detail_scroll_matches_full_paint(overview, Rect::new(0, 0, 80, 24));

    let mut filtered = long.clone();
    filtered.filter = UsageFilter::Stale;
    assert!(filtered.visible_order().is_empty());
    assert_detail_scroll_matches_full_paint(filtered, Rect::new(0, 0, 80, 24));

    let mut loading_empty = UsageScreenState {
        refresh_due: true,
        notice: Some("empty loading diagnostic TAIL-END".to_owned()),
        ..UsageScreenState::default()
    };
    loading_empty.scroll = u16::MAX;
    assert_detail_scroll_matches_full_paint(loading_empty, Rect::new(0, 0, 80, 24));

    let mut all_fit = route_screen();
    all_fit.accounts.truncate(1);
    all_fit.notice = Some("short all-fit diagnostic TAIL-END".to_owned());
    assert_eq!(
        assert_detail_scroll_matches_full_paint(all_fit, Rect::new(0, 0, 120, 80)),
        0
    );

    let mut shrunk = long;
    shrunk.accounts[0].windows.clear();
    shrunk.notice = Some("short after content shrink".to_owned());
    assert_eq!(
        assert_detail_scroll_matches_full_paint(shrunk, Rect::new(0, 0, 120, 80)),
        0,
        "stale raw MAX paints at zero after content shrinks below the viewport"
    );

    let zero_body = assert_detail_scroll_matches_full_paint(route_screen(), Rect::new(0, 0, 40, 3));
    assert_eq!(zero_body, 0);
}

#[test]
fn t04_independent_pane_offsets_survive_clone_and_reset_on_view_changes() {
    let mut screen = route_screen();
    screen.scroll = 17;
    screen.list_scroll = 29;
    let cloned = screen.clone();
    assert_eq!(cloned, screen);
    assert_eq!(cloned.scroll, 17);
    assert_eq!(cloned.list_scroll, 29);

    screen.reanchor_after_view_change();
    assert_eq!(screen.scroll, 0);
    assert_eq!(screen.list_scroll, 0);
    screen.scroll = 17;
    screen.list_scroll = 29;
    assert!(screen.jump_to_most_constrained());
    assert_eq!(screen.scroll, 0);
    assert_eq!(screen.list_scroll, 0);

    screen.scroll = 17;
    screen.list_scroll = 29;
    screen.apply_refresh(route_screen(), Instant::now());
    assert_eq!(screen.scroll, 0);
    assert_eq!(screen.list_scroll, 0);
}

#[test]
fn t04_page_keys_keep_both_pane_scrolls_reachable() {
    let mut screen = route_screen();
    screen.scroll = 7;
    screen.list_scroll = 9;
    let mut manager = s8_manager(screen);

    handle_key(
        &mut manager,
        KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
    );
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!((screen.scroll, screen.list_scroll), (12, 14));

    handle_key(
        &mut manager,
        KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
    );
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!((screen.scroll, screen.list_scroll), (7, 9));

    {
        let screen = manager.usage.screen.as_mut().unwrap();
        screen.scroll = 2;
        screen.list_scroll = 3;
    }
    handle_key(
        &mut manager,
        KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
    );
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!((screen.scroll, screen.list_scroll), (0, 0));
}

#[test]
fn t03_paint_receipt_cannot_authorize_unpainted_geometry() {
    let area = Rect::new(0, 0, 120, 40);
    let mut manager = s8_manager(route_screen());
    let screen = manager.usage.screen.as_ref().unwrap();
    let layout = crate::tui::screens::usage::usage_frame_layout(area);
    let unpainted = list_geometry(screen, layout.list_inner, TEST_NOW_EPOCH, true);

    let (buffer, receipt) = painted_receipt(&manager, area);
    let account_row = painted_row_y(&buffer, area, "Alpha account");
    assert!(
        unpainted
            .target_at(layout.list_inner.x, account_row, screen.list_scroll)
            .is_some()
    );
    assert!(
        manager
            .usage
            .painted_row_target(area, layout.list_inner.x, account_row)
            .is_none()
    );

    manager
        .usage
        .publish_painted_pointer(area, Some(receipt.clone()));
    assert!(
        manager
            .usage
            .painted_row_target(area, layout.list_inner.x, account_row)
            .is_some()
    );

    manager
        .usage
        .publish_painted_pointer(Rect::new(0, 0, 100, 30), Some(receipt));
    assert!(
        manager
            .usage
            .painted_row_target(Rect::new(0, 0, 100, 30), layout.list_inner.x, account_row)
            .is_none()
    );

    let clipped_area = Rect::new(0, 0, 80, 30);
    let mut small_terminal = Terminal::new(TestBackend::new(60, 20)).expect("small terminal");
    let mut clipped_receipt = None;
    small_terminal
        .draw(|frame| {
            clipped_receipt = crate::tui::screens::usage::render_at_with_pointer_receipt(
                frame,
                clipped_area,
                &manager,
                TEST_NOW_EPOCH,
            );
        })
        .expect("clipped TestBackend draw remains safe");
    assert!(clipped_receipt.is_none());
}
