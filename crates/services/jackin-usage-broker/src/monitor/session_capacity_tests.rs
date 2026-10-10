// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorDispatchReadiness, MonitorIssueCode,
    MonitorOperation, MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
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
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: window.clone(),
            seven_day: window,
        },
    }
}

fn ingest(
    store: &MonitorStore,
    binding_id: &str,
    binding_revision: u64,
    observation: StatuslineObservation,
    now_epoch: i64,
) -> Result<MonitorReply, jackin_protocol::usage_monitor::MonitorIssue> {
    store.operate(
        MonitorOperation::Ingest {
            scope: MonitorScope::BoundAccount {
                binding_id: binding_id.to_owned(),
                binding_revision,
                session_id: None,
            },
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
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-session-cap".to_owned(),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind isolated test account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };

    let invalid = ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        observation("poison-reset", 10_000, NOW + 5 * 60 * 60 + 301),
        NOW,
    )
    .expect_err("implausible reset is rejected before touching the watermark");
    assert_eq!(invalid.code, MonitorIssueCode::StatuslineInvalid);

    for index in 0..16 {
        ingest(
            &store,
            &binding.binding_id,
            binding.revision,
            observation(&format!("stale-{index}"), 8_000, reset),
            NOW,
        )
        .expect("ingest within the session cap");
    }

    // A unique seventeenth session arrives after all prior account sessions
    // expired. The old reset watermark must survive pruning and reject its
    // stale lower reset instead of turning that row into quota evidence.
    let stale_reset = observation("session-new", 1_000, reset - 1);
    ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        stale_reset.clone(),
        NOW + 301,
    )
    .expect("prune stale sessions before enforcing the cap");
    ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        stale_reset,
        NOW + 302,
    )
    .expect("repeated stale reset remains rejected");

    let started = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::ObserveOnly,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id,
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: None,
                    expected_model: None,
                    policy_revision: None,
                },
                idempotency_key: "fixture-session-cap-observer".to_owned(),
            },
            NOW + 302,
        )
        .expect("start account-scoped monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    };
    assert_eq!(started.five_hour.used_percentage_basis_points, None);
    assert_eq!(started.purpose, MonitorPurpose::ObserveOnly);
    assert_eq!(started.goal_id, None);
    assert_eq!(started.policy, None);
    assert_eq!(started.cumulative_goal_spend, None);
    assert_eq!(
        started.readiness.dispatch,
        MonitorDispatchReadiness::NotAuthorized
    );
    if let Some(decision) = &started.latest_decision {
        assert!(decision.actions.is_empty());
    }
    assert!(
        started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaUnknown)
    );
    assert!(!started.runnable);
}
