// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::CanonicalRemainingPercent;
use jackin_protocol::control::CountQuota;
use jackin_protocol::control::CountQuotaPeriod;
use jackin_protocol::control::CountQuotaProvenance;
use jackin_protocol::control::CountQuotaUnit;
use jackin_protocol::control::Money;
use jackin_protocol::control::PercentObservationOrigin;
use jackin_protocol::control::QuotaBucketView;
use jackin_protocol::control::UsageSeverity;
use jackin_protocol::control::UsageSnapshotStatus;

fn bucket(
    remaining: Option<u64>,
    severity: UsageSeverity,
    status: UsageSnapshotStatus,
    unit: CountQuotaUnit,
) -> QuotaBucketView {
    QuotaBucketView {
        label: "Requests".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(
            CanonicalRemainingPercent::new(0.0).expect("valid raw remaining percent"),
        ),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: Some(CountQuota::new(
            Some(9),
            Some(10),
            remaining,
            unit,
            if unit == CountQuotaUnit::Requests {
                CountQuotaPeriod::UtcDaily
            } else {
                CountQuotaPeriod::Unknown
            },
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status,
        used_money: None,
        limit_money: None,
        severity,
    }
}

#[test]
fn money_exhaustion_compares_each_amount_scale_exactly() {
    let mut view = bucket(
        None,
        UsageSeverity::Normal,
        UsageSnapshotStatus::Fresh,
        CountQuotaUnit::Requests,
    );
    view.used_money = Some(Money::new(100, "USD", 2));
    view.limit_money = Some(Money::new(1, "USD", 0));
    assert!(money_is_exhausted(&view));

    view.used_money = Some(Money::new(999_999_999_999_999_999, "USD", 2));
    view.limit_money = Some(Money::new(100_000_000_000_000_000, "USD", 0));
    assert!(!money_is_exhausted(&view));

    view.limit_money = Some(Money::new(0, "USD", 0));
    assert!(!money_is_exhausted(&view));
    view.limit_money = Some(Money::new(1, "SGD", 0));
    assert!(!money_is_exhausted(&view));
}
