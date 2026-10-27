// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::control::Money;

pub(super) fn window_group(group_id: &str, rank: u32) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: group_id.into(),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: "Weekly".into(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: Some(1_800_000_000),
        fetched_at_epoch: 1_800_000_001,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: Some(RawPercentPoints::new(57.0).unwrap()),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            used_percent: None,
            used_percent_origin: None,
            meter_percent: Some(MeterPercentPoints::new(57.0).unwrap()),
            meter_side: Some(UsageMeterSide::Remaining),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
            unit: None,
        },
        reset_at_epoch: Some(1_800_100_000),
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

mod case_01 {
    use super::*;

    #[test]
    fn metric_group_validate_accepts_same_currency_independent_scales() {
        let mut group = window_group("g-mixed-scale", 0);
        group.kind = UsageMetricGroupKindV1::SpendCap;
        group.value = UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
            spent: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
            remaining: Some(Money::new(9_000_000_000_000_000_001, "USD", 2)),
        };
        group
            .validate(0)
            .expect("independent Money scales validate");

        let bytes = serde_json::to_vec(&group).expect("group serializes");
        assert_eq!(
            serde_json::from_slice::<UsageMetricGroupV1>(&bytes).expect("group deserializes"),
            group
        );
    }
    #[test]
    fn metric_group_validate_rejects_each_mixed_currency_spend_pair() {
        for value in [
            UsageMetricValueV1::SpendCap {
                cap: Some(Money::new(100, "USD", 2)),
                spent: Some(Money::new(50, "SGD", 0)),
                remaining: None,
            },
            UsageMetricValueV1::SpendCap {
                cap: Some(Money::new(100, "USD", 2)),
                spent: None,
                remaining: Some(Money::new(50, "SGD", 0)),
            },
            UsageMetricValueV1::SpendCap {
                cap: None,
                spent: Some(Money::new(100, "USD", 2)),
                remaining: Some(Money::new(50, "SGD", 0)),
            },
        ] {
            let mut group = window_group("g-currency-mismatch", 0);
            group.kind = UsageMetricGroupKindV1::SpendCap;
            group.value = value;
            group.validate(0).unwrap_err();
        }
    }
    #[test]
    fn metric_group_validate_rejects_mismatched_kind_value_and_money() {
        let mut group = window_group("g", 0);
        group.kind = UsageMetricGroupKindV1::Balance;
        group.validate(0).unwrap_err();
        group.kind = UsageMetricGroupKindV1::Window;

        group.value = UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(1_00, "USD", 2)),
            spent: Some(Money::new(50, "SGD", 2)),
            remaining: None,
        };
        group.kind = UsageMetricGroupKindV1::SpendCap;
        group.validate(0).unwrap_err();

        group.value = UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(1_00, "USD", 2)),
            spent: Some(Money::new(50, "USD", 3)),
            remaining: None,
        };
        group
            .validate(0)
            .expect("same-currency Money values may use independent scales");
    }
}
