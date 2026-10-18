// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn policy_args(
    policy: UsageMonitorPolicyArg,
    budget_sgd: Option<&str>,
    acknowledge_no_sgd_cap: bool,
) -> UsagePolicyApproveArgs {
    UsagePolicyApproveArgs {
        binding: "binding-1".to_owned(),
        binding_revision: 3,
        goal: "goal-1".to_owned(),
        policy,
        budget_sgd: budget_sgd.map(str::to_owned),
        operator_label: "operator".to_owned(),
        confirm: true,
        acknowledge_no_sgd_cap,
        expected_revision: None,
    }
}

#[test]
fn operator_terminal_gate_fails_closed_when_any_stream_is_headless() {
    for streams in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
        (false, false, false),
    ] {
        let error = require_operator_terminal(streams.0, streams.1, streams.2).unwrap_err();
        let exit = error.downcast_ref::<UsageCommandExit>().unwrap();
        assert_eq!(exit.exit_code(), 2);
        let value: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
        assert_eq!(value["error"]["code"], "interaction_required");
    }
    require_operator_terminal(true, true, true).unwrap();
}

#[test]
fn monitor_issues_have_stable_blocked_and_invalid_exit_classes() {
    for code in [
        MonitorIssueCode::BudgetUnverifiable,
        MonitorIssueCode::SpendUnavailable,
        MonitorIssueCode::SpendStale,
        MonitorIssueCode::SpendUnverified,
        MonitorIssueCode::PolicyRequired,
        MonitorIssueCode::BindingRequired,
        MonitorIssueCode::SgdCapAcknowledgementRequired,
        MonitorIssueCode::InteractionRequired,
    ] {
        let error = issue_error(
            MonitorIssue {
                code,
                message: "blocked".to_owned(),
                retry_at_epoch: None,
            },
            3,
        );
        assert_eq!(
            error
                .downcast_ref::<UsageCommandExit>()
                .unwrap()
                .exit_code(),
            2,
            "{code:?}"
        );
    }

    for code in [
        MonitorIssueCode::BrokerUnavailable,
        MonitorIssueCode::StatuslineInvalid,
        MonitorIssueCode::IdempotencyConflict,
        MonitorIssueCode::BindingMismatch,
    ] {
        let error = issue_error(
            MonitorIssue {
                code,
                message: "invalid or unavailable".to_owned(),
                retry_at_epoch: None,
            },
            3,
        );
        assert_eq!(
            error
                .downcast_ref::<UsageCommandExit>()
                .unwrap()
                .exit_code(),
            3,
            "{code:?}"
        );
    }
}

#[test]
fn observer_start_reports_tracking_success_without_authorizing_dispatch() {
    assert_eq!(
        monitor_status_exit_code(
            MonitorPurpose::ObserveOnly,
            false,
            MonitorTrackingReadiness::Waiting,
            true,
        ),
        0
    );
    assert_eq!(
        monitor_status_exit_code(
            MonitorPurpose::ObserveOnly,
            false,
            MonitorTrackingReadiness::Unavailable,
            true,
        ),
        3
    );
    assert_eq!(
        monitor_status_exit_code(
            MonitorPurpose::ObserveOnly,
            false,
            MonitorTrackingReadiness::Ready,
            false,
        ),
        2
    );
    assert_eq!(
        monitor_status_exit_code(
            MonitorPurpose::DispatchGuard,
            false,
            MonitorTrackingReadiness::Ready,
            true,
        ),
        2
    );
    assert_eq!(
        monitor_status_exit_code(
            MonitorPurpose::DispatchGuard,
            true,
            MonitorTrackingReadiness::Ready,
            false,
        ),
        0
    );
}

#[test]
fn policy_approval_defaults_strict_sgd_and_never_infers_quota_only() {
    let strict =
        policy_approval_input(&policy_args(UsageMonitorPolicyArg::StrictSgd, None, false)).unwrap();
    assert_eq!(strict.new_policy, MonitorPolicy::StrictSgd);
    assert_eq!(strict.budget, Some(Money::new(5_000, "SGD", 2)));
    assert!(!strict.acknowledge_no_sgd_cap);

    let quota_only =
        policy_approval_input(&policy_args(UsageMonitorPolicyArg::QuotaOnly, None, true)).unwrap();
    assert_eq!(quota_only.new_policy, MonitorPolicy::QuotaOnly);
    assert_eq!(quota_only.budget, None);
    assert!(quota_only.acknowledge_no_sgd_cap);

    let missing_ack =
        policy_approval_input(&policy_args(UsageMonitorPolicyArg::QuotaOnly, None, false))
            .unwrap_err();
    assert_eq!(
        missing_ack
            .downcast_ref::<UsageCommandExit>()
            .unwrap()
            .exit_code(),
        2
    );
}

#[test]
fn strict_sgd_budget_must_be_positive() {
    for value in ["0", "0.00", "00.00"] {
        let error = parse_sgd_budget(value).unwrap_err();
        let exit = error.downcast_ref::<UsageCommandExit>().unwrap();
        assert_eq!(exit.exit_code(), 3);
        let parsed: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
        assert_eq!(parsed["error"]["code"], "invalid_budget");
    }

    assert_eq!(parse_sgd_budget("0.01").unwrap(), Money::new(1, "SGD", 2));
}

#[test]
fn statusline_session_only_scope_comes_from_the_payload() {
    let scope = statusline_monitor_scope(
        &UsageStatuslineScopeArgs {
            session_only: true,
            binding: None,
            binding_revision: None,
        },
        "payload-session",
    )
    .unwrap();
    assert_eq!(
        scope,
        MonitorScope::Session {
            session_id: "payload-session".to_owned()
        }
    );
}
