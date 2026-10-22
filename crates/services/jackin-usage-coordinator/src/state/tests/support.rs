// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{CanonicalRemainingPercent, PercentObservationOrigin};

pub(super) fn legacy_account_state(
    envelope: &AccountStateEnvelope,
    schema_version: u32,
) -> serde_json::Value {
    let mut value = serde_json::to_value(envelope).unwrap();
    value["schema_version"] = serde_json::json!(schema_version);
    for result_name in ["terminal_result", "last_good"] {
        if value[result_name].is_null() {
            continue;
        }
        for bucket in value[result_name]["buckets"].as_array_mut().unwrap() {
            let object = bucket.as_object_mut().unwrap();
            for key in [
                "used_percent",
                "used_percent_origin",
                "remaining_percent_origin",
                "count_quota",
                "provider_quota_state",
            ] {
                object.remove(key);
            }
            if let Some(number) = object
                .get("remaining_percent")
                .and_then(serde_json::Value::as_f64)
            {
                let exact_integer = i64::try_from(number as i128).unwrap();
                assert_eq!(number, exact_integer as f64);
                object.insert(
                    "remaining_percent".to_owned(),
                    serde_json::json!(exact_integer),
                );
            }
        }
    }
    value
}

pub(super) fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "account-123".into(),
        surface_id: "claude".into(),
    }
}

pub(super) fn quota_view(epoch: i64, label: &str) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", epoch);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".into();
    view.account.account_label = label.into();
    view.buckets = vec![QuotaBucketView {
        label: "Session".into(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(CanonicalRemainingPercent::new(72.0).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
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
    }];
    view.last_error = None;
    view
}

pub(super) fn completed(epoch: i64, label: &str) -> AccountStateEnvelope {
    let view = quota_view(epoch, label);
    AccountStateEnvelope {
        schema_version: ACCOUNT_STATE_SCHEMA_VERSION,
        capability: capability(),
        generation: 1,
        phase: UsageRefreshPhase::Completed,
        terminal_result: Some(view.clone()),
        last_good: Some(view),
        terminal_error: None,
        started_at_epoch: Some(epoch),
        provider_invoked_at_epoch: Some(epoch),
        reload_fence_required: false,
        completed_at_epoch: Some(epoch),
        rate_limit_deadline_epoch: None,
        retry_deadline_epoch: None,
        success_deadline_epoch: Some(epoch + 300),
        consecutive_failures: 0,
    }
}

pub(super) fn empty_projection() -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "projection-1".into(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog-1".into(),
        broker_instance_id: "instance-1".into(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}
