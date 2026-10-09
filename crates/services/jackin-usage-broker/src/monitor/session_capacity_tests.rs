// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_monitor::{
    MonitorConfig, MonitorIssueCode, MonitorOperation, MonitorProvider, MonitorReply,
    StatuslineObservation, StatuslineQuotaWindow, StatuslineRateLimits,
    USAGE_MONITOR_SCHEMA_VERSION,
};

use super::MonitorStore;

const NOW: i64 = 1_800_000_000;

fn observation(session_id: &str, used: i32, reset_at_epoch: i64) -> StatuslineObservation {
    let window = Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(used),
        reset_at_epoch: Some(reset_at_epoch),
    });
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: None,
        rate_limits: StatuslineRateLimits {
            five_hour: window.clone(),
            seven_day: window,
        },
    }
}

fn ingest(
    store: &MonitorStore,
    account_id: &str,
    observation: StatuslineObservation,
    now_epoch: i64,
) -> Result<MonitorReply, jackin_protocol::usage_monitor::MonitorIssue> {
    store.operate(
        MonitorOperation::Ingest {
            account_id: account_id.to_owned(),
            observation,
        },
        now_epoch,
    )
}

#[test]
fn stale_sessions_are_pruned_without_lowering_the_account_reset_watermark() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let reset = NOW + 3_600;

    let invalid = ingest(
        &store,
        "acct-session-cap",
        observation("poison-reset", 10_000, NOW + 5 * 60 * 60 + 301),
        NOW,
    )
    .expect_err("implausible reset is rejected before touching the watermark");
    assert_eq!(invalid.code, MonitorIssueCode::StatuslineInvalid);

    for index in 0..16 {
        ingest(
            &store,
            "acct-session-cap",
            observation(&format!("stale-{index}"), 8_000, reset),
            NOW,
        )
        .expect("ingest within the session cap");
    }

    // A unique seventeenth session arrives after all prior unbound sessions
    // expired. The old reset watermark must survive pruning and reject its
    // stale lower reset instead of turning that row into quota evidence.
    let stale_reset = observation("session-new", 1_000, reset - 1);
    ingest(&store, "acct-session-cap", stale_reset.clone(), NOW + 301)
        .expect("prune stale sessions before enforcing the cap");
    ingest(&store, "acct-session-cap", stale_reset, NOW + 302)
        .expect("repeated stale reset remains rejected");

    let started = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-session-cap".to_owned(),
                    goal_id: "goal-session-cap".to_owned(),
                    session_id: None,
                    expected_model: None,
                    budget: None,
                },
            },
            NOW + 302,
        )
        .expect("start account-scoped monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    };
    assert_eq!(started.five_hour.used_percentage_basis_points, None);
    assert!(
        started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaUnknown)
    );
    assert!(!started.runnable);
}
