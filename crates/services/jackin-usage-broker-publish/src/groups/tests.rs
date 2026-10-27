use super::*;

fn mixed_scale_bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Key Limit".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
        limit_money: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
        severity: jackin_protocol::control::UsageSeverity::Normal,
    }
}

#[test]
fn mixed_scale_spend_projection_preserves_associated_exact_amounts() {
    let bucket = mixed_scale_bucket();
    let remaining = spend_remaining(&bucket);
    assert_eq!(
        remaining,
        Some(Money::new(9_000_000_000_000_000_001, "USD", 2))
    );

    let group = project_spend_group("opaque", &bucket, UsageSnapshotStatus::Fresh, 1_000, 0)
        .expect("spend group projects");
    assert_eq!(group.label, "Key Limit spend");
    assert_eq!(group.quota_state, UsageQuotaStateV1::Available);
    match &group.value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &Some(Money::new(100_000_000_000_000_000, "USD", 0)));
            assert_eq!(spent, &Some(Money::new(999_999_999_999_999_999, "USD", 2)));
            assert_eq!(
                remaining,
                &Some(Money::new(9_000_000_000_000_000_001, "USD", 2))
            );
        }
        _ => panic!("spend projection retains its Money group"),
    }
    group
        .validate(0)
        .expect("mixed-scale spend group validates");
}

#[test]
fn spend_state_keeps_exact_integer_thresholds_across_scales() {
    let limit = Money::new(1_000_000_000_000_000_000, "USD", 0);
    assert_eq!(
        spend_ratio_state(&Money::new(799_999_999_999_999_999, "USD", 0), &limit),
        UsageQuotaStateV1::Available
    );
    assert_eq!(
        spend_ratio_state(&Money::new(800_000_000_000_000_000, "USD", 0), &limit),
        UsageQuotaStateV1::Warning
    );
    assert_eq!(
        spend_ratio_state(&Money::new(999_999_999_999_999_999, "USD", 0), &limit),
        UsageQuotaStateV1::Warning
    );
    assert_eq!(
        spend_ratio_state(&Money::new(100, "USD", 2), &Money::new(1, "USD", 0)),
        UsageQuotaStateV1::Exhausted
    );
}

#[test]
fn spend_remaining_clamps_overage_before_checked_conversion() {
    let mut bucket = mixed_scale_bucket();
    bucket.limit_money = Some(Money::new(i64::MIN, "USD", 0));
    bucket.used_money = Some(Money::new(i64::MAX, "USD", 0));
    assert_eq!(spend_remaining(&bucket), Some(Money::new(0, "USD", 0)));

    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 0));
    bucket.used_money = Some(Money::new(-1, "USD", 0));
    assert_eq!(spend_remaining(&bucket), None);
}
