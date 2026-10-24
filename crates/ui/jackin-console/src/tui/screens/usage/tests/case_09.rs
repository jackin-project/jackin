// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::screens::usage::list_geometry;
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
        _ => unreachable!("fixture field index"),
    }
}

#[test]
fn t03_structural_revision_covers_public_usage_inputs() {
    for field in 0..15 {
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
            .target_at(layout.list_inner.x, account_row, screen.scroll)
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
