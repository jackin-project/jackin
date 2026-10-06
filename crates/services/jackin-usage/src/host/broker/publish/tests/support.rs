// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Default)]
pub(super) struct MemoryStore {
    states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
}

impl AccountStateStore for MemoryStore {
    fn load(
        &self,
        capability: &UsageAccountCapability,
        _now_epoch: i64,
    ) -> Result<Option<AccountStateEnvelope>, StateStoreError> {
        Ok(self.states.lock().unwrap().get(capability).cloned())
    }

    fn store(
        &self,
        envelope: &AccountStateEnvelope,
        _now_epoch: i64,
    ) -> Result<(), StateStoreError> {
        self.states
            .lock()
            .unwrap()
            .insert(envelope.capability.clone(), envelope.clone());
        Ok(())
    }
}

pub(super) struct ImmediateExecutor;

impl UsageProviderExecutor for ImmediateExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        ProviderProbeOutcome::success(fresh_view())
    }
}

pub(super) struct FailingCatalogExecutor {
    pub(super) reconciles: AtomicUsize,
}

impl UsageProviderExecutor for FailingCatalogExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        ProviderProbeOutcome::success(fresh_view())
    }

    fn reconcile_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.reconciles.fetch_add(1, Ordering::SeqCst);
        Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "fixture catalog reconciliation failed".to_owned(),
        })
    }
}

pub(super) fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "account-a".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

pub(super) fn fresh_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", 1_000);
    view.focused_agent = Some("claude".to_owned());
    view.focused_provider = Some("Claude".to_owned());
    view.account.provider_label = "Anthropic".to_owned();
    view.account.account_label = "account@example.test".to_owned();
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(75),
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

pub(super) fn empty_projection() -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "test:0".to_owned(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog".to_owned(),
        broker_instance_id: "test".to_owned(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

pub(super) fn account_with_retry(id: &str, retry_at_epoch: Option<i64>) -> UsageAccountV1 {
    UsageAccountV1 {
        canonical_account_id: id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: id.to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: UsageFreshnessV1 {
            generation: 1,
            phase: UsageFreshnessPhaseV1::Failed,
            last_good_at_epoch: None,
            retry_at_epoch,
            is_stale: false,
        },
        provenance_count: 1,
        windows: Vec::new(),
        metric_groups: Vec::new(),
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    }
}
