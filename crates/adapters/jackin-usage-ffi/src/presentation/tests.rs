use super::*;
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageMetricScopeV1};

#[test]
fn spend_cap_labels_preserve_exact_money_digits() {
    let group = UsageMetricGroupV1 {
        group_id: "spend".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 0,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_001, "USD", 18)),
            spent: Some(Money::new(100_000_000_000_000_000, "USD", 18)),
            remaining: Some(Money::new(1, "USD", 18)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    let bucket = spend_cap_bucket(&group).expect("spend cap bucket");
    assert_eq!(bucket.used_label.as_deref(), Some("$0.100000000000000000"));
    assert_eq!(bucket.limit_label.as_deref(), Some("$0.100000000000000001"));
    assert_eq!(
        metric_group_summary(&group),
        "spent $0.100000000000000000 · cap $0.100000000000000001 · remaining $0.000000000000000001"
    );
}

#[test]
fn spend_cap_labels_preserve_mixed_money_scales() {
    let group = UsageMetricGroupV1 {
        group_id: "spend-mixed-scale".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 0,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
            spent: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
            remaining: Some(Money::new(9_000_000_000_000_000_001, "USD", 2)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };
    group
        .validate(0)
        .expect("mixed-scale spend group validates");

    let bucket = spend_cap_bucket(&group).expect("spend cap bucket");
    assert_eq!(bucket.limit_label.as_deref(), Some("$100000000000000000"));
    assert_eq!(bucket.used_label.as_deref(), Some("$9999999999999999.99"));
    assert_eq!(
        metric_group_summary(&group),
        "spent $9999999999999999.99 · cap $100000000000000000 · remaining $90000000000000000.01"
    );
}
