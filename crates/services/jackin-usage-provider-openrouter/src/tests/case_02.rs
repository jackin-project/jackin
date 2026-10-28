// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn openrouter_credits_separates_spend_from_remaining_balance() {
    let snapshot = |credits_body: &str| {
        let server =
            CannedSnapshotServer::start_with_credits(key_fixture(), credits_body.to_owned());
        openrouter_snapshot_with_base(
            "opencode",
            Some("fixture-key"),
            server.base_url(),
            1_780_000_000,
        )
    };

    let normal = snapshot(r#"{"data":{"total_credits":100.0,"total_usage":25.0}}"#);
    assert_eq!(
        normal.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(normal.account.provider_label, "OpenRouter");
    let view = account_credits_bucket(&normal);
    assert_eq!(view.used_label.as_deref(), Some("$25"));
    assert_eq!(view.limit_label.as_deref(), Some("$100"));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(2_500)
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(10_000)
    );
    assert_eq!(
        view.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(75.0)
    );
    assert_eq!(
        view.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let exhausted = snapshot(r#"{"data":{"total_credits":10.0,"total_usage":10.0}}"#);
    assert_eq!(
        exhausted.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(exhausted.account.provider_label, "OpenRouter");
    let exhausted = account_credits_bucket(&exhausted);
    assert_eq!(exhausted.used_label.as_deref(), Some("$10"));
    assert_eq!(
        exhausted
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(0.0)
    );
    assert_eq!(
        exhausted.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let overage = snapshot(r#"{"data":{"total_credits":100.0,"total_usage":120.0}}"#);
    assert_eq!(
        overage.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(overage.account.provider_label, "OpenRouter");
    let overage = account_credits_bucket(&overage);
    assert_eq!(overage.used_label.as_deref(), Some("$120"));
    assert_eq!(overage.remaining_percent, None);
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(overage)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(120.0)
    );
    assert_eq!(
        overage.used_money.as_ref().map(|money| money.amount_minor),
        Some(12_000)
    );

    let bare = snapshot(r#"{"data":{"total_credits":0.0,"total_usage":0.0}}"#);
    assert_eq!(
        bare.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(bare.account.provider_label, "OpenRouter");
    let bare = account_credits_bucket(&bare);
    assert_eq!(bare.remaining_percent, None);
}

#[test]
fn openrouter_money_derived_remaining_stays_continuous_and_signed_ratio_stays_raw() {
    use jackin_protocol::control::PercentObservationOrigin::Derived;

    let server = CannedSnapshotServer::start_with_credits(
        key_fixture(),
        r#"{"data":{"total_credits":0.03,"total_usage":0.01}}"#.to_owned(),
    );
    let snapshot = openrouter_snapshot_with_base(
        "opencode",
        Some("fixture-key"),
        server.base_url(),
        1_780_000_000,
    );
    assert_eq!(
        snapshot.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(snapshot.account.provider_label, "OpenRouter");
    let fractional = account_credits_bucket(&snapshot);
    let remaining = fractional
        .remaining_percent
        .map(jackin_protocol::control::CanonicalRemainingPercent::get)
        .expect("derived remaining");
    assert!((remaining - (200.0 / 3.0)).abs() < 1e-12);
    assert_eq!(fractional.remaining_percent_origin, Some(Derived));
    assert_eq!(
        fractional
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(1)
    );
    assert_eq!(
        fractional
            .limit_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(3)
    );
    assert!(
        (jackin_usage_provider_core::money_used_percent(fractional)
            .expect("money ratio")
            .get()
            - (100.0 / 3.0))
            .abs()
            < 1e-12
    );

    let negative_used = openrouter_credits_bucket(-2_500, 10_000);
    assert_eq!(
        negative_used
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(125.0)
    );
    assert_eq!(negative_used.remaining_percent_origin, Some(Derived));
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(&negative_used)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(-25.0)
    );
    assert_eq!(
        negative_used
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(-2_500)
    );
}

#[test]
fn openrouter_base_url_prefers_api_url_override() {
    assert_eq!(
        openrouter_base_url_from(None, None),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        openrouter_base_url_from(Some("https://api.test/"), Some("https://base.test")),
        "https://api.test"
    );
    assert_eq!(
        openrouter_base_url_from(None, Some("https://base.test/")),
        "https://base.test"
    );
    assert_eq!(
        openrouter_base_url_from(Some("  "), Some("https://base.test")),
        "https://base.test"
    );
}

#[test]
fn openrouter_snapshot_public_failures_keep_diagnostics_without_rate_limit() {
    const TEST_NAME: &str =
        "tests::case_02::openrouter_snapshot_public_failures_keep_diagnostics_without_rate_limit";

    if let Ok(child_state) = std::env::var("JACKIN_OPENROUTER_TEST_CHILD") {
        let (scenario, completion_marker) = child_state
            .split_once('|')
            .expect("child state includes a scenario and completion marker");
        let base_url =
            std::env::var("OPENROUTER_BASE_URL").expect("child has an owned loopback endpoint");
        assert!(base_url.starts_with("http://127.0.0.1:"));
        let (view, rate_limit) =
            openrouter_snapshot_with_rate_limit("opencode", Some("fixture-key"), 1_780_000_000);
        assert_eq!(
            view.status,
            jackin_protocol::control::UsageSnapshotStatus::Error
        );
        assert_eq!(view.account.provider_label, "OpenRouter");
        let error = view.last_error.as_deref().expect("public error diagnostic");
        match scenario {
            "connection-close" => {
                assert!(
                    error.starts_with("OpenRouter key request failed:"),
                    "unexpected request diagnostic: {error}"
                );
            }
            "decode-invalid-json" => {
                assert!(
                    error.starts_with("OpenRouter key decode failed:"),
                    "unexpected decode diagnostic: {error}"
                );
            }
            _ => panic!("unknown child scenario: {scenario}"),
        }
        assert_eq!(rate_limit, None);
        println!("{completion_marker}");
        return;
    }

    for (scenario, response) in [
        ("connection-close", ChildSnapshotResponse::Close),
        (
            "decode-invalid-json",
            ChildSnapshotResponse::Http {
                status: 200,
                reason: "OK",
                header: None,
                body: "not-json",
            },
        ),
    ] {
        let server = ChildSnapshotServer::start(response);
        let completion_marker = format!(
            "OPENROUTER_CHILD_COMPLETE:{scenario}:{}",
            std::process::id()
        );
        let output = run_snapshot_child(&server, TEST_NAME, scenario, &completion_marker);
        assert_snapshot_child_completed(&output, server.request_seen(), &completion_marker);
    }
}

#[test]
fn openrouter_snapshot_public_429_preserves_retry_after_deadline() {
    const TEST_NAME: &str =
        "tests::case_02::openrouter_snapshot_public_429_preserves_retry_after_deadline";

    if let Ok(child_state) = std::env::var("JACKIN_OPENROUTER_TEST_CHILD") {
        let (scenario, completion_marker) = child_state
            .split_once('|')
            .expect("child state includes a scenario and completion marker");
        assert_eq!(scenario, "http-429-retry-after");
        let base_url =
            std::env::var("OPENROUTER_BASE_URL").expect("child has an owned loopback endpoint");
        assert!(base_url.starts_with("http://127.0.0.1:"));
        let before_call_epoch = jackin_usage_provider_core::now_epoch();
        let request_now = before_call_epoch.saturating_sub(120);
        let (view, rate_limit) =
            openrouter_snapshot_with_rate_limit("opencode", Some("fixture-key"), request_now);
        let after_call_epoch = jackin_usage_provider_core::now_epoch();
        assert_eq!(
            view.status,
            jackin_protocol::control::UsageSnapshotStatus::Error
        );
        assert_eq!(view.account.provider_label, "OpenRouter");
        let error = view.last_error.as_deref().expect("public error diagnostic");
        assert!(
            error.starts_with("OpenRouter key HTTP 429"),
            "unexpected HTTP status diagnostic: {error}"
        );
        let retry_at_epoch = rate_limit
            .expect("429 response retains a public rate-limit observation")
            .retry_at_epoch
            .expect("Retry-After produces a public deadline");
        assert!(
            retry_at_epoch >= request_now.saturating_add(100),
            "retry deadline {retry_at_epoch} is not anchored after request time {request_now}"
        );
        assert!(
            retry_at_epoch >= before_call_epoch.saturating_add(37),
            "retry deadline {retry_at_epoch} precedes the observed response window"
        );
        assert!(
            retry_at_epoch <= after_call_epoch.saturating_add(37),
            "retry deadline {retry_at_epoch} exceeds the observed response window"
        );
        println!("{completion_marker}");
        return;
    }

    let server = ChildSnapshotServer::start(ChildSnapshotResponse::Http {
        status: 429,
        reason: "Too Many Requests",
        header: Some(("Retry-After", "37")),
        body: "{}",
    });
    let completion_marker = format!(
        "OPENROUTER_CHILD_COMPLETE:http-429-retry-after:{}",
        std::process::id()
    );
    let output = run_snapshot_child(
        &server,
        TEST_NAME,
        "http-429-retry-after",
        &completion_marker,
    );
    assert_snapshot_child_completed(&output, server.request_seen(), &completion_marker);
}
