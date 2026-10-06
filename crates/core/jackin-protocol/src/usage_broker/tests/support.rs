// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "opaque-account".into(),
        surface_id: "claude".into(),
    }
}

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
            remaining_percent: Some(UsagePercent::new(57).unwrap()),
            remaining_raw_percent: Some(57),
            used_percent: None,
            used_raw_percent: None,
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
