// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    PercentObservationOrigin, UsageConfidence, UsageMeterSide, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::parse_iso_epoch;

#[test]
fn opencode_percent_spellings_and_used_limit_fallback() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 20, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"usagePercent": 30, "reset_at": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"used": 5, "limit": 100, "resetInSec": 3600}),
            ),
        ),
        1_776_000_000,
    )
    .expect("tolerant fixture");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(
        quota.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(20.0)
    );
    assert_eq!(
        quota.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(30.0)
    );
    assert_eq!(
        quota.buckets[2]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(5.0)
    );
    assert_eq!(quota.buckets[0].remaining_percent, None);
    assert_eq!(
        quota.buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        quota.buckets[2].used_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
    assert_eq!(quota.buckets[2].resets_at, Some(1_776_003_600));
}

#[test]
fn opencode_one_percent_means_one_percent() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 1, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    )
    .expect("one-percent fixture");
    assert_eq!(
        quota.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(1.0)
    );
}

#[test]
fn opencode_per_model_and_zen_fields_render_nothing() {
    let mut payload = usage_fixture(
        window_fixture(
            "ok",
            serde_json::json!({
                "percent": 20,
                "resetsAt": "2026-08-21T12:00:00Z",
                "models": {"acme-model": {"percent": 99}}
            }),
        ),
        window_fixture(
            "ok",
            serde_json::json!({"percent": 10, "resetsAt": "2026-08-25T12:00:00Z"}),
        ),
        window_fixture(
            "ok",
            serde_json::json!({"percent": 5, "resetsAt": "2026-09-01T12:00:00Z"}),
        ),
    );
    payload["zenBalanceUSD"] = serde_json::json!(42.0);
    let quota = parse_opencode_usage(payload, 1_776_000_000).expect("extra fields");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(
        quota.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(20.0)
    );
}

#[test]
fn opencode_entitlement_403_is_typed_apart_from_key_failure() {
    let entitlement = classify_opencode_http_error(
        403,
        r#"{"error": {"type": "EntitlementError", "message": "OpenCode Go subscription required."}}"#,
    );
    assert!(matches!(entitlement, OpenCodeUsageError::Entitlement(_)));
    let entitlement_on_odd_status =
        classify_opencode_http_error(200, r#"{"error": {"type": "EntitlementError"}}"#);
    assert!(matches!(
        entitlement_on_odd_status,
        OpenCodeUsageError::Entitlement(_)
    ));
    let auth = classify_opencode_http_error(
        401,
        r#"{"error": {"type": "AuthError", "message": "Invalid key"}}"#,
    );
    assert!(matches!(auth, OpenCodeUsageError::Key(_)));
    let bare_403 = classify_opencode_http_error(403, "forbidden");
    assert!(matches!(bare_403, OpenCodeUsageError::Entitlement(_)));
    let server = classify_opencode_http_error(500, "boom");
    assert!(matches!(server, OpenCodeUsageError::Http(_)));
}

#[test]
fn opencode_missing_or_malformed_window_does_not_fail_siblings() {
    let missing = parse_opencode_usage(serde_json::json!({"usage": {}}), 1_776_000_000);
    let missing = missing.expect("one absent window does not fail the response");
    assert_eq!(missing.buckets.len(), 3);
    assert_eq!(missing.buckets[0].status, UsageSnapshotStatus::Unavailable);
    assert_eq!(missing.buckets[1].status, UsageSnapshotStatus::Unavailable);
    assert_eq!(missing.buckets[2].status, UsageSnapshotStatus::Unavailable);

    let malformed = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": -5, "used": 1, "limit": 2}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    )
    .expect("malformed percent is local to its window");
    assert_eq!(
        malformed.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert_eq!(malformed.buckets[0].used_percent, None);
    assert_eq!(malformed.buckets[0].used_money, None);
    assert_eq!(malformed.buckets[0].remaining_percent, None);
    assert_eq!(malformed.buckets[1].status, UsageSnapshotStatus::Fresh);
    assert_eq!(
        malformed.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.0)
    );
}

#[test]
fn opencode_profile_outcome_requires_a_usable_quantity_and_keeps_partial_data() {
    let now = 1_776_000_000;
    let empty = parse_opencode_usage(serde_json::json!({"usage": {}}), now)
        .expect("empty usage remains a valid partial response");
    let view = opencode_profile_view("opencode", Ok(empty), now);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert_eq!(view.updated_label, "Unavailable");
    assert_eq!(view.account.account_label, "OpenCode account (unresolved)");
    assert_eq!(view.account.plan_label.as_deref(), Some("OpenCode Go"));
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| { error.contains("no quantitative observation") })
    );

    let reset_only = parse_opencode_usage(
        usage_fixture(
            window_fixture("ok", serde_json::json!({"resetInSec": 3600})),
            window_fixture(
                "ok",
                serde_json::json!({"resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"reset_at": "2026-09-01T12:00:00Z"}),
            ),
        ),
        now,
    )
    .expect("reset-only windows remain parseable");
    let view = opencode_profile_view("opencode", Ok(reset_only), now);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.status == UsageSnapshotStatus::Unavailable)
    );
    assert!(view.buckets.iter().all(|bucket| bucket.resets_at.is_some()));
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| { error.contains("no quantitative observation") })
    );

    let all_invalid = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({
                    "percent": -5,
                    "resetsAt": "2026-08-21T12:00:00Z"
                }),
            ),
            window_fixture("ok", serde_json::json!({"percent": "bad"})),
            window_fixture("pending", serde_json::json!({})),
        ),
        now,
    )
    .expect("malformed windows remain isolated");
    let view = opencode_profile_view("opencode", Ok(all_invalid), now);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert_eq!(view.fetched_at_epoch, now);
    assert_eq!(
        view.buckets[0].resets_at,
        parse_iso_epoch("2026-08-21T12:00:00Z")
    );
    assert!(view.last_error.as_deref().is_some_and(|error| {
        error.contains("percentage is invalid") && error.contains("window status is unavailable")
    }));

    let valid_sibling = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({
                    "percent": -5,
                    "resetsAt": "2026-08-21T12:00:00Z"
                }),
            ),
            window_fixture("ok", serde_json::json!({"percent": 43.375})),
            window_fixture("ok", serde_json::json!({})),
        ),
        now,
    )
    .expect("one valid sibling survives invalid windows");
    let view = opencode_profile_view("opencode", Ok(valid_sibling), now);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::ProviderApi);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.buckets[0].status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        view.buckets[0].resets_at,
        parse_iso_epoch("2026-08-21T12:00:00Z")
    );
    assert_eq!(
        view.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| { error.contains("OpenCode usage percentage is invalid") })
    );

    let rate_limited = parse_opencode_usage(
        usage_fixture(
            window_fixture("rate-limited", serde_json::json!({"percent": 40})),
            window_fixture("ok", serde_json::json!({"percent": 20})),
            window_fixture("ok", serde_json::json!({})),
        ),
        now,
    )
    .expect("rate limit keeps observed siblings");
    let view = opencode_profile_view("opencode", Ok(rate_limited), now);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        view.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(40.0)
    );
    assert_eq!(
        view.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(20.0)
    );
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| error.contains("rate limited"))
    );
}

#[test]
fn opencode_over_cap_keeps_raw_percent_without_failing_siblings() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 101, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 30, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    )
    .expect("over-cap is a valid reading");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(quota.buckets[0].used_label.as_deref(), Some("101% used"));
    assert_eq!(
        quota.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(101.0)
    );
    assert_eq!(quota.buckets[0].remaining_percent, None);
    assert_eq!(
        quota.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(30.0)
    );
    assert_eq!(quota.buckets[1].used_label.as_deref(), Some("30% used"));
}

#[test]
fn opencode_fractional_points_round_only_at_display_edge() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture("ok", serde_json::json!({"percent": 43.375})),
            window_fixture("ok", serde_json::json!({"percent": 0.375})),
            window_fixture("ok", serde_json::json!({"percent": 150.0})),
        ),
        1_776_000_000,
    )
    .expect("fractional observations parse without reset values");
    assert_eq!(
        quota.buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(quota.buckets[0].used_label.as_deref(), Some("43.38% used"));
    assert_eq!(
        quota.buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.375)
    );
    assert_eq!(quota.buckets[1].used_label.as_deref(), Some("0.38% used"));
    assert_eq!(
        quota.buckets[2]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(150.0)
    );
    assert_eq!(quota.buckets[2].used_label.as_deref(), Some("150% used"));
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&quota.buckets[0]).1,
        Some(UsageMeterSide::Used)
    );
}

#[test]
fn opencode_malformed_explicit_alias_is_authoritative_over_fallbacks() {
    for explicit in [
        serde_json::json!(null),
        serde_json::json!("bad"),
        serde_json::json!(-0.5),
    ] {
        let quota = parse_opencode_usage(
            usage_fixture(
                window_fixture(
                    "ok",
                    serde_json::json!({"percent": explicit, "usagePercent": 20, "used": 10, "limit": 20}),
                ),
                window_fixture("ok", serde_json::json!({"percent": 1})),
                window_fixture("ok", serde_json::json!({"percent": 2})),
            ),
            1_776_000_000,
        )
        .expect("one bad explicit field does not fail siblings");
        assert_eq!(quota.buckets[0].status, UsageSnapshotStatus::Unavailable);
        assert_eq!(quota.buckets[0].used_percent, None);
        assert_eq!(
            quota.buckets[1]
                .used_percent
                .map(jackin_protocol::control::CanonicalUsedPercent::get),
            Some(1.0)
        );
    }
}
