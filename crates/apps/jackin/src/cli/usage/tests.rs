// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CanonicalUsedPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    Money, PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, UsageSeverity,
    UsageSnapshotStatus,
};
use jackin_protocol::usage_monitor::SpendRecordSource;

#[test]
fn canonical_overview() {
    assert_eq!(
        normalize_usage_provider_label("OpenAI / Codex"),
        "openaicodex"
    );
    assert_eq!(normalize_usage_provider_label("Z.AI"), "zai");
}

#[test]
fn formats_account_usage_with_shared_unit() {
    let account = AccountUsageSnapshotView {
        provider: "codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        source: "codex-rpc".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "session".to_owned(),
        used_amount: Some(37),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: None,
        fetched_at: 0,
        expires_at: None,
        status: "fresh".to_owned(),
        last_error: None,
        quota_bucket: None,
        account_capability: None,
    };

    assert_eq!(usage_amount_label(&account), "37/100 percent");
}

#[test]
fn typed_percent_payload_keeps_fractional_points_and_source() {
    let mut row = account("Codex", "fresh", "provider_api", "authoritative");
    row.quota_bucket.as_mut().unwrap().used_percent =
        Some(CanonicalUsedPercent::new(37.375).unwrap());

    assert_eq!(usage_amount_label(&row), "37.38% used (reported)");
}

#[test]
fn usage_verify_requires_fresh_valid_quantity_or_explicit_provider_state() {
    let mut row = account("Cursor", "fresh", "provider_api", "authoritative");
    let bucket = row.quota_bucket.as_mut().unwrap();
    bucket.used_percent = None;
    bucket.used_percent_origin = None;
    bucket.count_quota = Some(CountQuota::new(
        Some(0),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    assert!(verify::usage_row_proves_live_quota(&row));

    let mut empty = row.clone();
    empty.quota_bucket.as_mut().unwrap().count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    assert!(!verify::usage_row_proves_live_quota(&empty));

    let mut reset_only = empty.clone();
    let bucket = reset_only.quota_bucket.as_mut().unwrap();
    bucket.count_quota = None;
    bucket.status = UsageSnapshotStatus::Fresh;
    bucket.resets_at = Some(1_800_000_000);
    bucket.reset_label = Some("resets in 5m".to_owned());
    assert!(!verify::usage_row_proves_live_quota(&reset_only));

    reset_only.quota_bucket.as_mut().unwrap().status = UsageSnapshotStatus::Unavailable;
    assert!(!verify::usage_row_proves_live_quota(&reset_only));

    let mut missing_bucket = reset_only.clone();
    missing_bucket.quota_bucket = None;
    assert!(!verify::usage_row_proves_live_quota(&missing_bucket));

    let mut state_only = reset_only.clone();
    let bucket = state_only.quota_bucket.as_mut().unwrap();
    bucket.status = UsageSnapshotStatus::Fresh;
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    assert!(verify::usage_row_proves_live_quota(&state_only));
    state_only.quota_bucket.as_mut().unwrap().status = UsageSnapshotStatus::Stale;
    assert!(!verify::usage_row_proves_live_quota(&state_only));
    state_only.quota_bucket.as_mut().unwrap().status = UsageSnapshotStatus::Fresh;
    state_only.status = "stale".to_owned();
    assert!(!verify::usage_row_proves_live_quota(&state_only));

    let mut money = reset_only.clone();
    money.status = "fresh".to_owned();
    let bucket = money.quota_bucket.as_mut().unwrap();
    bucket.status = UsageSnapshotStatus::Fresh;
    bucket.used_money = Some(Money::new(1_200, "USD", 2));
    bucket.limit_money = Some(Money::new(5_000, "USD", 2));
    assert!(verify::usage_row_proves_live_quota(&money));

    let mut incomplete_money = money.clone();
    incomplete_money.quota_bucket.as_mut().unwrap().limit_money = None;
    assert!(!verify::usage_row_proves_live_quota(&incomplete_money));

    let mut mixed = row.clone();
    let bucket = mixed.quota_bucket.as_mut().unwrap();
    bucket.used_money = Some(Money::new(1, "USD", 2));
    bucket.limit_money = Some(Money::new(2, "USD", 2));
    assert!(!verify::usage_row_proves_live_quota(&mixed));

    let mut invalid_percent = row;
    let bucket = invalid_percent.quota_bucket.as_mut().unwrap();
    bucket.count_quota = None;
    bucket.used_percent = Some(CanonicalUsedPercent::new(1.0).unwrap());
    bucket.used_percent_origin = None;
    assert!(!verify::usage_row_proves_live_quota(&invalid_percent));
}

fn account(
    provider: &str,
    status: &str,
    source: &str,
    confidence: &str,
) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: provider.to_owned(),
        account_label: format!("{provider} account"),
        source: source.to_owned(),
        confidence: confidence.to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(37),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: None,
        fetched_at: 1_781_185_680,
        expires_at: None,
        status: status.to_owned(),
        last_error: None,
        quota_bucket: Some(QuotaBucketView {
            label: "Session".to_owned(),
            used_label: None,
            limit_label: None,
            used_percent: Some(CanonicalUsedPercent::new(37.0).unwrap()),
            used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            remaining_percent: None,
            remaining_percent_origin: None,
            count_quota: None,
            provider_quota_state: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }),
        account_capability: None,
    }
}

#[test]
fn typed_request_count_payload_wins_and_keeps_full_width() {
    let mut row = account("Cursor", "fresh", "provider_api", "authoritative");
    row.quota_bucket.as_mut().unwrap().count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(0),
        Some(7),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    assert_eq!(
        usage_amount_label(&row),
        format!(
            "{} used · 0 limit · 7 remaining requests · 37% used (reported)",
            u64::MAX
        )
    );
}

#[test]
fn explicit_provider_state_label_keeps_independent_count_and_percent_detail() {
    let mut row = account("MiniMax", "fresh", "provider_api", "authoritative");
    {
        let bucket = row.quota_bucket.as_mut().unwrap();
        bucket.used_percent = None;
        bucket.used_percent_origin = None;
        bucket.provider_quota_state = Some(ProviderQuotaState::Unlimited);
        bucket.count_quota = Some(CountQuota::new(
            Some(1),
            None,
            Some(0),
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
        bucket.remaining_percent =
            Some(jackin_protocol::control::CanonicalRemainingPercent::new(99.0).unwrap());
        bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    }

    assert_eq!(
        usage_amount_label(&row),
        "Unlimited · 1 used · 0 remaining count · 99% remaining (reported)"
    );

    {
        let bucket = row.quota_bucket.as_mut().unwrap();
        bucket.count_quota = None;
        bucket.remaining_percent = None;
        bucket.remaining_percent_origin = None;
    }
    assert_eq!(usage_amount_label(&row), "Unlimited");
}

#[test]
fn incomplete_typed_count_does_not_fall_back_to_legacy_amounts() {
    let mut row = account("Cursor", "fresh", "provider_api", "authoritative");
    row.quota_bucket.as_mut().unwrap().count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));

    assert_eq!(
        usage_amount_label(&row),
        "37% used (reported) · requests unavailable"
    );

    let bucket = row.quota_bucket.as_mut().unwrap();
    bucket.used_percent = None;
    bucket.used_percent_origin = None;
    assert_eq!(usage_amount_label(&row), "requests unavailable");
}

#[test]
fn usage_verify_accepts_trusted_rows_for_every_provider() {
    let accounts = [
        account("Codex", "fresh", "provider_api", "authoritative"),
        account("Claude", "fresh", "cli", "authoritative"),
        account("Amp", "fresh", "provider_api", "authoritative"),
        account("Grok Build", "fresh", "cli", "authoritative"),
        account("GLM / Z.AI", "fresh", "provider_api", "authoritative"),
        account("Kimi", "fresh", "provider_api", "authoritative"),
        account("MiniMax", "fresh", "provider_api", "authoritative"),
    ];

    let checks = verify_usage_accounts(&accounts);

    assert_eq!(checks.len(), 7);
    assert!(
        checks.iter().all(|check| check.status == "ok"),
        "{checks:?}"
    );
}

#[test]
fn usage_verify_reports_missing_and_untrusted_providers() {
    let mut untrusted = account("Codex", "needs_login", "none", "none");
    untrusted.account_label = "needs Codex login".to_owned();
    untrusted.last_error = Some("Codex auth not available".to_owned());
    let accounts = [
        untrusted,
        account("Amp", "fresh", "provider_api", "authoritative"),
    ];

    let checks = verify_usage_accounts(&accounts);

    let codex = checks
        .iter()
        .find(|check| check.label == "OpenAI")
        .expect("OpenAI check");
    assert_eq!(codex.status, "untrusted");
    assert!(
        codex
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("needs_login")),
        "{codex:?}"
    );
    let anthropic = checks
        .iter()
        .find(|check| check.label == "Anthropic")
        .expect("Anthropic check");
    assert_eq!(anthropic.status, "missing");
    let amp = checks
        .iter()
        .find(|check| check.label == "Amp")
        .expect("Amp check");
    assert_eq!(amp.status, "ok");
}

#[test]
fn truncates_long_values_with_ascii_ellipsis() {
    assert_eq!(truncate("abcdefghijkl", 8), "abcde...");
}

#[test]
fn budget_parser_uses_exact_sgd_minor_units() {
    assert_eq!(parse_sgd_budget("50").unwrap(), Money::new(5_000, "SGD", 2));
    assert_eq!(
        parse_sgd_budget("50.2").unwrap(),
        Money::new(5_020, "SGD", 2)
    );
    assert_eq!(
        parse_sgd_budget("50.25").unwrap(),
        Money::new(5_025, "SGD", 2)
    );
}

#[test]
fn budget_parser_rejects_rounding_and_negative_values() {
    for invalid in ["50.255", "-1", "1.2.3", "NaN", ""] {
        assert!(parse_sgd_budget(invalid).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn spend_file_is_bounded_typed_and_requires_explicit_verification() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("spend.json");
    std::fs::write(
        &path,
        r#"{
            "billing_period_start_epoch": 1800000000,
            "billing_period_end_epoch": 1802592000,
            "amount": {"amount_minor": 1234, "currency": "SGD", "exponent": 2},
            "evidence_at_epoch": 1800000123
        }"#,
    )
    .unwrap();

    let unverified = read_spend_record(&path, "account-1", false).unwrap();
    assert_eq!(unverified.account_id, "account-1");
    assert!(!unverified.verified);
    assert_eq!(unverified.source, SpendRecordSource::OperatorReceipt);
    let verified = read_spend_record(&path, "account-1", true).unwrap();
    assert!(verified.verified);

    std::fs::write(&path, "x".repeat(16 * 1024 + 1)).unwrap();
    let error = read_spend_record(&path, "account-1", true).unwrap_err();
    assert!(error.downcast_ref::<UsageCommandExit>().is_some());
}

#[test]
fn monitor_cli_errors_carry_stdout_json_and_exit_class() {
    let error = usage_error("broker_unavailable", "not running", 3);
    let exit = error.downcast_ref::<UsageCommandExit>().unwrap();
    assert_eq!(exit.exit_code(), 3);
    let value: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
    assert_eq!(value["error"]["code"], "broker_unavailable");
}

#[test]
fn doctor_treats_unknown_auth_as_informational() {
    let reply = MonitorReply::Doctor {
        report: jackin_protocol::usage_monitor::MonitorDoctorReport {
            provider: MonitorProvider::Claude,
            broker_available: true,
            statusline_ingress_supported: true,
            auth_state: jackin_protocol::usage_monitor::MonitorAuthState::Unknown,
            issues: vec![MonitorIssue {
                code: MonitorIssueCode::AuthStatusUnknown,
                message: "authentication was not inspected".to_owned(),
                retry_at_epoch: None,
            }],
        },
    };

    assert_eq!(doctor_exit_code(&reply), 0);
}

#[test]
fn watch_fresh_attach_uses_current_event_as_cursor_then_only_advances() {
    let fresh_attach_cursor = advance_watch_cursor(0, 17, [17]);
    assert_eq!(fresh_attach_cursor, 17);

    let live_cursor = advance_watch_cursor(fresh_attach_cursor, 19, [18, 19]);
    assert_eq!(live_cursor, 19);
    assert_eq!(advance_watch_cursor(live_cursor, 18, [16, 19]), 19);
}

#[test]
fn watch_batch_timeout_does_not_end_the_absolute_cli_deadline() {
    let started = Instant::now();
    let deadline = started + Duration::from_mins(5);
    let one_second_before_deadline = deadline
        .checked_sub(Duration::from_secs(1))
        .expect("five-minute deadline supports subtracting one second");
    let one_nanosecond_before_deadline = deadline
        .checked_sub(Duration::from_nanos(1))
        .expect("five-minute deadline supports subtracting one nanosecond");

    assert_eq!(watch_timeout_ms(None, started), 30_000);
    assert_eq!(watch_timeout_ms(Some(deadline), started), 30_000);
    assert_eq!(
        watch_timeout_ms(Some(deadline), started + Duration::from_secs(30)),
        30_000
    );
    assert_eq!(
        watch_timeout_ms(Some(deadline), one_second_before_deadline),
        1_000
    );
    assert_eq!(watch_timeout_ms(Some(deadline), deadline), 0);
    assert_eq!(
        watch_timeout_ms(Some(deadline), deadline + Duration::from_secs(1)),
        0
    );

    assert!(!watch_deadline_reached(
        Some(deadline),
        started + Duration::from_secs(30)
    ));
    assert!(!watch_deadline_reached(
        Some(deadline),
        one_nanosecond_before_deadline
    ));
    assert!(watch_deadline_reached(Some(deadline), deadline));
    assert!(watch_deadline_reached(
        Some(deadline),
        deadline + Duration::from_secs(1)
    ));
    assert!(!watch_deadline_reached(
        None,
        started + Duration::from_mins(5)
    ));
}

#[test]
fn wait_timeout_is_appended_as_a_typed_transient_status_issue() {
    let mut issues = vec![MonitorIssue {
        code: MonitorIssueCode::ResetDueUnverified,
        message: "reset needs a fresh observation".to_owned(),
        retry_at_epoch: None,
    }];

    append_wait_timeout_issue(&mut issues);

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].code, MonitorIssueCode::ResetDueUnverified);
    assert_eq!(issues[1].code, MonitorIssueCode::WaitTimeout);
    assert!(issues[1].message.contains("expired"));
    assert_eq!(issues[1].retry_at_epoch, None);
}

#[test]
fn auth_prepare_requires_all_standard_streams_to_be_terminal() {
    assert!(all_stdio_are_terminal(true, true, true));
    assert!(!all_stdio_are_terminal(false, true, true));
    assert!(!all_stdio_are_terminal(true, false, true));
    assert!(!all_stdio_are_terminal(true, true, false));
}

#[test]
fn auth_prepare_rejects_invalid_keychain_service_names() {
    let too_long = "x".repeat(513);
    for service in ["", "   ", "a\0b", too_long.as_str()] {
        let error = validate_keychain_service(service).unwrap_err();
        let exit = error.downcast_ref::<UsageCommandExit>().unwrap();
        assert_eq!(exit.exit_code(), 3);
    }
    validate_keychain_service("Claude Code-credentials").unwrap();
}

#[test]
fn experimental_collector_is_bound_to_an_opaque_foreground_source() {
    validate_experimental_collector_scope(false, None).unwrap();
    validate_experimental_collector_scope(true, Some("binding-1")).unwrap();
    assert!(validate_experimental_collector_scope(true, None).is_err());

    let source_id = "a".repeat(64);
    let active = jackin_protocol::usage_monitor::MonitorServiceStatus {
        running: true,
        experimental_collector_source: Some(source_id.clone()),
        active_monitors: 0,
        next_wake_epoch: None,
    };
    assert_eq!(
        foreground_experimental_collector_source(&active),
        Some(source_id.as_str())
    );
    let passive = jackin_protocol::usage_monitor::MonitorServiceStatus {
        running: false,
        experimental_collector_source: Some(source_id),
        active_monitors: 0,
        next_wake_epoch: None,
    };
    assert_eq!(foreground_experimental_collector_source(&passive), None);
    let malformed = jackin_protocol::usage_monitor::MonitorServiceStatus {
        running: true,
        experimental_collector_source: Some("g".repeat(64)),
        active_monitors: 0,
        next_wake_epoch: None,
    };
    assert_eq!(foreground_experimental_collector_source(&malformed), None);
}

#[test]
fn binding_maps_local_account_to_source_and_keeps_approval_explicit() {
    let source_id = "b".repeat(64);
    let input = binding_confirmation_input(&UsageBindingConfirmArgs {
        provider: UsageProviderArg::Claude,
        account: "work-account".to_owned(),
        source_capability_id: Some(source_id.clone()),
        approve_experimental_collector: true,
        operator_label: "work account".to_owned(),
        confirm: true,
    })
    .unwrap();
    assert_eq!(input.account_id, "work-account");
    assert_eq!(
        input.provider_account_id.as_deref(),
        Some(source_id.as_str())
    );
    assert!(input.experimental_collector_approved);

    let missing_source = binding_confirmation_input(&UsageBindingConfirmArgs {
        provider: UsageProviderArg::Claude,
        account: "work-account".to_owned(),
        source_capability_id: None,
        approve_experimental_collector: true,
        operator_label: "work account".to_owned(),
        confirm: true,
    })
    .expect_err("approval cannot exist without an explicit source mapping");
    assert_eq!(
        missing_source
            .downcast_ref::<UsageCommandExit>()
            .expect("CLI validation error")
            .exit_code(),
        3
    );
}

#[test]
fn auth_prepare_passes_exact_service_and_foreground_scope() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let config = jackin_usage::host::UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    let args = foreground_auth_bootstrap_args(&config, &paths, "Claude Code-credentials")
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    let expected = vec![
        "--prepare-auth".to_owned(),
        "--provider".to_owned(),
        "claude".to_owned(),
        "--keychain-service".to_owned(),
        "Claude Code-credentials".to_owned(),
        "--data-dir".to_owned(),
        paths.data_dir.to_string_lossy().into_owned(),
        "--config-root".to_owned(),
        paths.config_dir.to_string_lossy().into_owned(),
        "--operator-home".to_owned(),
        paths.home_dir.to_string_lossy().into_owned(),
        "--build-id".to_owned(),
        config.build_id,
    ];
    assert_eq!(args, expected);
}

mod v2_cli_contract_tests;
