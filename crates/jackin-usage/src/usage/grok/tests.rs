// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    assert_eq!(buckets[0].pace_label.as_deref(), Some("No data"));
    assert_eq!(buckets[0].status_slot, None);
    // The known period end still anchors the reset.
    assert!(buckets[0].resets_at.is_some());

    // Explicit zero still renders a full meter.
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
    assert_eq!(buckets[0].remaining_percent, Some(100));
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
    assert_eq!(buckets[0].pace_label.as_deref(), Some("No data"));
}

#[test]
fn grok_negative_cents_never_mirror_into_bounds() {
    // Negative limits/caps/balances are invalid, never mirrored positive.
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

    // Omitted on-demand used degrades to limit-only, never $0.
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
fn grok_billing_error_taxonomy_covers_rest_and_rpc() {
    assert_eq!(
        classify_grok_billing_error("Grok billing HTTP 401"),
        GrokBillingErrorKind::Auth
    );
    assert_eq!(
        classify_grok_billing_error("Grok Bearer [REDACTED] is expired"),
        GrokBillingErrorKind::Auth
    );
    assert_eq!(
        classify_grok_billing_error("Grok billing HTTP 429"),
        GrokBillingErrorKind::RateLimited
    );
    assert_eq!(
        classify_grok_billing_error("Grok RPC timed out waiting for x.ai/billing"),
        GrokBillingErrorKind::Timeout
    );
    assert_eq!(
        classify_grok_billing_error("Grok billing shape unsupported: foo"),
        GrokBillingErrorKind::Decode
    );
    assert_eq!(
        classify_grok_billing_error("Grok RPC x.ai/billing failed: boom"),
        GrokBillingErrorKind::Rpc
    );
    assert_eq!(
        classify_grok_billing_error("Grok billing request failed: reset"),
        GrokBillingErrorKind::Transport
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
