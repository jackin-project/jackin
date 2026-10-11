// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use clap::Parser as _;
use jackin_core::JackinPaths;
use std::time::{Duration, Instant};

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
    };

    assert_eq!(usage_amount_label(&account), "37/100 percent");
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
    }
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
fn cli_monitor_validation_matches_broker_identifier_and_text_bounds() {
    let max_id = "x".repeat(128);
    let valid_ids = ["a", "a-b_9", max_id.as_str()];
    for value in valid_ids {
        validate_monitor_identifier("monitor ID", value)
            .unwrap_or_else(|error| panic!("broker-valid identifier {value:?} rejected: {error}"));
    }
    let too_long_id = "x".repeat(129);
    for value in [
        "",
        "bad id",
        "non-ascii-ø",
        "line\nbreak",
        too_long_id.as_str(),
    ] {
        assert_cli_invalid_argument(validate_monitor_identifier("monitor ID", value).unwrap_err());
    }

    let max_model = "x".repeat(128);
    let valid_models = ["claude-opus", max_model.as_str()];
    for value in valid_models {
        validate_monitor_start_fields("valid-key", Some(value))
            .unwrap_or_else(|error| panic!("broker-valid model {value:?} rejected: {error}"));
    }
    let too_long_model = "x".repeat(129);
    for value in ["", "  ", "model\nname", "mødel", too_long_model.as_str()] {
        assert_cli_invalid_argument(
            validate_monitor_start_fields("valid-key", Some(value)).unwrap_err(),
        );
    }

    let max_goal = "x".repeat(256);
    for value in ["goal-1", "目标", max_goal.as_str()] {
        validate_monitor_goal_id("goal ID", value)
            .unwrap_or_else(|error| panic!("broker-valid goal {value:?} rejected: {error}"));
    }
    let too_long_goal = "x".repeat(257);
    for value in ["", "  ", "goal\nname", too_long_goal.as_str()] {
        assert_cli_invalid_argument(validate_monitor_goal_id("goal ID", value).unwrap_err());
    }

    let max_operator_label = "x".repeat(128);
    for value in ["operator", max_operator_label.as_str()] {
        validate_monitor_operator_label(value)
            .unwrap_or_else(|error| panic!("broker-valid operator label rejected: {error}"));
    }
    let too_long_operator_label = "x".repeat(129);
    for value in [
        "",
        "  ",
        "operator\nlabel",
        "opérator",
        too_long_operator_label.as_str(),
    ] {
        assert_cli_invalid_argument(validate_monitor_operator_label(value).unwrap_err());
    }

    let valid_idempotency = "x".repeat(512);
    validate_monitor_start_fields(&valid_idempotency, None).unwrap();
    let too_long_idempotency = "x".repeat(513);
    for value in [
        "",
        "  ",
        "idempotency\nkey",
        "non-ascii-ø",
        "v1-migrated-cli-key",
        too_long_idempotency.as_str(),
    ] {
        assert_cli_invalid_argument(validate_monitor_start_fields(value, None).unwrap_err());
    }
}

#[test]
fn cli_monitor_revision_preflight_rejects_zero_and_accepts_positive_values() {
    validate_monitor_revision("binding revision", 1).unwrap();
    validate_monitor_revision("policy revision", u64::MAX).unwrap();
    assert_cli_invalid_argument(validate_monitor_revision("binding revision", 0).unwrap_err());
}

#[test]
fn malformed_monitor_ids_fail_before_broker_attach_or_start() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let observe = UsageMonitorArgs {
        command: UsageMonitorCommand::Observe(UsageMonitorObserveArgs {
            provider: UsageProviderArg::Claude,
            session: Some("bad session".to_owned()),
            binding: None,
            binding_revision: None,
            idempotency_key: "valid-key".to_owned(),
            expected_model: None,
            experimental_collector: false,
        }),
    };

    assert_cli_invalid_argument(run_monitor(&paths, &observe).unwrap_err());
    assert_cli_invalid_argument(
        run_monitor_read(
            &paths,
            MonitorOperation::Status {
                monitor_id: String::new(),
            },
        )
        .unwrap_err(),
    );
    assert_cli_invalid_argument(
        run_monitor_read(
            &paths,
            MonitorOperation::Refresh {
                monitor_id: "bad id".to_owned(),
            },
        )
        .unwrap_err(),
    );
    assert_cli_invalid_argument(
        run_watch(
            &paths,
            &UsageWatchArgs {
                monitor: String::new(),
                timeout_secs: Some(1),
            },
        )
        .unwrap_err(),
    );
    assert_cli_invalid_argument(run_wait_until_runnable(&paths, "bad id", 1).unwrap_err());
    let stop = UsageMonitorArgs {
        command: UsageMonitorCommand::Stop(UsageMonitorIdArgs {
            monitor: String::new(),
        }),
    };
    assert_cli_invalid_argument(run_monitor(&paths, &stop).unwrap_err());
    assert!(
        !paths.data_dir.exists(),
        "invalid commands must not start a broker"
    );
}

#[test]
fn experimental_collector_requires_a_valid_foreground_service_source() {
    let active = MonitorServiceStatus {
        running: true,
        experimental_collector_source: Some("claude-source-account".to_owned()),
        active_monitors: 0,
        next_wake_epoch: None,
    };
    assert_eq!(
        foreground_experimental_collector_source(&active),
        Some("claude-source-account")
    );

    for status in [
        MonitorServiceStatus {
            running: true,
            experimental_collector_source: None,
            active_monitors: 0,
            next_wake_epoch: None,
        },
        MonitorServiceStatus {
            running: true,
            experimental_collector_source: Some("bad source id".to_owned()),
            active_monitors: 0,
            next_wake_epoch: None,
        },
        MonitorServiceStatus {
            running: false,
            experimental_collector_source: Some("claude-source-account".to_owned()),
            active_monitors: 0,
            next_wake_epoch: None,
        },
    ] {
        assert_eq!(foreground_experimental_collector_source(&status), None);
    }
}

#[test]
fn experimental_observer_does_not_autostart_passive_broker() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let args = UsageMonitorArgs {
        command: UsageMonitorCommand::Observe(UsageMonitorObserveArgs {
            provider: UsageProviderArg::Claude,
            session: None,
            binding: Some("binding-1".to_owned()),
            binding_revision: Some(1),
            idempotency_key: "collector-start-1".to_owned(),
            expected_model: None,
            experimental_collector: true,
        }),
    };

    let error = run_monitor(&paths, &args).unwrap_err();
    let exit = error
        .downcast_ref::<UsageCommandExit>()
        .expect("missing foreground service should return a stable CLI issue");
    assert_eq!(exit.exit_code(), 3);
    let value: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
    assert_eq!(value["error"]["code"], "collector_auth_required");
    assert!(
        !paths.data_dir.exists(),
        "experimental observation must not create a passive broker"
    );
}

fn assert_cli_invalid_argument(error: anyhow::Error) {
    let exit = error
        .downcast_ref::<UsageCommandExit>()
        .expect("preflight should return the stable CLI error");
    assert_eq!(exit.exit_code(), 3);
    let value: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_argument");
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
    // The broker's cursor-zero attach returns only the newest reconciled
    // current event, not retained history. Its cursor seeds subsequent reads.
    let fresh_attach_cursor = advance_watch_cursor(0, 17, [17]);
    assert_eq!(fresh_attach_cursor, 17);

    let live_cursor = advance_watch_cursor(fresh_attach_cursor, 19, [18, 19]);
    assert_eq!(live_cursor, 19);
    assert_eq!(advance_watch_cursor(live_cursor, 18, [16, 19]), 19);
}

#[test]
fn watch_batch_timeout_does_not_end_the_absolute_cli_deadline() {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(300);

    assert_eq!(watch_timeout_ms(None, started), 30_000);
    assert_eq!(watch_timeout_ms(Some(deadline), started), 30_000);
    assert_eq!(
        watch_timeout_ms(Some(deadline), started + Duration::from_secs(30)),
        30_000
    );
    assert_eq!(
        watch_timeout_ms(Some(deadline), deadline - Duration::from_secs(1)),
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
        deadline - Duration::from_nanos(1)
    ));
    assert!(watch_deadline_reached(Some(deadline), deadline));
    assert!(watch_deadline_reached(
        Some(deadline),
        deadline + Duration::from_secs(1)
    ));
    assert!(!watch_deadline_reached(
        None,
        started + Duration::from_secs(300)
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
fn usage_auth_prepare_keeps_exact_service_and_global_data_dir() {
    let cli = crate::cli::Cli::try_parse_from([
        "jackin",
        "usage",
        "auth",
        "prepare",
        "--provider",
        "claude",
        "--keychain-service",
        "Claude custom service",
        "--data-dir",
        "/tmp/usage-data",
    ])
    .unwrap();
    let Some(crate::cli::Command::Usage(args)) = cli.command else {
        panic!("usage command should parse");
    };
    assert_eq!(
        args.data_dir.as_deref(),
        Some(std::path::Path::new("/tmp/usage-data"))
    );
    let Some(UsageScope::Auth(auth)) = args.scope else {
        panic!("usage auth should parse");
    };
    let UsageAuthCommand::Prepare(auth) = auth.command;
    assert_eq!(auth.provider, UsageProviderArg::Claude);
    assert_eq!(
        auth.keychain_service.as_deref(),
        Some("Claude custom service")
    );
}

#[test]
fn monitor_observe_experimental_collector_is_explicit_and_defaults_off() {
    let parsed = |extra: &[&str]| {
        let mut args = vec![
            "jackin",
            "usage",
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--session",
            "session-1",
            "--idempotency-key",
            "start-1",
        ];
        args.extend_from_slice(extra);
        crate::cli::Cli::try_parse_from(args).unwrap()
    };
    let Some(crate::cli::Command::Usage(args)) = parsed(&[]).command else {
        panic!("usage command should parse");
    };
    let Some(UsageScope::Monitor(command)) = args.scope else {
        panic!("monitor command should parse");
    };
    let UsageMonitorCommand::Observe(observe) = command.command else {
        panic!("observe command should parse");
    };
    assert!(!observe.experimental_collector);

    let Some(crate::cli::Command::Usage(args)) = parsed(&["--experimental-collector"]).command
    else {
        panic!("usage command should parse");
    };
    let Some(UsageScope::Monitor(command)) = args.scope else {
        panic!("monitor command should parse");
    };
    let UsageMonitorCommand::Observe(observe) = command.command else {
        panic!("observe command should parse");
    };
    assert!(observe.experimental_collector);
}

#[test]
fn experimental_collector_requires_a_bound_observer_scope() {
    let result = validate_experimental_collector_scope(true, None);
    assert!(result.is_err());
    validate_experimental_collector_scope(false, None).unwrap();
    validate_experimental_collector_scope(true, Some("binding-1")).unwrap();
}

#[test]
fn binding_confirmation_can_select_a_canonical_local_source_account() {
    let cli = crate::cli::Cli::try_parse_from([
        "jackin",
        "usage",
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--account",
        "monitor-account",
        "--provider-account",
        "source-account",
        "--approve-experimental-collector",
        "--operator-label",
        "work",
        "--confirm",
    ])
    .unwrap();
    let Some(crate::cli::Command::Usage(args)) = cli.command else {
        panic!("usage command should parse");
    };
    let Some(UsageScope::Binding(binding)) = args.scope else {
        panic!("binding command should parse");
    };
    let UsageBindingCommand::Confirm(confirm) = binding.command;
    assert_eq!(confirm.provider_account.as_deref(), Some("source-account"));
    assert!(confirm.approve_experimental_collector);
    let binding = binding_confirmation_input(&confirm).unwrap();
    assert_eq!(
        binding.provider_account_id.as_deref(),
        Some("source-account")
    );
    assert!(binding.experimental_collector_approved);

    let cli = crate::cli::Cli::try_parse_from([
        "jackin",
        "usage",
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--account",
        "monitor-account",
        "--operator-label",
        "work",
        "--confirm",
    ])
    .unwrap();
    let Some(crate::cli::Command::Usage(args)) = cli.command else {
        panic!("usage command should parse");
    };
    let Some(UsageScope::Binding(binding)) = args.scope else {
        panic!("binding command should parse");
    };
    let UsageBindingCommand::Confirm(confirm) = binding.command;
    assert_eq!(confirm.provider_account, None);
    assert!(!confirm.approve_experimental_collector);
    let binding = binding_confirmation_input(&confirm).unwrap();
    assert!(!binding.experimental_collector_approved);

    let mapped_without_approval = UsageBindingConfirmArgs {
        provider: UsageProviderArg::Claude,
        account: "monitor-account".to_owned(),
        provider_account: Some("source-account".to_owned()),
        approve_experimental_collector: false,
        operator_label: "work".to_owned(),
        confirm: true,
    };
    let binding = binding_confirmation_input(&mapped_without_approval).unwrap();
    assert_eq!(
        binding.provider_account_id.as_deref(),
        Some("source-account")
    );
    assert!(!binding.experimental_collector_approved);
}

#[test]
fn collector_approval_requires_a_mapped_provider_account() {
    let cli = crate::cli::Cli::try_parse_from([
        "jackin",
        "usage",
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--account",
        "monitor-account",
        "--approve-experimental-collector",
        "--operator-label",
        "work",
        "--confirm",
    ])
    .unwrap();
    let Some(crate::cli::Command::Usage(args)) = cli.command else {
        panic!("usage command should parse");
    };
    let Some(UsageScope::Binding(binding)) = args.scope else {
        panic!("binding command should parse");
    };
    let UsageBindingCommand::Confirm(confirm) = binding.command;
    let error = binding_confirmation_input(&confirm).unwrap_err();
    let exit = error.downcast_ref::<UsageCommandExit>().unwrap();
    assert_eq!(exit.exit_code(), 3);
    let value: serde_json::Value = serde_json::from_str(exit.json()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_argument");
}

#[test]
fn foreground_auth_arguments_preserve_all_paths_and_exact_service() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = jackin_usage::host::UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    config.build_id = "test-build".to_owned();
    let args = foreground_auth_bootstrap_args(&config, &paths, "Claude custom service")
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let option_value = |name: &str| {
        let index = args.iter().position(|arg| arg == name).unwrap();
        args[index + 1].as_str()
    };

    assert!(args.iter().any(|arg| arg == "--prepare-auth"));
    assert_eq!(option_value("--provider"), "claude");
    assert_eq!(option_value("--keychain-service"), "Claude custom service");
    assert_eq!(option_value("--data-dir"), paths.data_dir.to_string_lossy());
    assert_eq!(
        option_value("--config-root"),
        paths.config_dir.to_string_lossy()
    );
    assert_eq!(
        option_value("--operator-home"),
        paths.home_dir.to_string_lossy()
    );
    assert_eq!(option_value("--build-id"), "test-build");
}

#[cfg(unix)]
use super::statusline::{
    MAX_INPUT_BYTES as STATUSLINE_MAX_INPUT_BYTES, MAX_LEGACY_COMMAND_BYTES,
    MAX_SETTINGS_BYTES as STATUSLINE_MAX_SETTINGS_BYTES, compose as statusline_compose,
};
#[cfg(unix)]
use serde_json::{Value as StatuslineValue, json as statusline_json};
#[cfg(unix)]
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(unix)]
use std::path::Path as StatuslinePath;
#[cfg(unix)]
use std::process::{Command as StatuslineCommand, Stdio as StatuslineStdio};
#[cfg(unix)]
use tempfile::TempDir as StatuslineTempDir;

fn usage_contract_policy_args(
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
fn cli_contract_operator_terminal_gate_fails_closed_when_any_stream_is_headless() {
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
fn cli_contract_monitor_issues_have_stable_blocked_and_invalid_exit_classes() {
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
fn cli_contract_observer_start_reports_tracking_success_without_authorizing_dispatch() {
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
fn cli_contract_policy_approval_defaults_strict_sgd_and_never_infers_quota_only() {
    let strict = policy_approval_input(&usage_contract_policy_args(
        UsageMonitorPolicyArg::StrictSgd,
        None,
        false,
    ))
    .unwrap();
    assert_eq!(strict.new_policy, MonitorPolicy::StrictSgd);
    assert_eq!(strict.budget, Some(Money::new(5_000, "SGD", 2)));
    assert!(!strict.acknowledge_no_sgd_cap);

    let quota_only = policy_approval_input(&usage_contract_policy_args(
        UsageMonitorPolicyArg::QuotaOnly,
        None,
        true,
    ))
    .unwrap();
    assert_eq!(quota_only.new_policy, MonitorPolicy::QuotaOnly);
    assert_eq!(quota_only.budget, None);
    assert!(quota_only.acknowledge_no_sgd_cap);

    let missing_ack = policy_approval_input(&usage_contract_policy_args(
        UsageMonitorPolicyArg::QuotaOnly,
        None,
        false,
    ))
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
fn cli_contract_strict_sgd_budget_must_be_positive() {
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
fn cli_contract_statusline_session_only_scope_comes_from_the_payload() {
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

#[cfg(unix)]
fn statusline_fixture(
    temp: &StatuslineTempDir,
    settings: StatuslineValue,
    binary_body: &str,
) -> (PathBuf, PathBuf) {
    let settings_path = temp.path().join("settings.json");
    std::fs::write(&settings_path, serde_json::to_vec(&settings).unwrap()).unwrap();

    let binary_path = temp.path().join("fake jackin");
    std::fs::write(&binary_path, binary_body).unwrap();
    let mut permissions = std::fs::metadata(&binary_path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary_path, permissions).unwrap();
    (settings_path, binary_path)
}

#[cfg(unix)]
fn statusline_session_scope() -> UsageStatuslineScopeArgs {
    UsageStatuslineScopeArgs {
        session_only: true,
        binding: None,
        binding_revision: None,
    }
}

#[cfg(unix)]
fn statusline_binding_scope(binding: &str) -> UsageStatuslineScopeArgs {
    UsageStatuslineScopeArgs {
        session_only: false,
        binding: Some(binding.to_owned()),
        binding_revision: Some(7),
    }
}

#[cfg(unix)]
fn statusline_run_composed(
    shell_command: &str,
    input: &[u8],
    env: &[(&str, &StatuslinePath)],
) -> std::process::Output {
    let mut launcher = StatuslineCommand::new("/bin/sh");
    launcher
        .arg("-c")
        .arg(shell_command)
        .env("SHELL", "/bin/sh")
        .stdout(StatuslineStdio::piped())
        .stderr(StatuslineStdio::piped())
        .stdin(StatuslineStdio::piped());
    for (key, path) in env {
        launcher.env(key, path);
    }
    let mut child = launcher.spawn().unwrap();
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

#[cfg(unix)]
#[test]
fn statusline_preserves_settings_and_statusline_options() {
    let temp = StatuslineTempDir::new().unwrap();
    let original = statusline_json!({
        "theme": "dark",
        "env": {"API_SECRET": "test-only-api-secret-sentinel"},
        "statusLine": {
            "type": "command",
            "command": "printf 'old output'",
            "padding": 3,
            "refreshInterval": 10,
            "futureOption": {"kept": true}
        }
    });
    let (settings, binary) = statusline_fixture(&temp, original.clone(), "#!/bin/sh\nexit 0\n");
    let result = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();

    assert_eq!(
        result
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["statusLine"]
    );
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("test-only-api-secret-sentinel")
    );
    assert_eq!(result["statusLine"]["type"], original["statusLine"]["type"]);
    assert_eq!(
        result["statusLine"]["padding"],
        original["statusLine"]["padding"]
    );
    assert_eq!(
        result["statusLine"]["refreshInterval"],
        original["statusLine"]["refreshInterval"]
    );
    assert_eq!(
        result["statusLine"]["futureOption"],
        original["statusLine"]["futureOption"]
    );
    assert_ne!(
        result["statusLine"]["command"],
        original["statusLine"]["command"]
    );
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        serde_json::to_vec(&original).unwrap(),
        "compose must not mutate the settings file"
    );
}

#[cfg(unix)]
#[test]
fn statusline_adds_a_statusline_for_initial_setup_without_rendering_output() {
    let temp = StatuslineTempDir::new().unwrap();
    let original = statusline_json!({"theme": "dark"});
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = statusline_fixture(
        &temp,
        original.clone(),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\n",
    );
    let data_dir = temp.path().join("data");
    let proposed =
        statusline_compose(&settings, &binary, &statusline_session_scope(), &data_dir).unwrap();
    let input = br#"{"session_id":"session-1"}"#;
    let input_path = temp.path().join("ingress.json");
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(input_path).unwrap(), input);
    let args = std::fs::read_to_string(args_path).unwrap();
    assert!(args.contains("--session-only\n"));
    assert!(!args.contains("--binding\n"));
    assert!(!args.contains("--account\n"));
    assert_eq!(
        proposed
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["statusLine"]
    );
    assert_eq!(proposed["statusLine"]["type"], "command");
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        serde_json::to_vec(&original).unwrap(),
        "compose must not mutate the settings file"
    );
}

#[cfg(unix)]
#[test]
fn statusline_proposes_initial_settings_when_settings_file_is_absent() {
    let temp = StatuslineTempDir::new().unwrap();
    let settings = temp.path().join("claude/settings.json");
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    let proposed = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();

    assert_eq!(proposed["statusLine"]["type"], "command");
    assert!(
        !settings.exists(),
        "compose must not create the settings file"
    );
    assert!(!settings.parent().unwrap().exists());
}

#[cfg(unix)]
#[test]
fn statusline_forwards_original_payload_and_output_once_and_quotes_arguments() {
    let temp = StatuslineTempDir::new().unwrap();
    let input_path = temp.path().join("ingress input.json");
    let args_path = temp.path().join("ingress args.txt");
    let legacy = "cat; printf '\\nlegacy-tail\\n'";
    let original = statusline_json!({"statusLine": {"type": "command", "command": legacy}});
    let (settings, binary) = statusline_fixture(
        &temp,
        original,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\nprintf 'ingress-output-must-not-leak\\n'\nexit 7\n",
    );
    let injection_path = temp.path().join("statusline-injection");
    let binding = format!("binding ' ; touch {}", injection_path.display());
    let data_dir = temp.path().join("data directory");
    let proposed = statusline_compose(
        &settings,
        &binary,
        &statusline_binding_scope(&binding),
        &data_dir,
    )
    .unwrap();
    let input = br#"{"session_id":"session-1","rate_limits":{"five_hour":{"used_percentage":18,"resets_at":2000000000}}}"#;
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = input.to_vec();
    expected.extend_from_slice(b"\nlegacy-tail\n");
    assert_eq!(output.stdout, expected);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ingress-output-must-not-leak"));
    assert_eq!(std::fs::read(&input_path).unwrap(), input);
    let args = std::fs::read_to_string(args_path).unwrap();
    assert!(args.contains("--binding\n"));
    assert!(args.contains(&binding));
    assert!(args.contains("--binding-revision\n7\n"));
    assert!(args.contains("--data-dir\n"));
    assert!(args.contains(data_dir.to_str().unwrap()));
    assert!(args.contains("--format\njson\n"));
    assert!(!injection_path.exists());
}

#[cfg(unix)]
#[test]
fn statusline_oversized_payload_reaches_legacy_without_ingress() {
    let temp = StatuslineTempDir::new().unwrap();
    let input_path = temp.path().join("ingress.json");
    let args_path = temp.path().join("ingress.args");
    let original = statusline_json!({"statusLine": {"type": "command", "command": "cat"}});
    let (settings, binary) = statusline_fixture(
        &temp,
        original,
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\n",
    );
    let data_dir = temp.path().join("data");
    let proposed =
        statusline_compose(&settings, &binary, &statusline_session_scope(), &data_dir).unwrap();
    let input = vec![b'x'; STATUSLINE_MAX_INPUT_BYTES + 1];
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        &input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, input);
    assert!(!args_path.exists(), "oversized input must skip ingress");
    assert!(!input_path.exists());
}

#[cfg(unix)]
#[test]
fn statusline_malformed_small_payload_does_not_replace_legacy_output() {
    let temp = StatuslineTempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let original =
        statusline_json!({"statusLine": {"type": "command", "command": "printf 'legacy\\n'"}});
    let (settings, binary) = statusline_fixture(
        &temp,
        original,
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\nprintf 'broken-json'\nexit 2\n",
    );
    let data_dir = temp.path().join("data");
    let proposed =
        statusline_compose(&settings, &binary, &statusline_session_scope(), &data_dir).unwrap();
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"{broken",
        &[("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, b"legacy\n");
    assert!(
        args_path.exists(),
        "small malformed input reaches the parser"
    );
}

#[cfg(unix)]
#[test]
fn statusline_rejects_settings_that_cannot_be_safely_composed() {
    let temp = StatuslineTempDir::new().unwrap();
    for (settings_value, expected_error) in [
        (
            statusline_json!({"statusLine": {"type": "prompt"}}),
            "statusLine.type must be `command`",
        ),
        (
            statusline_json!({"statusLine": {"type": "command"}}),
            "statusLine.command must be a string",
        ),
    ] {
        let (settings, binary) = statusline_fixture(&temp, settings_value, "#!/bin/sh\nexit 0\n");
        let error = statusline_compose(
            &settings,
            &binary,
            &statusline_session_scope(),
            &temp.path().join("data"),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_error));
    }

    let nul_command =
        statusline_json!({"statusLine": {"type": "command", "command": "printf\0bad"}});
    let (settings, binary) = statusline_fixture(&temp, nul_command, "#!/bin/sh\nexit 0\n");
    let error = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot contain a NUL byte"));
}

#[cfg(unix)]
#[test]
fn statusline_rejects_statusline_scope_without_exactly_one_selection() {
    let temp = StatuslineTempDir::new().unwrap();
    for (scope, expected) in [
        (
            UsageStatuslineScopeArgs {
                session_only: false,
                binding: None,
                binding_revision: None,
            },
            "--session-only",
        ),
        (
            UsageStatuslineScopeArgs {
                session_only: false,
                binding: Some("binding".to_owned()),
                binding_revision: None,
            },
            "--binding-revision",
        ),
        (
            UsageStatuslineScopeArgs {
                session_only: true,
                binding: Some("binding".to_owned()),
                binding_revision: Some(1),
            },
            "--session-only",
        ),
    ] {
        let settings = temp.path().join("settings.json");
        std::fs::write(&settings, b"{}").unwrap();
        let binary = temp.path().join("jackin");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
        let error =
            statusline_compose(&settings, &binary, &scope, &temp.path().join("data")).unwrap_err();
        assert!(error.to_string().contains(expected));
    }
}

#[cfg(unix)]
#[test]
#[cfg(unix)]
fn rejects_long_commands_and_quote_expansion_without_changing_settings() {
    let temp = StatuslineTempDir::new().unwrap();
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    for (command, expected_error) in [
        ("'".repeat(MAX_LEGACY_COMMAND_BYTES), "64 KiB"),
        ("x".repeat(MAX_LEGACY_COMMAND_BYTES + 1), "16 KiB"),
    ] {
        let original = statusline_json!({"statusLine": {"type": "command", "command": command}});
        let settings = temp.path().join("settings.json");
        let original_bytes = serde_json::to_vec(&original).unwrap();
        std::fs::write(&settings, &original_bytes).unwrap();

        let error = statusline_compose(
            &settings,
            &binary,
            &statusline_session_scope(),
            &temp.path().join("data"),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_error));
        assert_eq!(std::fs::read(&settings).unwrap(), original_bytes);
    }
}

#[cfg(unix)]
#[test]
#[cfg(unix)]
fn preserves_legacy_failure_status_and_skips_ingestion() {
    let temp = StatuslineTempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = statusline_fixture(
        &temp,
        statusline_json!({"statusLine": {"type": "command", "command": "printf 'legacy\\n'; exit 7"}}),
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\n",
    );
    let proposed = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"payload",
        &[("ARGS_CAPTURE", &args_path)],
    );

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"legacy\n");
    assert!(
        !args_path.exists(),
        "failed legacy status must skip ingestion"
    );
}

#[cfg(unix)]
#[test]
#[cfg(unix)]
fn runs_legacy_command_without_ingestion_when_python_is_unavailable() {
    let temp = StatuslineTempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = statusline_fixture(
        &temp,
        statusline_json!({"statusLine": {"type": "command", "command": "printf legacy"}}),
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\n",
    );
    let path = temp.path().join("path-with-shell-only");
    std::fs::create_dir(&path).unwrap();
    std::os::unix::fs::symlink("/bin/sh", path.join("sh")).unwrap();
    let proposed = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();
    let output = statusline_run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"payload",
        &[("PATH", &path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, b"legacy");
    assert!(!args_path.exists(), "missing Python must skip ingestion");
}

#[cfg(unix)]
#[test]
fn statusline_rejects_malformed_and_oversized_settings_without_unbounded_reads() {
    let temp = StatuslineTempDir::new().unwrap();
    let settings = temp.path().join("settings.json");
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    std::fs::write(&settings, b"{").unwrap();
    let error = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("parse Claude Code settings"));

    std::fs::write(&settings, vec![b' '; STATUSLINE_MAX_SETTINGS_BYTES + 1]).unwrap();
    let error = statusline_compose(
        &settings,
        &binary,
        &statusline_session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("1 MiB composition limit"));
}
