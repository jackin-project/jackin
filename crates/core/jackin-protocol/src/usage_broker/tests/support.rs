// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const CURRENT_PROJECTION_FIXTURE: &str = include_str!(
    "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
);
const HISTORICAL_V1_PROJECTION_FIXTURE: &str = include_str!(
    "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
);

pub(super) fn projection_fixture_value() -> serde_json::Value {
    serde_json::from_str(CURRENT_PROJECTION_FIXTURE).expect("current V2 projection fixture is JSON")
}

pub(super) fn historical_projection_fixture_value() -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_str(HISTORICAL_V1_PROJECTION_FIXTURE)
        .expect("historical V1 projection fixture is JSON");

    if value["schema_version"] == 1 {
        value["schema_version"] = serde_json::json!(2);
        for window in value["providers"]
            .as_array_mut()
            .expect("historical providers are an array")
            .iter_mut()
            .flat_map(|provider| provider["accounts"].as_array_mut().into_iter().flatten())
            .flat_map(|account| account["windows"].as_array_mut().into_iter().flatten())
        {
            if let Some(object) = window.as_object_mut() {
                let old_geometry = object.remove("remaining_percent");
                if let Some(old_geometry) = old_geometry.filter(|value| !value.is_null()) {
                    let points = old_geometry
                        .as_f64()
                        .expect("historical integer geometry converts to percentage points");
                    object.insert("meter_percent".to_owned(), serde_json::json!(points));
                    object.insert("meter_side".to_owned(), serde_json::json!("remaining"));
                }
            }
        }
    }

    value
}

pub(super) fn projection_fixture() -> UsageProjectionV1 {
    serde_json::from_value(projection_fixture_value())
        .expect("current V2 projection fixture decodes")
}

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
