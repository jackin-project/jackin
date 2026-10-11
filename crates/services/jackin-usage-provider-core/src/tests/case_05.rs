// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn provider_outcome_maps_presence_states() {
    use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: true,
            has_secret: true
        }),
        (
            UsageSnapshotStatus::Fresh,
            UsageSource::ProviderApi,
            UsageConfidence::Authoritative
        )
    );
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: false,
            has_secret: true
        }),
        (
            UsageSnapshotStatus::Unsupported,
            UsageSource::None,
            UsageConfidence::PresenceOnly
        )
    );
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: false,
            has_secret: false
        }),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageSource::None,
            UsageConfidence::None
        )
    );
}

#[test]
fn split_fetch_partitions_ok_err_and_absent() {
    assert_eq!(split_fetch(Some(Ok::<_, String>(7u64))), (Some(7), None));
    assert_eq!(
        split_fetch(Some(Err::<u64, _>("boom".to_owned()))),
        (None, Some("boom".to_owned()))
    );
    assert_eq!(split_fetch(None::<Result<u64, String>>), (None, None));
}

#[test]
fn provider_boundary_exports_only_bounded_request_fields() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let result = provider_request(
            jackin_telemetry::schema::enums::ProviderName::Openai,
            "GET",
            "/backend-api/wham/usage",
            || Ok::<_, String>("telemetry-private-response"),
        );
        assert_eq!(result.unwrap(), "telemetry-private-response");
    });
    export.force_flush();

    let spans = export
        .finished_spans()
        .into_iter()
        .filter(|span| span.name == jackin_telemetry::schema::spans::HTTP_CLIENT)
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    for prohibited in [
        "authorization",
        "account_id",
        "telemetry-private-response",
        "?private=query",
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_provider_boundary_exports_bounded_private_shapes() {
    if std::env::var_os("JACKIN_USAGE_WIRE_USAGE_CHILD").is_none() {
        let status = Command::new(
            std::env::current_exe().expect("usage test executable must resolve"),
        )
        .arg("--exact")
        .arg("tests::case_05::conformance_wire_provider_boundary_exports_bounded_private_shapes")
        .arg("--nocapture")
        .env("JACKIN_USAGE_WIRE_USAGE_CHILD", "1")
        .status()
        .expect("isolated wire usage test must start");
        assert!(status.success(), "isolated wire usage test failed");
        return;
    }
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");

    let success = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "GET",
        "/backend-api/wham/usage",
        || Ok::<_, String>("private-provider-response"),
    );
    assert_eq!(
        success.expect("provider request succeeds"),
        "private-provider-response"
    );
    let failure = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Anthropic,
        "POST",
        "/api/oauth/usage",
        || Err::<(), _>("private-token private-account ?private=query".to_owned()),
    );
    assert!(failure.is_err());
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = Instant::now() + Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "http.client")
            .collect::<Vec<_>>();
        if spans.len() == 2 {
            break spans;
        }
        assert!(
            Instant::now() < deadline,
            "provider HTTP wire spans did not arrive"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let wire_text = format!("{spans:?}");
    for expected in [
        "openai",
        "anthropic",
        "GET",
        "POST",
        "/backend-api/wham/usage",
        "/api/oauth/usage",
        "success",
        "failure",
        "http_error",
    ] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "private-provider-response",
        "private-token",
        "private-account",
        "?private=query",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn usage_bucket_presentation_orders_normal_segments() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("13% in deficit · Runs out in 2d".to_owned());
    bucket.reset_label = Some("Resets in 4d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec![
            "57% left",
            "13% in deficit",
            "Runs out in 2d",
            "Resets in 4d"
        ]
    );
    assert_eq!(presentation.remaining_label.as_deref(), Some("57% left"));
    assert_eq!(presentation.meter_percent, Some(57));
    assert_eq!(
        presentation.display_label,
        "57% left · 13% in deficit · Runs out in 2d · Resets in 4d"
    );
}

#[test]
fn usage_bucket_presentation_flattens_runout_composite() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(40),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("On pace · Runs out in 5d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["40% left", "On pace", "Runs out in 5d"]
    );
}

#[test]
fn usage_bucket_presentation_orders_spend_cap() {
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(70),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("SGD 78.49".to_owned());
    bucket.limit_label = Some("SGD 260.00".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["30% used", "Monthly cap: SGD 78.49 / SGD 260.00"]
    );
    // Spend text reads used, but meter geometry fills by remaining — the same
    // rule as every other slot and the console windows.
    assert_eq!(presentation.meter_percent, Some(70));
}

#[test]
fn usage_bucket_presentation_recovers_spend_overage_from_money() {
    // $150 against a $100 cap with a saturated 0% remaining: the money ratio
    // recovers the raw "150% used" text (matching the console window value)
    // and the meter reads empty (nothing left), matching the console meter.
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(0),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$150.00".to_owned());
    bucket.limit_label = Some("$100.00".to_owned());
    bucket.used_money = Some(Money::new(15_000, "USD", 2));
    bucket.limit_money = Some(Money::new(10_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.remaining_label.as_deref(), Some("150% used"));
    assert_eq!(presentation.meter_percent, Some(0));

    // A non-overage money ratio agrees with the remaining percent; the text
    // still reads used and the meter still fills by remaining.
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(55),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$45.20".to_owned());
    bucket.limit_label = Some("$100.00".to_owned());
    bucket.used_money = Some(Money::new(4_520, "USD", 2));
    bucket.limit_money = Some(Money::new(10_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.remaining_label.as_deref(), Some("45% used"));
    assert_eq!(presentation.meter_percent, Some(55));
}

#[test]
fn usage_bucket_presentation_orders_non_spend_budget() {
    let mut bucket = presentation_bucket(
        "Global budget",
        None,
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$0.00 spent".to_owned());
    bucket.limit_label = Some("$25,000.00".to_owned());
    bucket.used_money = Some(Money::new(0, "USD", 2));
    bucket.limit_money = Some(Money::new(2_500_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert!(
        presentation
            .display_segments
            .contains(&"Budget: $0.00 spent / $25,000.00".to_owned())
    );
}

#[test]
fn usage_bucket_presentation_appends_degraded_status() {
    let bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Stale,
    );
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["57% left", "stale"]);
}

#[test]
fn usage_bucket_presentation_credits_zero_left() {
    let mut bucket = presentation_bucket("Credits", Some(0), None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$4.76".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments.first().map(String::as_str),
        Some("0 left")
    );
    assert!(presentation.display_segments.contains(&"$4.76".to_owned()));
    assert_eq!(presentation.meter_percent, Some(0));
}

#[test]
fn usage_bucket_presentation_limit_only_balance() {
    let mut bucket = presentation_bucket("Prepaid", None, None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$25".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["$25"]);
    assert_eq!(presentation.meter_percent, None);
    assert_eq!(presentation.remaining_label, None);
}
