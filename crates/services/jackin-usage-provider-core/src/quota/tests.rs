// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::Money;
use jackin_protocol::control::QuotaBucketView;
use jackin_protocol::control::RawPercentPoints;
use jackin_protocol::control::UsageSeverity;
use jackin_protocol::control::UsageSnapshotStatus;

fn bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Requests".to_owned(),
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
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn money_percent_uses_each_amount_scale() {
    let mut bucket = bucket();
    bucket.used_money = Some(Money::new(150, "USD", 2));
    bucket.limit_money = Some(Money::new(1, "USD", 0));

    assert_eq!(
        money_used_percent(&bucket).map(RawPercentPoints::get),
        Some(150.0)
    );

    bucket.used_money = Some(Money::new(999_999_999_999_999_999, "USD", 2));
    bucket.limit_money = Some(Money::new(100_000_000_000_000_000, "USD", 0));
    let points = money_used_percent(&bucket)
        .map(RawPercentPoints::get)
        .expect("large mixed-scale ratio remains finite");
    assert!(points.is_finite());
    assert!((points - 10.0).abs() < 1e-12);

    bucket.used_money = Some(Money::new(1, "USD", 0));
    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 255));
    assert!(
        money_used_percent(&bucket)
            .map(RawPercentPoints::get)
            .is_some_and(f64::is_finite)
    );
    bucket.used_money = Some(Money::new(1, "USD", 255));
    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 0));
    assert!(
        money_used_percent(&bucket)
            .map(RawPercentPoints::get)
            .is_some_and(|points| points > 0.0)
    );
}
