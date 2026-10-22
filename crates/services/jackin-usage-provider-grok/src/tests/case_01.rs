// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CanonicalUsedPercent, PercentObservationOrigin, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};

#[test]
fn grok_omitted_weekly_percent_is_unknown_not_zero() {
    let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-17T00:00:00Z",
                "end": "2026-08-24T00:00:00Z"
            }
        }
    }))
    .expect("omitted percent");
    let buckets = response.buckets(1_755_000_000);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].used_percent, None);
    assert_eq!(buckets[0].pace_label.as_deref(), Some("No data"));
    assert_eq!(buckets[0].status_slot, None);
    assert!(buckets[0].resets_at.is_some());

    let zero: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": 0.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-17T00:00:00Z",
                "end": "2026-08-24T00:00:00Z"
            }
        }
    }))
    .expect("explicit zero");
    let buckets = zero.buckets(1_755_000_000);
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].used_percent,
        Some(CanonicalUsedPercent::new(0.0).unwrap())
    );
    assert_eq!(
        buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
}

#[test]
fn grok_omitted_fallback_used_is_unknown_not_zero() {
    let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "monthlyLimit": {"val": 30000},
            "billingPeriodStart": "2026-08-01T00:00:00Z",
            "billingPeriodEnd": "2026-09-01T00:00:00Z"
        }
    }))
    .expect("limit without used");
    let buckets = response.buckets(1_754_000_000);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Monthly");
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].used_percent, None);
    assert_eq!(buckets[0].pace_label.as_deref(), Some("No data"));
}

#[test]
fn grok_negative_cents_never_mirror_into_bounds() {
    let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "on_demand_enabled": true,
        "config": {
            "monthlyLimit": {"val": -30000},
            "used": {"val": 5000},
            "billingPeriodStart": "2026-08-01T00:00:00Z",
            "billingPeriodEnd": "2026-09-01T00:00:00Z",
            "prepaidBalance": {"val": -2500},
            "onDemandCap": {"val": -4000},
            "onDemandUsed": {"val": 300}
        }
    }))
    .expect("negative cents");
    assert!(
        response.buckets(1_754_000_000).is_empty(),
        "no headline, no prepaid row, no on-demand row"
    );

    let partial: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "on_demand_enabled": true,
        "config": {"onDemandCap": {"val": 4000}}
    }))
    .expect("cap without used");
    let buckets = partial.buckets(1_780_315_200);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "On-demand usage");
    assert_eq!(buckets[0].used_label, None);
    assert!(buckets[0].used_money.is_none());
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$40"));
}

#[test]
fn grok_monthly_period_never_renders_weekly_percent_meter() {
    let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": 40.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_MONTHLY",
                "start": "2026-08-01T00:00:00Z",
                "end": "2026-09-01T00:00:00Z"
            }
        }
    }))
    .expect("monthly period");
    assert!(
        response.buckets(1_755_000_000).is_empty(),
        "monthly-only accounts honestly blank without a monthly-limit fallback"
    );
}

#[test]
fn grok_monthly_fallback_accepts_period_start_aliases() {
    let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "monthlyLimit": {"val": 30000},
            "used": {"val": 5000},
            "periodStart": "2026-08-01T00:00:00Z",
            "periodEnd": "2026-09-01T00:00:00Z"
        }
    }))
    .expect("period aliases");
    let buckets = response.buckets(1_754_000_000);
    assert_eq!(buckets[0].label, "Monthly");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].used_percent, None);
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].used_percent_origin, None);
    assert_eq!(buckets[0].used_label.as_deref(), Some("$50"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$300"));
    assert_eq!(
        buckets[0]
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(5000)
    );
    assert_eq!(
        buckets[0]
            .limit_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(30000)
    );
}

#[test]
fn grok_keeps_fractional_and_over_cap_used_percent_points_raw() {
    for (reported, label) in [(43.375, "43.38% used"), (150.0, "150% used")] {
        let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
            "config": {
                "creditUsagePercent": reported,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-17T00:00:00Z",
                    "end": "2026-08-24T00:00:00Z"
                }
            }
        }))
        .expect("reported percentage parses");
        let bucket = response.buckets(1_755_000_000).remove(0);
        assert_eq!(
            bucket.used_percent.map(CanonicalUsedPercent::get),
            Some(reported)
        );
        assert_eq!(bucket.used_label.as_deref(), Some(label));
        assert_eq!(bucket.remaining_percent, None);
        assert_eq!(
            bucket.used_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
    }
}

#[test]
fn grok_malformed_explicit_percent_does_not_fall_back_to_monthly_money() {
    for bad in [
        serde_json::json!(null),
        serde_json::json!("bad"),
        serde_json::json!(-0.25),
    ] {
        let response: GrokBillingResponse = serde_json::from_value(serde_json::json!({
            "config": {
                "creditUsagePercent": bad,
                "monthlyLimit": {"val": 30000},
                "used": {"val": 5000},
                "billingPeriodStart": "2026-08-01T00:00:00Z",
                "billingPeriodEnd": "2026-09-01T00:00:00Z"
            }
        }))
        .expect("malformed percentage remains isolated in decoded response");
        let bucket = response.buckets(1_754_000_000).remove(0);
        assert_eq!(bucket.status, UsageSnapshotStatus::Unavailable);
        assert_eq!(bucket.used_percent, None);
        assert_eq!(bucket.used_money, None);
        assert_eq!(
            bucket.pace_label.as_deref(),
            Some("Grok used percent is invalid")
        );
    }
}

#[test]
fn grok_settings_tier_prefers_display_form() {
    let settings = serde_json::json!({
        "subscription_tier_display": "SuperGrok Heavy",
        "subscription_tier": "supergrok_heavy"
    });
    assert_eq!(
        grok_tier_from_settings(&settings).as_deref(),
        Some("SuperGrok Heavy")
    );
    let machine = serde_json::json!({"subscriptionTier": "supergrok"});
    assert_eq!(
        grok_tier_from_settings(&machine).as_deref(),
        Some("supergrok")
    );
    let blank = serde_json::json!({"subscription_tier": "  "});
    assert_eq!(grok_tier_from_settings(&blank), None);
}

#[test]
fn grok_status_classification_requires_typed_http_status() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
    let ordinary = ProviderError::from(ProviderHttpError::Transport(
        "transport failed while contacting port 429".to_owned(),
    ));
    let (_, ordinary_rate_limit) = grok_snapshot_from_rpc_result_with_rate_limit(
        "grok",
        1_780_315_200,
        missing,
        true,
        false,
        false,
        Err(ordinary),
    );
    assert_eq!(ordinary_rate_limit, None);

    let typed = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 429,
        message: "Grok billing HTTP 429".to_owned(),
        retry_after_seconds: Some(97),
        response_received_at_epoch: Some(1_780_315_200),
    });
    let enriched = typed.with_message(
        "Grok billing HTTP 429; Grok ACP billing failed: agent unavailable".to_owned(),
    );
    let (view, rate_limit) = grok_snapshot_from_rpc_result_with_rate_limit(
        "grok",
        1_780_315_200,
        missing,
        true,
        false,
        false,
        Err(typed),
    );
    assert_eq!(
        rate_limit,
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_780_315_297),
        })
    );
    assert_eq!(view.last_error.as_deref(), Some("Grok billing HTTP 429"));

    let (view, rate_limit) = grok_snapshot_from_rpc_result_with_rate_limit(
        "grok",
        1_780_315_200,
        missing,
        true,
        false,
        false,
        Err(enriched),
    );
    assert_eq!(
        rate_limit,
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_780_315_297),
        })
    );
    assert_eq!(
        view.last_error.as_deref(),
        Some("Grok billing HTTP 429; Grok ACP billing failed: agent unavailable")
    );
}

#[test]
fn grok_subscription_auth_outranks_ambient_keys() {
    assert_eq!(
        resolve_grok_billing_auth(true, true, true),
        GrokBillingAuth::Subscription
    );
    assert_eq!(
        resolve_grok_billing_auth(false, true, false),
        GrokBillingAuth::EnvKeyOnly
    );
    assert_eq!(
        resolve_grok_billing_auth(false, false, true),
        GrokBillingAuth::EnvKeyOnly
    );
    assert_eq!(
        resolve_grok_billing_auth(false, false, false),
        GrokBillingAuth::None
    );
}

#[test]
fn grok_snapshot_freshness_requires_quantitative_billing_data() {
    let auth = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
    let now = 1_780_315_200;
    let snapshot = |payload| {
        let response: GrokBillingResponse =
            serde_json::from_value(payload).expect("billing response fixture");
        grok_snapshot_from_rpc_result(
            "grok",
            now,
            auth,
            true,
            false,
            false,
            Ok(GrokBillingSnapshot::Rest(Box::new(response))),
        )
    };

    for payload in [
        serde_json::json!({"subscription_tier": "SuperGrok", "config": {}}),
        serde_json::json!({
            "subscription_tier": "SuperGrok",
            "config": {
                "creditUsagePercent": 43.375,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_MONTHLY",
                    "start": "2026-08-01T00:00:00Z",
                    "end": "2026-09-01T00:00:00Z"
                }
            }
        }),
    ] {
        let view = snapshot(payload);
        assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
        assert_eq!(view.source, UsageSource::None);
        assert_eq!(view.confidence, UsageConfidence::None);
        assert_eq!(view.account.account_label, "local Grok auth");
        assert_eq!(view.account.plan_label.as_deref(), Some("SuperGrok"));
        assert_eq!(view.fetched_at_epoch, now);
        assert_eq!(view.updated_label, "Unavailable");
        assert!(
            view.last_error
                .as_deref()
                .is_some_and(|error| { error.contains("no quantitative observation") })
        );
    }

    let reset_only = snapshot(serde_json::json!({
        "subscription_tier": "SuperGrok",
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-17T00:00:00Z",
                "end": "2026-08-24T00:00:00Z"
            }
        }
    }));
    assert_eq!(reset_only.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        reset_only.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert!(reset_only.buckets[0].resets_at.is_some());
    assert!(
        reset_only
            .last_error
            .as_deref()
            .is_some_and(|error| { error.contains("No data") })
    );

    let all_invalid = snapshot(serde_json::json!({
        "subscription_tier": "SuperGrok",
        "config": {
            "creditUsagePercent": "bad",
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-17T00:00:00Z",
                "end": "2026-08-24T00:00:00Z"
            }
        }
    }));
    assert_eq!(all_invalid.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        all_invalid.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert!(all_invalid.buckets[0].resets_at.is_some());
    assert!(
        all_invalid
            .last_error
            .as_deref()
            .is_some_and(|error| { error.contains("Grok used percent is invalid") })
    );

    let valid_sibling = snapshot(serde_json::json!({
        "subscription_tier": "SuperGrok",
        "on_demand_enabled": true,
        "config": {
            "creditUsagePercent": "bad",
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-17T00:00:00Z",
                "end": "2026-08-24T00:00:00Z"
            },
            "onDemandCap": {"val": 4000},
            "onDemandUsed": {"val": 0}
        }
    }));
    assert_eq!(valid_sibling.status, UsageSnapshotStatus::Fresh);
    assert_eq!(valid_sibling.source, UsageSource::ProviderApi);
    assert_eq!(valid_sibling.confidence, UsageConfidence::Authoritative);
    assert_eq!(
        valid_sibling.account.plan_label.as_deref(),
        Some("SuperGrok")
    );
    assert_eq!(valid_sibling.buckets.len(), 2);
    assert_eq!(
        valid_sibling.buckets[0].status,
        UsageSnapshotStatus::Unavailable
    );
    assert_eq!(
        valid_sibling.buckets[1]
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(0)
    );
    assert!(
        valid_sibling
            .last_error
            .as_deref()
            .is_some_and(|error| { error.contains("Grok used percent is invalid") })
    );
}

#[test]
fn healthy_grok_pace_labels_are_not_snapshot_errors() {
    let auth = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
    let snapshot = |now, payload| {
        let response: GrokBillingResponse =
            serde_json::from_value(payload).expect("billing response fixture");
        grok_snapshot_from_rpc_result(
            "grok",
            now,
            auth,
            true,
            false,
            false,
            Ok(GrokBillingSnapshot::Rest(Box::new(response))),
        )
    };

    let weekly = snapshot(
        1_787_187_132,
        serde_json::json!({
            "subscription_tier": "SuperGrok",
            "config": {
                "creditUsagePercent": 43.375,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-17T00:00:00Z",
                    "end": "2026-08-24T00:00:00Z"
                }
            }
        }),
    );
    assert_eq!(weekly.status, UsageSnapshotStatus::Fresh);
    assert_eq!(weekly.buckets[0].pace_label.as_deref(), Some("On pace"));
    assert_eq!(weekly.last_error, None);

    let zero = snapshot(
        1_787_187_132,
        serde_json::json!({
            "subscription_tier": "SuperGrok",
            "config": {
                "creditUsagePercent": 0.0,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-17T00:00:00Z",
                    "end": "2026-08-24T00:00:00Z"
                }
            }
        }),
    );
    assert_eq!(zero.status, UsageSnapshotStatus::Fresh);
    assert!(zero.buckets[0].pace_label.is_some());
    assert_eq!(zero.last_error, None);

    let money = snapshot(
        1_787_227_200,
        serde_json::json!({
            "subscription_tier": "SuperGrok",
            "config": {
                "monthlyLimit": {"val": 30000},
                "used": {"val": 15000},
                "billingPeriodStart": "2026-08-01T00:00:00Z",
                "billingPeriodEnd": "2026-09-01T00:00:00Z"
            }
        }),
    );
    assert_eq!(money.status, UsageSnapshotStatus::Fresh);
    assert!(money.buckets[0].pace_label.is_some());
    assert_eq!(
        money.buckets[0].used_money.as_ref().unwrap().amount_minor,
        15000
    );
    assert_eq!(money.last_error, None);
}
