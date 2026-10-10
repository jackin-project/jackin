// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
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
        remaining_percent: Some(72),
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
        completed_at_epoch: Some(epoch),
        rate_limit_deadline_epoch: None,
        retry_deadline_epoch: None,
        success_deadline_epoch: Some(epoch + 300),
        consecutive_failures: 0,
    }
}

pub(super) fn empty_projection() -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
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
