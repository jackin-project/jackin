// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Incremental per-account publication of the canonical broker projection.
//!
//! The broker publishes one immutable [`UsageProjectionV1`] per change. Each
//! account merges independently as its generation completes: one stalled
//! account never blocks healthy accounts, and a stalled account keeps its
//! loading/refreshing state instead of receiving fabricated data.
//!
//! Publication rules:
//!
//! - The catalog revision (`discovery_revision`) is fixed for the broker
//!   process lifetime; every publication carries the same revision while
//!   `broker_generation` increases monotonically.
//! - Only capabilities observed on broker traffic are merged. Unknown or
//!   never-requested accounts are never fabricated, and per-account published
//!   generations only move forward, so older generations can never regress a
//!   publication.
//! - A publication that fails [`UsageProjectionV1::validate`] is discarded and
//!   the last-good publication is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV1, UsageCoordinationErrorKind, UsageFreshnessPhaseV1,
    UsageFreshnessV1, UsageGenerationView, UsageIdentityKindV1, UsageIssueRecoverabilityV1,
    UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1,
    UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionV1, UsageProviderV1,
    UsageQuotaStateV1, UsageRefreshPhase, UsageWindowCategoryV1,
};

use crate::coordinator::{FileProjectionStateStore, ProjectionStateEnvelope, UsageCoordinator};

use super::super::projection::metric_groups_for_view;

/// Server-side incremental publisher. Cheap to clone; all state is shared.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionPublisher {
    coordinator: Arc<UsageCoordinator>,
    projection: Arc<Mutex<UsageProjectionV1>>,
    store: FileProjectionStateStore,
    known: Arc<Mutex<BTreeSet<UsageAccountCapability>>>,
    published: Arc<Mutex<BTreeMap<UsageAccountCapability, PublishedAccount>>>,
    identity_metadata: BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PublishedAccount {
    generation: u64,
    phase: UsageRefreshPhase,
    has_snapshot: bool,
}

/// Canonical identity evidence captured by host discovery and carried into
/// the Capsule-facing projection. A display label is not identity evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AccountIdentityMetadata {
    pub identity_kind: UsageIdentityKindV1,
    pub provenance_count: u32,
}

impl ProjectionPublisher {
    /// Attach a publisher to one broker-owned coordinator and projection.
    pub(crate) fn new(
        coordinator: Arc<UsageCoordinator>,
        projection: Arc<Mutex<UsageProjectionV1>>,
        store: FileProjectionStateStore,
    ) -> Self {
        Self {
            coordinator,
            projection,
            store,
            known: Arc::new(Mutex::new(BTreeSet::new())),
            published: Arc::new(Mutex::new(BTreeMap::new())),
            identity_metadata: BTreeMap::new(),
        }
    }

    /// Attach the immutable host-discovery identity evidence used by the
    /// Capsule/FFI publication. Missing entries remain conservative fallback
    /// rows for synthetic broker seams only.
    pub(crate) fn with_identity_metadata(
        mut self,
        identity_metadata: BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
    ) -> Self {
        self.identity_metadata = identity_metadata;
        self
    }

    /// Record one capability served by the broker. Only observed capabilities
    /// are ever merged into a publication.
    pub(crate) fn observe(&self, capability: &UsageAccountCapability) {
        if let Ok(mut known) = self.known.lock() {
            known.insert(capability.clone());
        }
    }

    /// Capabilities observed so far, in settled order.
    pub(crate) fn known_capabilities(&self) -> Vec<UsageAccountCapability> {
        self.known
            .lock()
            .map(|known| known.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Merge every observed account's latest state and publish when anything
    /// advanced. Returns whether a new publication was stored.
    ///
    /// Each account is read independently: one unreadable account is skipped
    /// without affecting the others.
    pub(crate) fn publish_due(&self, now_epoch: i64) -> bool {
        let capabilities = self.known_capabilities();
        if capabilities.is_empty() {
            return false;
        }
        let mut views = Vec::with_capacity(capabilities.len());
        for capability in &capabilities {
            if let Ok(view) = self.coordinator.current(capability, now_epoch) {
                views.push(view);
            }
        }
        if views.is_empty() {
            return false;
        }
        let Ok(mut published) = self.published.lock() else {
            return false;
        };
        let mut pending = Vec::new();
        let mut advanced = false;
        for view in &views {
            let current = PublishedAccount {
                generation: view.generation,
                phase: view.phase,
                has_snapshot: view.snapshot.is_some(),
            };
            // Older generations and resurrected activity never regress a
            // publication; unchanged accounts publish nothing new.
            let stale = published.get(&view.capability).is_some_and(|previous| {
                *previous == current
                    || previous.generation > current.generation
                    || (previous.generation == current.generation
                        && previous.phase.is_terminal()
                        && current.phase.is_active())
            });
            if !stale {
                pending.push((view.capability.clone(), current));
                advanced = true;
            }
        }
        if !advanced {
            return false;
        }
        let Ok(mut projection) = self.projection.lock() else {
            return false;
        };
        let mut next = projection.clone();
        merge_views(&mut next, &views, &self.identity_metadata);
        next.broker_generation = next.broker_generation.saturating_add(1);
        next.projection_id = format!("{}:{}", next.broker_instance_id, next.broker_generation);
        next.generated_at_epoch = now_epoch;
        if next.validate().is_err() {
            return false;
        }
        let envelope = ProjectionStateEnvelope {
            schema_version: 1,
            catalog_revision: next.discovery_revision.clone(),
            broker_instance_id: next.broker_instance_id.clone(),
            projection: next.clone(),
            aliases: Vec::new(),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        };
        if self.store.store(&envelope).is_err() {
            return false;
        }
        *projection = next;
        for (capability, current) in pending {
            published.insert(capability, current);
        }
        true
    }
}

/// Rebuild provider/account rows from per-account generation views.
///
/// Providers and accounts are rebuilt in settled `(surface_id, account_id)`
/// order with canonical ranks. Projection-level `unresolved`, `issues`, and
/// the catalog revision are preserved untouched.
fn merge_views(
    projection: &mut UsageProjectionV1,
    views: &[UsageGenerationView],
    identity_metadata: &BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
) {
    let mut ordered = views.to_vec();
    ordered.sort_by(|left, right| {
        (&left.capability.surface_id, &left.capability.account_id)
            .cmp(&(&right.capability.surface_id, &right.capability.account_id))
    });
    let any_active = ordered.iter().any(|view| view.phase.is_active());
    projection.refresh_state = if any_active {
        UsageProjectionRefreshStateV1::Refreshing
    } else {
        UsageProjectionRefreshStateV1::Idle
    };
    let mut providers: Vec<UsageProviderV1> = Vec::new();
    for view in &ordered {
        let surface_id = view.capability.surface_id.clone();
        if providers
            .last()
            .is_none_or(|provider: &UsageProviderV1| provider.provider_id != surface_id)
        {
            providers.push(UsageProviderV1 {
                provider_id: surface_id.clone(),
                display_name: surface_id.clone(),
                rank: u32::try_from(providers.len()).unwrap_or(u32::MAX),
                membership_state: UsageMembershipStateV1::Current,
                freshness: UsageFreshnessV1 {
                    generation: 0,
                    phase: UsageFreshnessPhaseV1::Failed,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                accounts: Vec::new(),
                issues: Vec::new(),
            });
        }
        let Some(provider) = providers.last_mut() else {
            continue;
        };
        let account = account_for_view(
            view,
            provider.accounts.len(),
            identity_metadata.get(&view.capability),
        );
        if let Some(snapshot) = &view.snapshot
            && !snapshot.account.provider_label.is_empty()
        {
            provider.display_name = snapshot.account.provider_label.clone();
        }
        provider.accounts.push(account);
    }
    for provider in &mut providers {
        let provider_active = ordered
            .iter()
            .filter(|view| view.capability.surface_id == provider.provider_id)
            .any(|view| view.phase.is_active());
        provider.freshness = aggregate_freshness(provider_active, &provider.accounts);
    }
    projection.providers = providers;
}

fn aggregate_freshness(any_active: bool, accounts: &[UsageAccountV1]) -> UsageFreshnessV1 {
    let mut freshness = UsageFreshnessV1 {
        generation: 0,
        phase: UsageFreshnessPhaseV1::Failed,
        last_good_at_epoch: None,
        retry_at_epoch: None,
        is_stale: false,
    };
    for account in accounts {
        freshness.generation = freshness.generation.max(account.freshness.generation);
        freshness.last_good_at_epoch = freshness
            .last_good_at_epoch
            .max(account.freshness.last_good_at_epoch);
        freshness.retry_at_epoch = [freshness.retry_at_epoch, account.freshness.retry_at_epoch]
            .into_iter()
            .flatten()
            .min();
        freshness.is_stale |= account.freshness.is_stale;
    }
    freshness.phase = if any_active {
        UsageFreshnessPhaseV1::Refreshing
    } else if accounts
        .iter()
        .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Failed)
    {
        UsageFreshnessPhaseV1::Failed
    } else if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV1::Stale)
    {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    freshness
}

fn account_for_view(
    view: &UsageGenerationView,
    rank: usize,
    identity_metadata: Option<&AccountIdentityMetadata>,
) -> UsageAccountV1 {
    let snapshot = view.snapshot.clone();
    let header_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.account_label.clone())
        .unwrap_or_default();
    let provider_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.provider_label.clone())
        .unwrap_or_default();
    let display_label = if header_label.trim().is_empty() {
        if provider_label.trim().is_empty() {
            view.capability.surface_id.clone()
        } else {
            provider_label.clone()
        }
    } else {
        header_label.clone()
    };
    let status = snapshot.as_ref().map(|snapshot| snapshot.status);
    let lifecycle = match status {
        Some(UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret) => {
            UsageLifecycleV1::NeedsSecret
        }
        Some(UsageSnapshotStatus::Unsupported) => UsageLifecycleV1::Unsupported,
        Some(UsageSnapshotStatus::Unavailable) => UsageLifecycleV1::Unavailable,
        Some(UsageSnapshotStatus::Error) | None if view.error.is_some() => UsageLifecycleV1::Error,
        _ => UsageLifecycleV1::Available,
    };
    let is_stale = snapshot.as_ref().is_some_and(|snapshot| {
        matches!(snapshot.status, UsageSnapshotStatus::Stale)
            || view.phase == UsageRefreshPhase::Failed
    });
    let phase = if view.phase.is_active() {
        UsageFreshnessPhaseV1::Refreshing
    } else if view.snapshot.is_none() {
        UsageFreshnessPhaseV1::Failed
    } else if is_stale {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    let windows = snapshot
        .as_ref()
        .map(|snapshot| windows_for_snapshot(&view.capability.account_id, snapshot))
        .unwrap_or_default();
    let metric_groups = snapshot
        .as_ref()
        .and_then(|snapshot| {
            metric_groups_for_view(
                &view.capability.account_id,
                snapshot,
                snapshot.account.plan_label.as_deref(),
            )
            .ok()
        })
        .unwrap_or_default();
    let issues = view
        .error
        .as_ref()
        .map(|error| {
            vec![UsageIssueV1 {
                code: issue_code(error.kind),
                scope: UsageIssueScopeV1::Account,
                recoverability: issue_recoverability(error.kind),
                message: error.message.clone(),
                retry_at_epoch: view.retry_at_epoch,
            }]
        })
        .unwrap_or_default();
    let fallback_identity_kind = if header_label.trim().is_empty() {
        UsageIdentityKindV1::ProviderAccountId
    } else {
        UsageIdentityKindV1::ProviderStableHandle
    };
    UsageAccountV1 {
        canonical_account_id: view.capability.account_id.clone(),
        identity_kind: identity_metadata
            .map_or(fallback_identity_kind, |metadata| metadata.identity_kind),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        display_label,
        plan_label: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.plan_label.clone()),
        status_label: None,
        lifecycle,
        freshness: UsageFreshnessV1 {
            generation: view.generation,
            phase,
            last_good_at_epoch: snapshot.as_ref().map(|snapshot| snapshot.fetched_at_epoch),
            retry_at_epoch: view.retry_at_epoch,
            is_stale,
        },
        provenance_count: identity_metadata.map_or(1, |metadata| metadata.provenance_count),
        windows,
        metric_groups,
        credential_expires_at_epoch: None,
        issues,
    }
}

fn windows_for_snapshot(account_id: &str, snapshot: &FocusedUsageView) -> Vec<UsageLimitWindowV1> {
    snapshot
        .buckets
        .iter()
        .enumerate()
        .map(|(rank, bucket)| window_for_bucket(account_id, rank, bucket))
        .collect()
}

fn window_for_bucket(
    account_id: &str,
    rank: usize,
    bucket: &QuotaBucketView,
) -> UsageLimitWindowV1 {
    let category = match bucket.status_slot {
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    };
    let raw_used = money_used_raw_percent(bucket);
    let overage = raw_used.is_some_and(|value| value > 100);
    let (remaining_percent, remaining_raw_percent) = if overage {
        (None, None)
    } else {
        bucket.remaining_percent.map_or((None, None), |percent| {
            let clamped = UsagePercent::clamp_raw(i32::from(percent));
            (Some(clamped), Some(i32::from(percent)))
        })
    };
    let (used_percent, used_raw_percent) = if overage || remaining_percent.is_none() {
        raw_used.map_or((None, None), |raw| {
            (Some(UsagePercent::clamp_raw(raw)), Some(raw))
        })
    } else {
        (None, None)
    };
    let value_label = if overage {
        raw_used.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |raw| format!("{raw}% used"),
        )
    } else {
        match (&bucket.used_label, &bucket.limit_label) {
            (Some(used), Some(limit)) => format!("{used} of {limit}"),
            (Some(used), None) => used.clone(),
            (None, Some(limit)) => limit.clone(),
            (None, None) => bucket
                .remaining_percent
                .map_or_else(String::new, |percent| format!("{percent}% left")),
        }
    };
    UsageLimitWindowV1 {
        window_id: format!("{account_id}:{rank}"),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        category,
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        reset_at_epoch: bucket.resets_at,
        quota_state: quota_state_for_bucket(bucket),
        pace_label: bucket.pace_label.clone(),
        runs_out_label: None,
    }
}

/// Raw used percentage for money-backed quota windows. The broker publisher
/// must preserve overage just like the desktop projection; only bar geometry
/// is clamped.
fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return None;
    }
    let scaled = used.amount_minor.saturating_mul(100);
    let raw = scaled.checked_div(limit.amount_minor)?;
    Some(i32::try_from(raw).unwrap_or(if raw < 0 { i32::MIN } else { i32::MAX }))
}

fn quota_state_for_bucket(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV1::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV1::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV1::Available
                        } else {
                            UsageQuotaStateV1::Unknown
                        }
                    }
                }
            }
        }
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
    }
}

fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
    match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
        (Some(used), Some(limit)) => {
            used.currency == limit.currency
                && used.exponent == limit.exponent
                && limit.amount_minor > 0
                && used.amount_minor >= limit.amount_minor
        }
        _ => false,
    }
}

fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.used_label.is_some()
        || bucket.limit_label.is_some()
}

fn issue_code(kind: UsageCoordinationErrorKind) -> String {
    match kind {
        UsageCoordinationErrorKind::Unavailable => "unavailable",
        UsageCoordinationErrorKind::Unauthorized => "unauthorized",
        UsageCoordinationErrorKind::OwnerLost => "owner_lost",
        UsageCoordinationErrorKind::WaitTimeout => "wait_timeout",
        UsageCoordinationErrorKind::CorruptState => "corrupt_state",
        UsageCoordinationErrorKind::ProviderTimeout => "provider_timeout",
        UsageCoordinationErrorKind::ProviderUnavailable => "provider_unavailable",
        UsageCoordinationErrorKind::NeedsSecret => "needs_secret",
        UsageCoordinationErrorKind::RateLimited => "rate_limited",
        UsageCoordinationErrorKind::ProtocolMismatch => "protocol_mismatch",
    }
    .to_owned()
}

const fn issue_recoverability(kind: UsageCoordinationErrorKind) -> UsageIssueRecoverabilityV1 {
    match kind {
        UsageCoordinationErrorKind::NeedsSecret | UsageCoordinationErrorKind::Unauthorized => {
            UsageIssueRecoverabilityV1::ActionRequired
        }
        UsageCoordinationErrorKind::ProtocolMismatch
        | UsageCoordinationErrorKind::CorruptState
        | UsageCoordinationErrorKind::OwnerLost => UsageIssueRecoverabilityV1::Terminal,
        _ => UsageIssueRecoverabilityV1::Retryable,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use jackin_protocol::control::{Money, UsageConfidence, UsageSeverity, UsageSource};
    use jackin_protocol::usage_broker::{
        UsageAccountCapability, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1,
        UsageMetricValueV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
        UsageQuotaStateV1,
    };

    use super::*;
    use crate::coordinator::{
        AccountStateEnvelope, AccountStateStore, ProviderProbeOutcome, StateStoreError,
        UsageCoordinatorConfig, UsageProviderExecutor,
    };

    #[derive(Default)]
    struct MemoryStore {
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

    struct ImmediateExecutor;

    impl UsageProviderExecutor for ImmediateExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            ProviderProbeOutcome::success(fresh_view())
        }
    }

    fn capability() -> UsageAccountCapability {
        UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        }
    }

    fn fresh_view() -> FocusedUsageView {
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

    fn empty_projection() -> UsageProjectionV1 {
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

    fn account_with_retry(id: &str, retry_at_epoch: Option<i64>) -> UsageAccountV1 {
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

    #[test]
    fn capsule_publication_preserves_identity_kind_and_provenance_per_account() {
        let first = UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let second = UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let views = vec![
            UsageGenerationView {
                capability: first.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
            UsageGenerationView {
                capability: second.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
        ];
        let metadata = BTreeMap::from([
            (
                first,
                AccountIdentityMetadata {
                    identity_kind: UsageIdentityKindV1::ProviderAccountId,
                    provenance_count: 3,
                },
            ),
            (
                second,
                AccountIdentityMetadata {
                    identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                    provenance_count: 2,
                },
            ),
        ]);
        let mut projection = empty_projection();

        merge_views(&mut projection, &views, &metadata);

        let accounts = &projection.providers[0].accounts;
        assert_eq!(accounts.len(), 2);
        assert_eq!(
            accounts[0].identity_kind,
            UsageIdentityKindV1::ProviderAccountId
        );
        assert_eq!(accounts[0].provenance_count, 3);
        assert_eq!(
            accounts[1].identity_kind,
            UsageIdentityKindV1::ProviderStableHandle
        );
        assert_eq!(accounts[1].provenance_count, 2);
    }

    #[test]
    fn capsule_publication_preserves_openrouter_overage_raw_used_percent() {
        let capability = capability();
        let mut view = fresh_view();
        view.buckets = vec![QuotaBucketView {
            label: "Account credits".to_owned(),
            used_label: Some("$120".to_owned()),
            limit_label: Some("$100".to_owned()),
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: Some(StatusSlot::Spend),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: Some(Money::new(12_000, "USD", 2)),
            limit_money: Some(Money::new(10_000, "USD", 2)),
            severity: UsageSeverity::Danger,
        }];
        let views = [UsageGenerationView {
            capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(view),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();

        merge_views(&mut projection, &views, &BTreeMap::new());

        let window = &projection.providers[0].accounts[0].windows[0];
        assert_eq!(window.value_label, "120% used");
        assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
        assert_eq!(window.used_raw_percent, Some(120));
        assert_eq!(window.remaining_percent, None);
        assert_eq!(window.remaining_raw_percent, None);
        assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
        match &projection.providers[0].accounts[0].metric_groups[1].value {
            UsageMetricValueV1::SpendCap {
                cap,
                spent,
                remaining,
            } => {
                assert_eq!(cap, &Some(Money::new(10_000, "USD", 2)));
                assert_eq!(spent, &Some(Money::new(12_000, "USD", 2)));
                assert_eq!(remaining, &Some(Money::new(0, "USD", 2)));
            }
            other => panic!("expected structured spend-cap value, got {other:?}"),
        }
        window.validate(0).unwrap();
    }

    #[test]
    fn publication_marks_empty_and_stale_quota_states_without_fabrication() {
        let capability = capability();
        let mut empty = fresh_view();
        empty.status = UsageSnapshotStatus::Fresh;
        empty.buckets = vec![QuotaBucketView {
            label: "Provider-defined".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }];
        let unknown_projection = {
            let views = [UsageGenerationView {
                capability: capability.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(empty),
                error: None,
                retry_at_epoch: None,
            }];
            let mut projection = empty_projection();
            merge_views(&mut projection, &views, &BTreeMap::new());
            projection
        };
        assert_eq!(
            unknown_projection.providers[0].accounts[0].windows[0].quota_state,
            UsageQuotaStateV1::Unknown
        );

        let mut stale = fresh_view();
        stale.status = UsageSnapshotStatus::Stale;
        stale.buckets[0].status = UsageSnapshotStatus::Stale;
        let views = [UsageGenerationView {
            capability,
            generation: 2,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(stale),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        merge_views(&mut projection, &views, &BTreeMap::new());
        let account = &projection.providers[0].accounts[0];
        assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Stale);
        assert!(account.freshness.is_stale);
        assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Available);
    }

    #[test]
    fn publication_refreshing_is_scoped_to_provider_surface() {
        let stalled = UsageAccountCapability {
            account_id: "stalled".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let healthy = UsageAccountCapability {
            account_id: "healthy".to_owned(),
            surface_id: "codex".to_owned(),
        };
        let views = [
            UsageGenerationView {
                capability: stalled,
                generation: 2,
                phase: UsageRefreshPhase::Updating,
                snapshot: None,
                error: None,
                retry_at_epoch: None,
            },
            UsageGenerationView {
                capability: healthy,
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
        ];
        let mut projection = empty_projection();
        merge_views(&mut projection, &views, &BTreeMap::new());

        assert_eq!(
            projection.refresh_state,
            UsageProjectionRefreshStateV1::Refreshing
        );
        assert_eq!(
            projection
                .providers
                .iter()
                .find(|provider| provider.provider_id == "claude")
                .map(|provider| provider.freshness.phase),
            Some(UsageFreshnessPhaseV1::Refreshing)
        );
        assert_eq!(
            projection
                .providers
                .iter()
                .find(|provider| provider.provider_id == "codex")
                .map(|provider| provider.freshness.phase),
            Some(UsageFreshnessPhaseV1::Current)
        );
    }

    #[test]
    fn retry_deadline_aggregation_is_independent_of_account_order() {
        let early = account_with_retry("early", Some(100));
        let late = account_with_retry("late", Some(200));
        let first = aggregate_freshness(false, &[late.clone(), early.clone()]);
        let second = aggregate_freshness(false, &[early, late]);
        assert_eq!(first.retry_at_epoch, Some(100));
        assert_eq!(second.retry_at_epoch, Some(100));
    }

    #[test]
    fn publication_checkpoint_advances_only_after_durable_store() {
        let temp = tempfile::tempdir().unwrap();
        let account = capability();
        let coordinator = Arc::new(UsageCoordinator::new(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
        ));
        let queued = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        coordinator
            .join_generation(&account, queued.generation, Duration::from_secs(2), 1_001)
            .unwrap();

        let projection = Arc::new(Mutex::new(empty_projection()));
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            FileProjectionStateStore::under_data_dir(temp.path()),
        );
        publisher.observe(&account);

        let broker_dir = temp.path().join("usage-broker");
        fs::create_dir_all(&broker_dir).unwrap();
        fs::create_dir(broker_dir.join("projection.json")).unwrap();
        assert!(!publisher.publish_due(1_002));
        assert_eq!(projection.lock().unwrap().broker_generation, 0);

        fs::remove_dir(broker_dir.join("projection.json")).unwrap();
        assert!(publisher.publish_due(1_003));
        assert_eq!(projection.lock().unwrap().broker_generation, 1);
        assert!(!publisher.publish_due(1_004));
    }
}
