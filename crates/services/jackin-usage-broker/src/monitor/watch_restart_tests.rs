// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorConfig, MonitorIssueCode, MonitorOperation, MonitorProvider, MonitorReply,
    SpendRecordInput, SpendRecordSource, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION,
};

use super::MonitorStore;

const NOW: i64 = 1_800_000_000;

fn observation() -> StatuslineObservation {
    let quota = Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(1_000),
        reset_at_epoch: Some(NOW + 3_600),
    });
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: "session-watch-restart".to_owned(),
        model: None,
        rate_limits: StatuslineRateLimits {
            five_hour: quota.clone(),
            seven_day: quota,
        },
    }
}

#[test]
fn fresh_watch_after_expired_restart_returns_only_the_reconciled_current_event() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    store
        .operate(
            MonitorOperation::Ingest {
                account_id: "acct-watch-restart".to_owned(),
                observation: observation(),
            },
            NOW,
        )
        .expect("ingest fresh quota evidence");
    store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: "acct-watch-restart".to_owned(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(NOW),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            NOW,
        )
        .expect("record a current SGD baseline");
    let started = store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-watch-restart".to_owned(),
                    goal_id: "goal-watch-restart".to_owned(),
                    session_id: None,
                    expected_model: None,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                },
            },
            NOW,
        )
        .expect("start monitor");
    let MonitorReply::Started { status } = started else {
        panic!("expected started status");
    };
    assert!(
        status.runnable,
        "fixture must begin with a runnable decision"
    );
    let monitor_id = status.monitor_id.clone();
    drop(store);

    // Reopening the state after its source evidence expired must reconcile and
    // persist the current blocked state before a new watcher receives anything.
    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    let reply = reopened
        .operate(
            MonitorOperation::Watch {
                monitor_id,
                after_sequence: 0,
                timeout_ms: 0,
            },
            NOW + 301,
        )
        .expect("watch reconciled current state");
    let MonitorReply::Watch {
        events,
        next_sequence,
        timed_out,
    } = reply
    else {
        panic!("expected watch response");
    };

    assert!(!timed_out);
    assert_eq!(events.len(), 1, "fresh attach must not replay old history");
    let current = &events[0];
    assert_eq!(current.sequence, next_sequence);
    assert!(!current.status.runnable);
    assert!(current.status.issues.iter().any(|issue| {
        matches!(
            issue.code,
            MonitorIssueCode::QuotaStale | MonitorIssueCode::SpendStale
        )
    }));

    let persisted = reopened
        .operate(
            MonitorOperation::Status {
                monitor_id: current.status.monitor_id.clone(),
            },
            NOW + 301,
        )
        .expect("read persisted current status");
    let MonitorReply::Status { status } = persisted else {
        panic!("expected status response");
    };
    assert!(!status.runnable);
    assert_eq!(status.latest_decision, current.status.latest_decision);
}
