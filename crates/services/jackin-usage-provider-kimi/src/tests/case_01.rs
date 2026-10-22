// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, PercentObservationOrigin,
    UsageConfidence, UsageMeterSide, UsageSnapshotStatus, UsageSource,
};

fn assert_near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < f64::EPSILON * 256.0);
}

#[test]
fn kimi_code_api_pools_map_rolling_weekly_monthly() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {
            "limit": 1000,
            "used": 220,
            "reset_time": "2026-06-18T12:00:00Z",
            "title": "Weekly summary"
        },
        "limits": [{
            "window": { "duration": 5, "timeUnit": "TIME_UNIT_HOUR" },
            "detail": { "limit": 200, "remaining": 150 }
        }],
        "usages": {
            "limit_5h": {
                "limit": 200,
                "used": 50,
                "reset_time": "2026-06-11T16:00:00Z"
            },
            "limit_7d": { "used_ratio": 0.22, "reset_at": 1_782_000_000_i64 },
            "limit_month_total": {
                "limit": "5000",
                "used": "1000",
                "resetTime": "2026-07-01T00:00:00Z"
            }
        },
        "user": {
            "id": "user-123",
            "email": "dev@example.test",
            "membership": { "level": "LEVEL_INTERMEDIATE" }
        },
        "version": "GOODS_VERSION_V1"
    }))
    .expect("valid Kimi Code API usage");

    assert!(matches!(usage.usages.as_ref(), Some(KimiUsages::Pools(_))));
    let buckets = usage.buckets(1_781_185_560);
    assert_eq!(buckets.len(), 3);
    assert_eq!(buckets[0].label, "5-hour");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].used_label.as_deref(), Some("50"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("200"));
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].used_percent, None);
    let five_hour = buckets[0]
        .count_quota
        .expect("independent count observations");
    assert_eq!(
        (five_hour.used, five_hour.limit, five_hour.remaining),
        (Some(50), Some(200), None)
    );
    assert_eq!(five_hour.unit, CountQuotaUnit::Unknown);
    assert_eq!(five_hour.period, CountQuotaPeriod::Unknown);
    assert_eq!(five_hour.provenance, CountQuotaProvenance::ProviderReported);
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(
        buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(22.0)
    );
    assert_eq!(
        buckets[1].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(buckets[1].resets_at, Some(1_782_000_000));
    assert_eq!(buckets[2].label, "Monthly");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(buckets[2].remaining_percent, None);
    assert_eq!(buckets[2].used_percent, None);
    let monthly = buckets[2].count_quota.expect("exact string counts");
    assert_eq!(
        (monthly.used, monthly.limit, monthly.remaining),
        (Some(1000), Some(5000), None)
    );
    assert_eq!(monthly.unit, CountQuotaUnit::Unknown);
    assert_eq!(monthly.period, CountQuotaPeriod::Unknown);

    let (account, username, plan) = kimi_account_identity(&usage);
    assert_eq!(account, "dev@example.test");
    assert_eq!(username.as_deref(), Some("dev@example.test"));
    assert_eq!(plan.as_deref(), Some("Allegretto"));
}

#[test]
fn kimi_usages_dispatches_arrays_as_lists_and_rejects_malformed_arrays() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": [{
            "scope": "FEATURE_CODING",
            "detail": { "limit": 1000, "used": 220, "remaining": 780 },
            "limits": [{
                "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                "detail": { "limit": 200, "remaining": 150 }
            }]
        }]
    }))
    .expect("list-shaped Kimi usage");

    assert!(matches!(usage.usages.as_ref(), Some(KimiUsages::List(_))));
    assert_eq!(
        usage
            .buckets(1_781_185_560)
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Rate Limit", "Weekly"]
    );

    let malformed: Result<KimiUsageResponse, _> = serde_json::from_value(serde_json::json!({
        "usages": [{ "limit_5h": { "limit": 200, "used": 50 } }]
    }));
    assert!(
        malformed.is_err(),
        "malformed array must not deserialize as pools"
    );
}

#[test]
fn kimi_membership_plan_mapping_is_version_gated() {
    assert_eq!(
        kimi_membership_plan("LEVEL_FREE", None),
        "Adagio".to_owned()
    );
    assert_eq!(
        kimi_membership_plan("LEVEL_ADVANCED", Some("GOODS_VERSION_V1")),
        "Allegro".to_owned()
    );
    assert_eq!(
        kimi_membership_plan("LEVEL_BASIC", Some("GOODS_VERSION_V2")),
        "Level Basic".to_owned()
    );
    assert_eq!(
        kimi_membership_plan("LEVEL_FOUNDER", None),
        "Founder".to_owned()
    );
}

#[test]
fn kimi_identity_falls_back_through_name_to_id() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "user": { "id": "user-9", "name": "Dev Nine" }
    }))
    .expect("identity-only usage");
    let (account, username, plan) = kimi_account_identity(&usage);
    assert_eq!(account, "Dev Nine");
    assert_eq!(username.as_deref(), Some("Dev Nine"));
    assert_eq!(plan, None);

    let anonymous: KimiUsageResponse =
        serde_json::from_value(serde_json::json!({})).expect("empty usage");
    assert_eq!(
        kimi_account_identity(&anonymous),
        (String::new(), None, None)
    );
}

#[test]
fn kimi_numeric_detail_and_snake_reset_parse() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {
            "limit": 1000,
            "remaining": 780,
            "reset_time": "2026-06-18T12:00:00Z"
        }
    }))
    .expect("numeric detail usage");
    let buckets = usage.buckets(1_781_185_560);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].used_label, None);
    assert_eq!(buckets[0].remaining_percent, None);
    let quota = buckets[0].count_quota.expect("exact independent counts");
    assert_eq!(
        (quota.used, quota.limit, quota.remaining),
        (None, Some(1000), Some(780))
    );
    assert_eq!(quota.unit, CountQuotaUnit::Unknown);
    assert_eq!(quota.period, CountQuotaPeriod::Unknown);
    assert!(buckets[0].resets_at.is_some());

    let ms: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": { "limit": 100, "used": 10, "reset_at": 1_782_000_000_000_i64 }
    }))
    .expect("ms reset usage");
    let buckets = ms.buckets(1_781_185_560);
    assert_eq!(buckets[0].resets_at, Some(1_782_000_000));
}

#[test]
fn kimi_extra_usage_wallet_maps_to_spend_bucket() {
    let usage: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "monthly_cap_cents": 2000,
            "monthly_used_cents": 500,
            "currency": "USD"
        }
    }))
    .expect("wallet usage");
    let wallet = usage.wallet().expect("no in-band error").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.label, "Extra usage");
    assert_eq!(view.status_slot, Some(StatusSlot::Spend));
    assert_eq!(view.remaining_percent, None);
    let ratio = jackin_usage_provider_core::money_used_percent(&view).expect("money ratio");
    assert_eq!(ratio.get(), 25.0);
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(500)
    );
    assert_eq!(
        view.used_money
            .as_ref()
            .map(|money| money.currency.as_str()),
        Some("USD")
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(2000)
    );

    let pair: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": { "balance": 7500, "total": 10000 }
    }))
    .expect("balance-pair usage");
    let wallet = pair.wallet().expect("ok").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.remaining_percent, None);
    assert_eq!(
        view.used_money
            .as_ref()
            .map(|money| money.currency.as_str()),
        Some("credits")
    );

    let failed: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "error": { "code": "UNAUTHENTICATED", "message": "bad credential" }
    }))
    .expect("error usage");
    assert_eq!(
        failed.wallet(),
        Err("Kimi local usage error: bad credential".to_owned())
    );
}

#[test]
fn kimi_unitless_wallet_keys_are_unknown_scale_never_money() {
    let usage: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": { "monthly_used": 500, "monthly_cap": 2000 }
    }))
    .expect("unitless wallet");
    let wallet = usage.wallet().expect("ok").expect("wallet");
    assert_eq!(wallet.monthly_used, None);
    assert_eq!(wallet.monthly_cap, None);
    let view = kimi_extra_usage_bucket(wallet).expect("label-only bucket");
    assert_eq!(view.label, "Extra usage");
    assert_eq!(view.used_label.as_deref(), Some("2000"));
    assert_eq!(
        view.pace_label.as_deref(),
        Some("monthly_cap · unknown scale")
    );
    assert!(view.used_money.is_none());
    assert!(view.limit_money.is_none());
    assert_eq!(view.status_slot, None);
    assert_eq!(view.remaining_percent, None);

    let mixed: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "monthly_used_cents": 500,
            "monthly_cap_cents": 2000,
            "used": 999
        }
    }))
    .expect("mixed wallet");
    let wallet = mixed.wallet().expect("ok").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.status_slot, Some(StatusSlot::Spend));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(500)
    );
}

#[test]
fn kimi_over_cap_keeps_raw_percent_with_clamped_bar() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": { "limit": 100, "used": 140 },
            "limit_7d": { "used_ratio": 150.0 }
        }
    }))
    .expect("over-cap pools");
    let buckets = usage.buckets(1_781_185_560);
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].used_percent, None);
    assert_eq!(buckets[0].pace_label, None);
    assert_eq!(buckets[0].used_label.as_deref(), Some("140"));
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(
        buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(15_000.0)
    );
    assert_eq!(buckets[1].pace_label.as_deref(), Some("15000% used"));
    let (meter, side) = jackin_usage_provider_core::select_quota_meter(&buckets[1]);
    assert_eq!(
        meter.map(jackin_protocol::control::MeterPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(side, Some(UsageMeterSide::Used));

    assert_eq!(kimi_over_cap_label(100.0), None);
    assert_eq!(kimi_over_cap_label(42.5), None);
    assert_eq!(kimi_over_cap_label(f64::NAN), None);
}

#[test]
fn kimi_count_values_preserve_u64_boundaries_without_deriving_fields() {
    for value in [0, 1, (1_u64 << 53) + 1, u64::MAX] {
        for count in [
            serde_json::json!(value),
            serde_json::json!(value.to_string()),
        ] {
            let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
                "usage": {"used": count.clone(), "limit": count.clone(), "remaining": count.clone()}
            }))
            .expect("valid unsigned count body");
            let bucket = response.buckets(1_781_185_560).remove(0);
            let quota = bucket.count_quota.expect("typed quota");
            assert_eq!(quota.used, Some(value));
            assert_eq!(quota.limit, Some(value));
            assert_eq!(quota.remaining, Some(value));
            assert_eq!(quota.unit, CountQuotaUnit::Unknown);
            assert_eq!(quota.period, CountQuotaPeriod::Unknown);
            assert_eq!(quota.provenance, CountQuotaProvenance::ProviderReported);
            assert_eq!(bucket.remaining_percent, None);
            assert_eq!(bucket.used_percent, None);
        }
    }
}

#[test]
fn kimi_count_fields_are_independent_and_unknown_units_do_not_select_count_geometry() {
    let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {"limit": 10, "remaining": 1}
    }))
    .expect("partial counts");
    let bucket = response.buckets(1_781_185_560).remove(0);
    let quota = bucket.count_quota.expect("partial count quota");
    assert_eq!(
        (quota.used, quota.limit, quota.remaining),
        (None, Some(10), Some(1))
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&bucket),
        (None, None)
    );

    let inconsistent: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {"used": 99, "limit": 2, "remaining": 7}
    }))
    .expect("independent observations do not need reconciliation");
    let quota = inconsistent
        .buckets(1_781_185_560)
        .remove(0)
        .count_quota
        .unwrap();
    assert_eq!(
        (quota.used, quota.limit, quota.remaining),
        (Some(99), Some(2), Some(7))
    );
}

#[test]
fn kimi_fractional_used_ratio_scales_uniformly_and_keeps_independent_counts() {
    let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": {"limit": 200, "used": 50, "used_ratio": 0.43375},
            "limit_7d": {"used_ratio": 0.00375},
            "limit_month_total": {"used_ratio": 100.125}
        }
    }))
    .expect("reported ratios");
    let buckets = response.buckets(1_781_185_560);
    assert_eq!(
        buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("50"));
    assert_eq!(buckets[0].count_quota.unwrap().used, Some(50));
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&buckets[0]),
        (None, None)
    );
    assert_eq!(
        buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.375)
    );
    assert_eq!(buckets[1].used_percent.unwrap().label(), "0.38% used");
    assert_eq!(
        buckets[2]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(10_012.5)
    );
    assert_eq!(buckets[2].used_percent.unwrap().label(), "10012.5% used");

    let ratio_only: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {"limit_5h": {"used_ratio": 0.43375}}
    }))
    .expect("ratio-only observation");
    let bucket = ratio_only.buckets(1_781_185_560).remove(0);
    let (meter, side) = jackin_usage_provider_core::select_quota_meter(&bucket);
    assert_eq!(
        meter.map(jackin_protocol::control::MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(side, Some(UsageMeterSide::Used));
}

#[test]
fn kimi_used_ratio_source_boundary_matrix_preserves_raw_counts_and_authority() {
    let now = 1_781_185_560;
    let reset_at = 1_782_000_000_i64;
    let cases = [
        (serde_json::json!(0.0), 0.0),
        (serde_json::json!(-0.0), 0.0),
        (serde_json::json!("-0"), 0.0),
        (serde_json::json!(0.00375), 0.375),
        (serde_json::json!(0.375), 37.5),
        (serde_json::json!("0.375"), 37.5),
        (serde_json::json!(0.999), 99.9),
        (serde_json::json!(1.0), 100.0),
        (serde_json::json!(1.001), 100.1),
        (serde_json::json!(43.375), 4_337.5),
        (serde_json::json!(100.125), 10_012.5),
        (serde_json::json!(150.0), 15_000.0),
    ];

    for (reported_ratio, expected_points) in cases {
        let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
            "usages": {
                "limit_5h": {
                    "limit": 200,
                    "used": 50,
                    "used_ratio": reported_ratio,
                    "reset_time": reset_at
                }
            },
            "user": {"email": "quota@example.test"}
        }))
        .expect("production Kimi usages payload");

        let bucket = response.buckets(now).remove(0);
        let used_percent = bucket.used_percent.expect("fraction converted to points");
        assert_near(used_percent.get(), expected_points);
        assert_eq!(
            bucket.used_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(bucket.remaining_percent, None);
        assert_eq!(bucket.resets_at, Some(reset_at));
        assert_eq!(bucket.status, UsageSnapshotStatus::Fresh);
        let count = bucket.count_quota.expect("independent exact counts");
        assert_eq!(
            (count.used, count.limit, count.remaining),
            (Some(50), Some(200), None)
        );
        assert_eq!(count.unit, CountQuotaUnit::Unknown);
        assert_eq!(count.period, CountQuotaPeriod::Unknown);
        assert_eq!(
            jackin_usage_provider_core::select_quota_meter(&bucket),
            (None, None),
            "unknown count units do not override the independent ratio"
        );
        if expected_points.partial_cmp(&0.375) == Some(std::cmp::Ordering::Equal) {
            assert_eq!(used_percent.label(), "0.38% used");
        }

        let snapshot = kimi_snapshot_from_usage("kimi", Some(&response), None, true, false, now);
        assert_eq!(snapshot.status, UsageSnapshotStatus::Fresh);
        assert_eq!(snapshot.source, UsageSource::ProviderApi);
        assert_eq!(snapshot.confidence, UsageConfidence::Authoritative);
        assert_eq!(snapshot.fetched_at_epoch, now);
        assert_eq!(snapshot.last_error, None);
        assert_near(
            snapshot.buckets[0]
                .used_percent
                .expect("final view retains reported used ratio")
                .get(),
            expected_points,
        );
        assert_eq!(
            snapshot.buckets[0].used_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(snapshot.buckets[0].resets_at, Some(reset_at));
        assert_eq!(
            snapshot.buckets[0]
                .count_quota
                .expect("final view retains independent counts")
                .used,
            Some(50)
        );
    }
}

#[test]
fn kimi_bad_window_does_not_discard_valid_sibling_or_account_freshness() {
    let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": {"limit": -1},
            "limit_7d": {"used_ratio": 0.57}
        },
        "user": {"email": "quota@example.test"}
    }))
    .expect("malformed count is retained as a local invalid observation");
    let observation = kimi_quota_observation(
        Some(&response),
        UsageSnapshotStatus::Fresh,
        None,
        1_781_185_560,
    );
    assert_eq!(observation.status, UsageSnapshotStatus::Fresh);
    assert!(observation.error.is_some());
    assert_eq!(observation.buckets.len(), 2);
    assert_eq!(
        observation.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert!(
        observation.buckets[0]
            .pace_label
            .as_deref()
            .unwrap()
            .contains("negative count")
    );
    assert_eq!(observation.buckets[1].status, UsageSnapshotStatus::Fresh);
    assert_near(
        observation.buckets[1]
            .used_percent
            .expect("reported ratio")
            .get(),
        57.0,
    );
    assert_eq!(observation.buckets[1].count_quota, None);

    let malformed_pool: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": null,
            "limit_7d": {"used_ratio": 0.57}
        }
    }))
    .expect("malformed pool shape remains local to its window");
    let observation = kimi_quota_observation(
        Some(&malformed_pool),
        UsageSnapshotStatus::Fresh,
        None,
        1_781_185_560,
    );
    assert_eq!(observation.status, UsageSnapshotStatus::Fresh);
    assert!(
        observation
            .error
            .as_deref()
            .unwrap()
            .contains("malformed pool")
    );
    assert_eq!(observation.buckets.len(), 2);
    assert_eq!(
        observation.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert_eq!(observation.buckets[1].status, UsageSnapshotStatus::Fresh);
    assert_near(
        observation.buckets[1]
            .used_percent
            .expect("reported ratio")
            .get(),
        57.0,
    );
}

#[test]
fn kimi_invalid_count_fields_are_unknown_while_valid_sibling_count_is_retained() {
    for invalid in [
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("-1"),
        serde_json::json!("18446744073709551616"),
        serde_json::Value::Null,
        serde_json::json!(false),
        serde_json::json!({"count": 1}),
    ] {
        let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
            "usage": {"limit": invalid, "used": 0, "remaining": 1}
        }))
        .expect("invalid count remains isolatable after decode");
        let bucket = response.buckets(1_781_185_560).remove(0);
        let quota = bucket.count_quota.expect("valid sibling counts retained");
        assert_eq!(quota.limit, None);
        assert_eq!(quota.used, Some(0));
        assert_eq!(quota.remaining, Some(1));
        assert_eq!(bucket.status, UsageSnapshotStatus::Fresh);
        assert!(bucket.pace_label.as_deref().is_some_and(|error| {
            error.contains("limit")
                && (error.contains("invalid")
                    || error.contains("negative")
                    || error.contains("unsupported"))
        }));
    }
}

#[test]
fn kimi_invalid_count_and_percent_sides_preserve_valid_independent_observations() {
    let invalid_count: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {"limit_5h": {"limit": -1, "used_ratio": 0.57}}
    }))
    .expect("invalid count does not invalidate an independent ratio");
    let bucket = invalid_count.buckets(1_781_185_560).remove(0);
    assert_eq!(bucket.status, UsageSnapshotStatus::Fresh);
    assert_eq!(bucket.count_quota, None);
    assert_near(bucket.used_percent.expect("reported ratio").get(), 57.0);
    assert_eq!(
        bucket.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert!(
        bucket
            .pace_label
            .as_deref()
            .is_some_and(|error| error.contains("negative count"))
    );
    assert_near(
        jackin_usage_provider_core::select_quota_meter(&bucket)
            .0
            .expect("percentage-only meter")
            .get(),
        57.0,
    );

    let invalid_percent: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {"limit_5h": {"limit": 10, "used": 0, "used_ratio": null}}
    }))
    .expect("invalid ratio is isolated from exact counts");
    let bucket = invalid_percent.buckets(1_781_185_560).remove(0);
    let counts = bucket.count_quota.expect("valid exact counts remain");
    assert_eq!(
        (counts.used, counts.limit, counts.remaining),
        (Some(0), Some(10), None)
    );
    assert_eq!(bucket.status, UsageSnapshotStatus::Fresh);
    assert_eq!(bucket.used_percent, None);
    assert_eq!(bucket.remaining_percent, None);
    assert!(
        bucket
            .pace_label
            .as_deref()
            .is_some_and(|error| error.contains("invalid reported used ratio"))
    );
}

#[test]
fn kimi_reset_and_labels_do_not_fabricate_unit_or_period() {
    let response: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {"limit": 5, "remaining": 0, "reset_at": 1_782_000_000},
        "limits": [{
            "window": {"duration": 1, "timeUnit": "TIME_UNIT_DAY"},
            "detail": {"limit": 5, "remaining": 0, "reset_at": 1_782_000_000}
        }]
    }))
    .expect("daily-looking reset and label");
    let bucket = response.buckets(1_781_185_560).remove(0);
    let quota = bucket.count_quota.expect("typed count");
    assert_eq!(quota.unit, CountQuotaUnit::Unknown);
    assert_eq!(quota.period, CountQuotaPeriod::Unknown);
    assert_eq!(quota.is_exhausted(), Some(true));
    assert_eq!(bucket.resets_at, Some(1_782_000_000));
}

#[test]
fn kimi_empty_and_nonquantitative_responses_are_not_fresh() {
    for response in [
        serde_json::json!({}),
        serde_json::json!({"usage": {"reset_time": "2026-06-18T12:00:00Z"}}),
        serde_json::json!({"usages": {"limit_5h": {"reset_at": 1_782_000_000_i64}}}),
    ] {
        let usage: KimiUsageResponse = serde_json::from_value(response).expect("valid empty body");
        let observation = kimi_quota_observation(
            Some(&usage),
            UsageSnapshotStatus::Fresh,
            None,
            1_781_185_560,
        );
        assert_eq!(observation.status, UsageSnapshotStatus::Unavailable);
        assert!(observation.error.is_some());
        assert!(observation.buckets.iter().all(|bucket| {
            bucket.status == UsageSnapshotStatus::Unavailable
                && bucket.used_percent.is_none()
                && bucket.count_quota.is_none()
        }));
    }
}

#[test]
fn kimi_final_view_clears_authority_for_an_unusable_success_response() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": {"used_ratio": -0.2, "reset_at": 1_782_000_000_i64}
        },
        "user": {
            "email": "quota@example.test",
            "membership": {"level": "LEVEL_ADVANCED"}
        },
        "version": "GOODS_VERSION_V1"
    }))
    .expect("invalid quota stays isolated from identity");
    let unavailable =
        kimi_snapshot_from_usage("kimi", Some(&usage), None, true, false, 1_781_185_560);
    assert_eq!(unavailable.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(unavailable.source, UsageSource::None);
    assert_eq!(unavailable.confidence, UsageConfidence::None);
    assert_eq!(unavailable.account.account_label, "quota@example.test");
    assert_eq!(unavailable.account.plan_label.as_deref(), Some("Allegro"));
    assert_eq!(unavailable.fetched_at_epoch, 1_781_185_560);
    assert_eq!(unavailable.updated_label, "Unavailable");
    assert!(unavailable.last_error.is_some());
    assert!(unavailable.buckets.iter().any(|bucket| {
        bucket.resets_at == Some(1_782_000_000) && bucket.status == UsageSnapshotStatus::Unavailable
    }));

    let zero_count: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {"limit_5h": {"limit": 10, "used": 0}},
        "user": {"email": "quota@example.test"}
    }))
    .expect("exact zero is an observed count");
    let fresh =
        kimi_snapshot_from_usage("kimi", Some(&zero_count), None, true, false, 1_781_185_560);
    assert_eq!(fresh.status, UsageSnapshotStatus::Fresh);
    assert_eq!(fresh.source, UsageSource::ProviderApi);
    assert_eq!(fresh.confidence, UsageConfidence::Authoritative);
    assert_eq!(
        fresh.buckets[0].count_quota.and_then(|quota| quota.used),
        Some(0)
    );
}

#[test]
fn kimi_usages_url_normalizes_coding_base_variants() {
    assert_eq!(
        kimi_usages_url_from_base(None),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://api.kimi.com/coding/v1")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://api.kimi.com/coding/")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("api.kimi.com")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://proxy.test/kimi/coding/v1/usages")),
        "https://proxy.test/kimi/coding/v1/usages"
    );
}
