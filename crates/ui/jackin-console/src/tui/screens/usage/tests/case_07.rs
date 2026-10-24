// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crossterm::event::KeyCode;
use jackin_protocol::usage_broker::UsageProjectionRefreshStateV1;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

const REFRESHING: &str = "Refreshing usage…";

fn pending_state(
    accounts: Vec<UsageAccount>,
    selected: usize,
    detail: bool,
    scroll: u16,
) -> UsageScreenState {
    let selected_id = selected
        .checked_sub(1)
        .and_then(|index| accounts.get(index))
        .map(|account| format!("{}:{}", account.provider_id, account.canonical_account_id));
    let mut state = UsageScreenState {
        accounts,
        selected,
        selected_id,
        detail,
        scroll,
        ..UsageScreenState::default()
    };
    let response = snapshot(state.accounts.clone(), state.notice.clone());
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        state.refresh_generation,
        Ok(response),
    )));
    state
}

fn route_state(route: &str, scroll: u16) -> UsageScreenState {
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();

    match route {
        "overview" => pending_state(vec![account], 0, false, scroll),
        "account summary" => pending_state(vec![account], 1, false, scroll),
        "full detail" => pending_state(vec![account], 1, true, scroll),
        "filtered empty" => {
            let mut state = pending_state(vec![account], 0, false, scroll);
            state.filter = UsageFilter::Stale;
            state
        }
        "canonical only" => {
            let (mut projection, _) = metric_group_projection_fixture();
            projection.providers.clear();
            projection.issues.clear();
            projection.refresh_state = UsageProjectionRefreshStateV1::Refreshing;
            let mut state = UsageScreenState::from_projection(&projection);
            state.scroll = scroll;
            state
        }
        "empty loading" => UsageScreenState {
            refresh_due: true,
            scroll,
            ..UsageScreenState::default()
        },
        "recoverable error retry" => {
            let mut state = UsageScreenState {
                accounts: vec![account],
                selected: 1,
                selected_id: Some("openai:work-id".to_owned()),
                scroll,
                ..UsageScreenState::default()
            };
            state.apply_refresh_error("Usage unavailable: disconnected".to_owned(), Instant::now());
            let response = snapshot(state.accounts.clone(), state.notice.clone());
            state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
                state.refresh_generation,
                Ok(response),
            )));
            state
        }
        _ => panic!("unknown Usage route {route}"),
    }
}

fn render_workspace(state: UsageScreenState, width: u16, height: u16) -> String {
    let mut manager = manager_with_usage(state);
    manager.usage.visible = true;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_at(frame, frame.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    backend_text(&terminal)
}

fn render_detail_grid(
    manager: &crate::tui::state::ManagerState<'_>,
    width: u16,
    height: u16,
) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_detail(frame, frame.area(), manager, TEST_NOW_EPOCH))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn inner_row_text(buffer: &Buffer, y: u16) -> String {
    (1..buffer.area.width.saturating_sub(1))
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn physical_row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn backend_text_from_buffer(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| physical_row_text(buffer, y))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn t01_refresh_progress_remains_visible_in_every_view_scroll_and_supported_size() {
    for route in [
        "overview",
        "account summary",
        "full detail",
        "filtered empty",
        "canonical only",
        "empty loading",
        "recoverable error retry",
    ] {
        for scroll in [0, 5, u16::MAX] {
            for (width, height) in [(40, 20), (80, 24), (120, 30)] {
                let text = render_workspace(route_state(route, scroll), width, height);
                assert_eq!(
                    text.matches(REFRESHING).count(),
                    1,
                    "expected one sticky progress row for {route}, scroll={scroll}, size={width}x{height}:\n{text}"
                );
            }
        }
    }

    let retry_text = render_workspace(route_state("recoverable error retry", 0), 120, 60);
    assert_eq!(retry_text.matches(REFRESHING).count(), 1, "{retry_text}");
    assert!(
        retry_text.contains("Usage unavailable: disconnected"),
        "retry keeps the previous failure visible in the reachable body:\n{retry_text}"
    );
}

#[test]
fn t01_page_down_and_enter_keep_refresh_progress_and_selected_inventory() {
    let mut first = test_account("openai", "work-id", "work");
    first.provider = "OpenAI".to_owned();
    let mut second = test_account("anthropic", "home-id", "home");
    second.provider = "Anthropic".to_owned();
    let accounts = vec![first, second];
    let mut manager = manager_with_usage(pending_state(accounts.clone(), 1, false, 0));
    manager.usage.visible = true;

    press_key(&mut manager, KeyCode::PageDown);
    press_key(&mut manager, KeyCode::Enter);

    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.scroll, 5);
    assert!(screen.detail);
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id.as_deref(), Some("openai:work-id"));
    assert_eq!(screen.accounts, accounts);
    assert!(screen.refresh_in_flight());

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_at(frame, frame.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    let text = backend_text(&terminal);
    assert_eq!(text.matches(REFRESHING).count(), 1, "{text}");
}

#[test]
fn t01_completion_clears_reserved_status_without_moving_account_body() {
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    let idle = UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:work-id".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    };
    let mut active = idle.clone();
    let response = snapshot(active.accounts.clone(), active.notice.clone());
    active.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        active.refresh_generation,
        Ok(response),
    )));

    let mut manager = manager_with_usage(active);
    assert!(
        manager
            .usage
            .screen
            .as_ref()
            .expect("Usage screen is present")
            .refresh_in_flight()
    );
    let active_buffer = render_detail_grid(&manager, 120, 60);
    assert_eq!(inner_row_text(&active_buffer, 1).trim(), REFRESHING);
    assert_eq!(active_buffer[(0, 1)].symbol(), "│");
    assert_eq!(active_buffer[(119, 1)].symbol(), "│");

    let screen = manager
        .usage
        .screen
        .as_mut()
        .expect("Usage screen is present");
    let completed = screen
        .poll_refresh()
        .expect("refresh subscription is present")
        .expect("refresh succeeds");
    screen.apply_refresh(completed, Instant::now());
    assert!(!screen.refresh_in_flight());
    let idle_buffer = render_detail_grid(&manager, 120, 60);

    assert!(inner_row_text(&idle_buffer, 1).trim().is_empty());
    assert_eq!(idle_buffer[(0, 1)].symbol(), "│");
    assert_eq!(idle_buffer[(119, 1)].symbol(), "│");
    for y in 2..60 {
        assert_eq!(
            physical_row_text(&active_buffer, y),
            physical_row_text(&idle_buffer, y)
        );
    }
    let idle_text = backend_text_from_buffer(&idle_buffer);
    assert!(idle_text.contains("OpenAI"));
    assert!(idle_text.contains("work"));
}

#[test]
fn t01_one_inner_row_prioritizes_progress_and_keeps_border_geometry() {
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    let manager = manager_with_usage(pending_state(vec![account], 0, false, 0));
    let backend = TestBackend::new(80, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_detail(frame, frame.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    let text = backend_text(&terminal);
    let rows = text.lines().collect::<Vec<_>>();

    assert_eq!(rows.len(), 3, "{text}");
    assert!(rows[0].contains("Overview"), "{text}");
    assert!(rows[1].contains(REFRESHING), "{text}");
    assert!(rows[2].contains('└'), "{text}");
    assert_eq!(text.matches(REFRESHING).count(), 1, "{text}");
}

#[test]
fn t01_zero_area_and_resize_recovery_keep_progress_and_inventory() {
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    let state = pending_state(vec![account], 1, true, 0);
    let manager = manager_with_usage(state);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_detail(
                frame,
                Rect::new(frame.area().x, frame.area().y, 0, 0),
                &manager,
                TEST_NOW_EPOCH,
            );
        })
        .unwrap();
    terminal
        .draw(|frame| {
            render_detail(
                frame,
                Rect::new(frame.area().x, frame.area().y, 2, 2),
                &manager,
                TEST_NOW_EPOCH,
            );
        })
        .unwrap();
    terminal
        .draw(|frame| render_detail(frame, frame.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();

    let text = backend_text(&terminal);
    assert_eq!(text.matches(REFRESHING).count(), 1, "{text}");
    assert!(text.contains("OpenAI"), "{text}");
    assert!(text.contains("work"), "{text}");
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.selected_id.as_deref(), Some("openai:work-id"));
    assert!(screen.refresh_in_flight());
}
