// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) const TEST_NOW_EPOCH: i64 = 1_800_000_000;

mod case_03 {
    use super::*;

    #[test]
    fn spend_cap_summary_preserves_exact_money_digits() {
        use jackin_protocol::control::Money;
        use jackin_protocol::usage_broker::{
            UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1,
            UsageQuotaStateV1,
        };

        let group = UsageMetricGroup {
            group_id: "spend".to_owned(),
            rank: 0,
            kind: UsageMetricGroupKindV1::SpendCap,
            label: "Spend cap".to_owned(),
            scope: UsageMetricScopeV1::default(),
            observed_at_epoch: None,
            fetched_at_epoch: TEST_NOW_EPOCH,
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

        assert_eq!(
            metric_group_value_summary(&group).as_deref(),
            Some(
                "cap $0.100000000000000001 · spent $0.100000000000000000 · remaining $0.000000000000000001"
            )
        );
    }
    #[test]
    fn spend_cap_summary_preserves_mixed_money_scales() {
        use jackin_protocol::control::Money;
        use jackin_protocol::usage_broker::{
            UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1,
            UsageQuotaStateV1,
        };

        let group = UsageMetricGroup {
            group_id: "spend-mixed-scale".to_owned(),
            rank: 0,
            kind: UsageMetricGroupKindV1::SpendCap,
            label: "Spend cap".to_owned(),
            scope: UsageMetricScopeV1::default(),
            observed_at_epoch: None,
            fetched_at_epoch: TEST_NOW_EPOCH,
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

        assert_eq!(
            metric_group_value_summary(&group).as_deref(),
            Some(
                "cap $100000000000000000 · spent $9999999999999999.99 · remaining $90000000000000000.01"
            )
        );
    }
}
