// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn window_metric_group(
    label: &str,
    remaining: Option<u8>,
    now: i64,
) -> UsageMetricGroup {
    use jackin_protocol::usage_broker::{
        UsageMetricGroupKindV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
        UsagePercent, UsageQuotaStateV1,
    };
    UsageMetricGroup {
        group_id: format!("{label}-id"),
        rank: 0,
        kind: UsageMetricGroupKindV1::Window,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: now - 10,
        last_success_at_epoch: Some(now - 60),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: remaining.map(|p| UsagePercent::new(p).expect("valid percent")),
            remaining_raw_percent: remaining.map(i32::from),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

pub(super) fn backend_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn s8_press(
    state: &mut crate::tui::state::ManagerState<'_>,
    code: crossterm::event::KeyCode,
) {
    handle_key(
        state,
        crossterm::event::KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        },
    );
}

pub(super) fn s8_manager(screen: UsageScreenState) -> crate::tui::state::ManagerState<'static> {
    // `ManagerState::from_config` borrows nothing `'static`-blocking here: the
    // config outlives the call via the leaked box, matching how the console
    // owns config for the whole run.
    let config: &'static jackin_config::AppConfig =
        Box::leak(Box::new(jackin_config::AppConfig::default()));
    let mut manager =
        crate::tui::state::ManagerState::from_config(config, std::path::Path::new("/test"));
    manager.usage.screen = Some(screen);
    manager.usage.visible = true;
    manager
}

pub(super) fn s8_render_full(
    manager: &crate::tui::state::ManagerState<'_>,
    width: u16,
    height: u16,
) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_at(f, f.area(), manager, TEST_NOW_EPOCH))
        .unwrap();
    backend_text(&terminal)
}

pub(super) fn snapshot(accounts: Vec<UsageAccount>, notice: Option<String>) -> UsageScreenState {
    UsageScreenState {
        accounts,
        notice,
        ..UsageScreenState::default()
    }
}

pub(super) fn assert_manual_refresh_join_preserves_periodic_cadence(
    failed: bool,
    claim_before_completion: bool,
) {
    use crossterm::event::KeyCode;
    let (mut projection, _) = metric_group_projection_fixture();
    projection.providers[0].accounts[0].freshness.retry_at_epoch = Some(TEST_NOW_EPOCH + 900);
    let publication = UsageScreenState::from_projection(&projection);
    let started = Instant::now();
    let mut state = UsageScreenState::open_with_snapshot(publication.clone());
    let active = state.next_refresh_plan_if_due(started).expect("open cycle");
    assert!(!active.force);
    let result = if failed {
        Err("independent transport failure".to_owned())
    } else {
        Ok(publication)
    };
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        active.generation,
        result,
    )));
    let mut manager = manager_with_usage(state);
    press_key(&mut manager, KeyCode::Char('r'));
    let screen = manager.usage.screen.as_mut().unwrap();
    assert!(screen.refresh_due && screen.force_refresh_pending);
    if claim_before_completion {
        assert!(
            screen.next_refresh_plan_if_due(started).is_none(),
            "manual request joins active cycle"
        );
        assert_eq!(screen.refresh_generation, active.generation);
        assert!(
            !screen.refresh_due && !screen.force_refresh_pending,
            "joined intent is consumed together"
        );
    }
    // Production polls before claiming. A Ready receiver must consume a
    // manual request even when the in-flight claim branch never executes.
    let completed = started + Duration::from_secs(1);
    let outcome = screen.poll_refresh().expect("ready active generation");
    match outcome {
        Ok(snapshot) => screen.apply_refresh(snapshot, completed),
        Err(notice) => screen.apply_refresh_error(notice, completed),
    }
    assert!(!screen.refresh_in_flight());
    assert_eq!(screen.refresh_generation, active.generation);
    assert!(
        screen.next_refresh_plan_if_due(completed).is_none(),
        "joining never queues another cycle"
    );
    assert!(
        screen
            .next_refresh_plan_if_due(
                (completed + USAGE_HEARTBEAT_INTERVAL)
                    .checked_sub(Duration::from_nanos(1))
                    .expect("heartbeat boundary admits one nanosecond")
            )
            .is_none(),
        "heartbeat cadence is preserved"
    );
    assert_eq!(screen.canonical_projection.as_ref(), Some(&projection));
    assert_eq!(
        screen.accounts[0].retry_at_epoch,
        Some(TEST_NOW_EPOCH + 900)
    );
    let periodic = screen
        .next_refresh_plan_if_due(completed + USAGE_HEARTBEAT_INTERVAL)
        .expect("next periodic cycle");
    assert!(
        !periodic.force,
        "joined manual intent cannot bypass broker backoff on a later heartbeat"
    );
    assert_eq!(periodic.generation, active.generation + 1);
    assert!(!screen.refresh_due && !screen.force_refresh_pending);
}
