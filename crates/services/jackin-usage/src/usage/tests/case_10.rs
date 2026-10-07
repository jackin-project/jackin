// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn amp_paid_only_balances_do_not_infer_daily_or_plan() {
    let usage = parse_amp_usage_output(
        "Signed in as user@example.com (example)\n\
         Individual credits: $9.86 remaining\n\
         Workspace example: $5.33 remaining",
    )
    .expect("paid-only usage");
    assert_eq!(usage.plan_label(), None);
    let buckets = usage.buckets(1_781_185_560);
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.status_slot != Some(StatusSlot::Daily))
    );
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Amp, &buckets),
        None
    );

    // The Fresh, Authoritative view preserves provenance and has no plan label.
    for source in [UsageSource::ProviderApi, UsageSource::Cli] {
        let view = amp_view_from_usage(
            AmpSuccessContext {
                agent: "amp",
                credential_origin: Some("API key · env AMP_API_KEY".to_owned()),
                source,
            },
            usage.clone(),
            1_781_185_560,
        );
        assert_eq!(view.status, UsageSnapshotStatus::Fresh);
        assert_eq!(view.confidence, UsageConfidence::Authoritative);
        assert_eq!(view.source, source);
        assert_eq!(
            view.account.credential_origin.as_deref(),
            Some("API key · env AMP_API_KEY")
        );
        assert_eq!(view.account.plan_label, None);
        assert!(
            view.buckets
                .iter()
                .any(|bucket| bucket.label == "Individual credits")
        );
    }

    // A Daily bucket beside credits yields the daily headline, never a credit amount.
    let mut with_daily = usage.clone();
    with_daily.daily_remaining_percent = Some(61);
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Amp, &with_daily.buckets(1_781_185_560))
            .as_deref(),
        Some("Free 61%")
    );
}

#[test]
fn amp_legacy_hourly_display_text_is_rejected() {
    // The retired hourly-dollar line alone parses to nothing.
    assert!(
        parse_amp_usage_output("Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour)").is_none()
    );
    // Paired with current credit rows it contributes no Amp Free bucket.
    let usage = parse_amp_usage_output(
        "Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour)\n\
         Individual credits: $0.33 remaining",
    )
    .expect("credit rows");
    assert_eq!(usage.daily_remaining_percent, None);
    assert!(
        usage
            .buckets(1_781_185_560)
            .iter()
            .all(|bucket| bucket.status_slot != Some(StatusSlot::Daily))
    );
}

#[test]
fn cli_output_collector_treats_reaped_child_as_success() {
    let output = collect_cli_output(
        "amp",
        None,
        thread::spawn(|| Ok("usage rows".to_owned())),
        thread::spawn(|| Ok(String::new())),
    )
    .expect("cli output");

    assert!(output.success);
    assert_eq!(output.exit_code, None);
    assert_eq!(output.stdout, "usage rows");
}

#[cfg(unix)]
#[test]
fn usage_cli_owner_exports_outcomes_without_process_material() {
    // A fresh executable in a temporary directory can be held by macOS
    // Gatekeeper longer than the process timeout under parallel test load.
    let command = "/bin/sh";

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    // Success/error paths must outlive heavy parallel nextest load; 1s races
    // under full `ci --fast` when the host is saturated (poll loop is 50ms).
    let settle = Duration::from_secs(10);
    run_cli_with_timeout_full(command, &["-c", "printf usage-secret-output"], settle).unwrap();
    run_cli_with_timeout_full(
        command,
        &["-c", "printf usage-secret-stderr >&2; exit 17"],
        settle,
    )
    .unwrap();
    let _timeout = run_cli_with_timeout_full(command, &["-c", "sleep 1"], Duration::from_millis(5))
        .unwrap_err();
    let _spawn = run_cli_with_timeout_full(
        "/usage-secret/missing/claude",
        &["usage-secret-argument"],
        settle,
    )
    .unwrap_err();

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 3);
    for expected in [
        "claude",
        "process_exit_nonzero",
        "process_spawn_error",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        command,
        "usage-secret-output",
        "usage-secret-stderr",
        "/usage-secret/missing/claude",
        "usage-secret-argument",
    ] {
        assert!(!export.contains_span_text(prohibited));
    }
}

#[test]
fn usage_cli_output_capture_is_bounded() {
    let oversized = vec![b'x'; PROCESS_OUTPUT_MAX + 1];
    assert_eq!(
        read_process_pipe(std::io::Cursor::new(oversized)).unwrap_err(),
        "process output exceeded limit"
    );
}

#[test]
fn amp_secrets_json_provides_api_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secrets.json");
    fs::write(
        &path,
        serde_json::json!({
            "other": "ignored",
            "apiKey@https://ampcode.com/": " amp-token "
        })
        .to_string(),
    )
    .expect("write Amp secrets");

    assert_eq!(load_amp_api_key(&path).as_deref(), Some("amp-token"));
}

#[test]
fn zai_quota_response_maps_token_session_and_time_limits() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "msg": "ok",
        "data": {
            "planName": "Coding Pro",
            "limits": [
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 5,
                    "number": 300,
                    "usage": 1000,
                    "currentValue": 250,
                    "remaining": 750,
                    "percentage": 25,
                    "nextResetTime": 1_781_189_520_000_i64
                },
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 6,
                    "number": 1,
                    "usage": 10000,
                    "currentValue": 9000,
                    "remaining": 1000,
                    "percentage": 90,
                    "nextResetTime": 1_781_798_400_000_i64
                },
                {
                    "type": "TIME_LIMIT",
                    "unit": 5,
                    "number": 1,
                    "usage": 120,
                    "currentValue": 30,
                    "remaining": 90,
                    "percentage": 25
                }
            ]
        }
    }))
    .expect("valid Z.AI quota");

    let buckets = quota.buckets(1_781_185_560);

    assert_eq!(quota.plan_name().as_deref(), Some("Coding Pro"));
    // Semantic identity comes from explicit duration, not array position.
    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, Some(75));
    assert_eq!(buckets[0].pace_label, None);
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].pace_label, None);
    assert_eq!(buckets[2].label, "MCP");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(buckets[2].remaining_percent, Some(75));
    assert_eq!(
        buckets[2].pace_label.as_deref(),
        Some("30 / 120 (90 remaining)")
    );
}

#[test]
fn zai_plan_label_falls_back_to_level() {
    let tokens_limit = serde_json::json!({
        "type": "TOKENS_LIMIT",
        "unit": 5,
        "number": 300,
        "usage": 1000,
        "currentValue": 250,
        "remaining": 750,
        "percentage": 25,
        "nextResetTime": 1_781_189_520_000_i64
    });
    // `level` present, no `planName`: the one plan field observed in the wild.
    let level_only: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": { "level": "pro", "limits": [tokens_limit.clone()] }
    }))
    .expect("level-only quota");
    assert_eq!(level_only.plan_name().as_deref(), Some("pro"));

    // Both present parses without a duplicate-field error; explicit name wins.
    let both: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": { "planName": "Coding Pro", "level": "pro", "limits": [tokens_limit] }
    }))
    .expect("planName + level quota");
    assert_eq!(both.plan_name().as_deref(), Some("Coding Pro"));
}

#[test]
fn zai_duration_classifier_handles_sole_and_reordered_limits() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": {
            "limits": [
                {"type": "CREDIT_LIMIT", "unit": 6, "number": 1, "percentage": 75},
                {"type": "TOKENS_LIMIT", "unit": 5, "number": 300, "percentage": 10},
                {"type": "TIME_LIMIT", "unit": 5, "number": 2, "percentage": 20}
            ]
        }
    }))
    .expect("duration fixture");
    let buckets = quota.buckets(1_781_185_560);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[1].label, "Session");
    assert_eq!(buckets[2].label, "MCP");

    let malformed: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "data": {"limits": [{"type": "TOKENS_LIMIT", "unit": 99, "number": 1, "percentage": 50}]}
    }))
    .expect("malformed duration fixture");
    assert!(malformed.buckets(1_781_185_560).is_empty());
}

#[test]
fn codex_duration_classifier_and_individual_limit_are_provider_evidenced() {
    let response: CodexUsageResponse = serde_json::from_value(serde_json::json!({
        "rate_limit": {
            "primary_window": {"used_percent": 10, "limit_window_seconds": 604800, "reset_at": 1782000000},
            "secondary_window": {"used_percent": 20, "limit_window_seconds": 300, "reset_at": 1781000000}
        },
        "spend_control": {"individual_limit": {
            "limit": 300,
            "used": 53.31,
            "remaining_percent": 82,
            "resets_at": 1783000000
        }}
    }))
    .expect("Codex duration fixture");
    let buckets = response.buckets(1_781_000_000);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[1].label, "Session");
    let cap = buckets
        .iter()
        .find(|bucket| bucket.label == "Individual limit")
        .expect("individual cap");
    assert_eq!(cap.remaining_percent, Some(82));
    assert_eq!(
        cap.limit_money.as_ref().map(|money| money.amount_minor),
        Some(30_000)
    );
    assert_eq!(
        cap.used_money.as_ref().map(|money| money.amount_minor),
        Some(5_331)
    );
}

#[test]
fn opencode_auth_and_usage_contract_is_typed_without_secret_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("auth.json");
    fs::write(
        &path,
        serde_json::json!({
            "opencode-go": {"type": "api", "key": "secret-not-output"}
        })
        .to_string(),
    )
    .expect("auth fixture");
    fs::write(dir.path().join("opencode.db"), "database fixture").expect("database fixture");
    assert_eq!(
        load_opencode_api_key(&path).as_deref(),
        Ok("secret-not-output")
    );
    let quota = parse_opencode_usage(
        serde_json::json!({
            "usage": {
                "rolling": {"status": "ok", "percent": 20, "resetsAt": "2026-08-21T12:00:00Z"},
                "weekly": {"status": "rate-limited", "percent": 80, "resetsAt": "2026-08-25T12:00:00Z"},
                "monthly": {"status": "ok", "percent": 5, "resetsAt": "2026-09-01T12:00:00Z"}
            }
        }),
        1_776_000_000,
    )
    .expect("OpenCode usage fixture");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(quota.buckets[0].label, "Rolling");
    assert_eq!(quota.buckets[1].status, UsageSnapshotStatus::Unavailable);
    assert!(quota.rate_limited);
    fs::write(
        &path,
        serde_json::json!({
            "anthropic": {"type": "api", "key": "unrelated-sentinel"},
            "opencode-go": {"type": "api", "key": "secret-not-output"}
        })
        .to_string(),
    )
    .expect("ambiguous auth fixture");
    let error = load_opencode_api_key(&path).unwrap_err();
    assert!(error.contains("multiple credentials"), "{error}");
    assert!(!error.contains("unrelated-sentinel"));
    fs::write(
        &path,
        serde_json::json!({"opencode-go": {"type": "oauth", "key": "secret-not-output"}})
            .to_string(),
    )
    .expect("malformed auth fixture");
    load_opencode_api_key(&path).unwrap_err();
    fs::write(
        &path,
        serde_json::json!({"anthropic": {"type": "api", "key": "unrelated-sentinel"}}).to_string(),
    )
    .expect("foreign-only auth fixture");
    let error = load_opencode_api_key(&path).unwrap_err();
    assert!(error.contains("opencode-go credential is missing"));
    assert!(!error.contains("unrelated-sentinel"));
}
