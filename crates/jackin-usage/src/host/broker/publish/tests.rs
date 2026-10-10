// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::control::{Money, UsageConfidence, UsageSeverity, UsageSource};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageIssueRecoverabilityV1,
    UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1, UsageMetricValueV1,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageQuotaStateV1,
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

struct RankedAccountExecutor;

impl UsageProviderExecutor for RankedAccountExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        let mut snapshot = fresh_view();
        snapshot.account.provider_label = HostSurfaceId::from_id(&capability.surface_id)
            .map_or_else(
                || capability.surface_id.clone(),
                |surface| surface.label().to_owned(),
            );
        snapshot.account.account_label = match capability.account_id.as_str() {
            "account-z" => "Zulu".to_owned(),
            "account-umlaut" => "Änne".to_owned(),
            "account-b" => "same".to_owned(),
            "account-ana" => "Ana".to_owned(),
            "account-ring" => "Åke".to_owned(),
            "account-a" => "Same".to_owned(),
            _ => format!("{} account", capability.surface_id),
        };
        ProviderProbeOutcome::success(snapshot)
    }
}

struct FailingCatalogExecutor {
    reconciles: AtomicUsize,
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

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "account-a".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn catalog_reconciliation(
    catalog_revision: &str,
    entries: Vec<UsageCatalogEntry>,
    diagnostics: CatalogDiagnostics,
) -> CatalogReconciliation {
    CatalogReconciliation {
        catalog_revision: catalog_revision.to_owned(),
        entries,
        diagnostics,
        identity_metadata: None,
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

struct CatalogDiagnosticFixture {
    _temp: tempfile::TempDir,
    coordinator: Arc<UsageCoordinator>,
    projection: Arc<Mutex<UsageProjectionV1>>,
    store: FileProjectionStateStore,
    publisher: ProjectionPublisher,
    revoked_capability: UsageAccountCapability,
    current_capability: UsageAccountCapability,
    current_entry: UsageCatalogEntry,
}

impl CatalogDiagnosticFixture {
    fn reconcile_interaction(&self) -> UsageProjectionV1 {
        let mut diagnostics = CatalogDiagnostics::default();
        diagnostics.push_provider_issue(
            "claude",
            "Claude",
            CatalogDiagnosticCode::InteractionRequired,
        );
        diagnostics.push_unresolved("claude", "Claude", "opaque-candidate".to_owned(), 1);

        let before = self.publisher.current_projection().unwrap();
        self.publisher
            .reconcile_catalog_if_projection(
                Some(&before.projection_id),
                catalog_reconciliation(
                    "catalog-current",
                    vec![self.current_entry.clone()],
                    diagnostics,
                ),
                1_002,
            )
            .unwrap()
    }

    fn refresh_and_publish(
        &self,
        capability: &UsageAccountCapability,
        request_at: i64,
        joined_at: i64,
        published_at: i64,
    ) {
        let generation = self
            .coordinator
            .request_refresh(capability, 0, true, request_at)
            .unwrap()
            .generation;
        self.coordinator
            .join_generation(capability, generation, Duration::from_secs(1), joined_at)
            .unwrap();
        self.publisher.observe(capability);
        assert!(self.publisher.publish_due(published_at));
    }
}

fn seeded_catalog_diagnostic_fixture() -> CatalogDiagnosticFixture {
    let temp = tempfile::tempdir().unwrap();
    let revoked_capability = capability();
    let current_capability = UsageAccountCapability {
        account_id: "account-current".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let revoked_entry = UsageCatalogEntry {
        capability: revoked_capability.clone(),
        revision: "revision-old".to_owned(),
    };
    let current_entry = UsageCatalogEntry {
        capability: current_capability.clone(),
        revision: "revision-current".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [revoked_entry.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        store.clone(),
    )
    .with_catalog([revoked_entry]);
    let fixture = CatalogDiagnosticFixture {
        _temp: temp,
        coordinator,
        projection,
        store,
        publisher,
        revoked_capability,
        current_capability,
        current_entry,
    };
    fixture.refresh_and_publish(&fixture.revoked_capability, 1_000, 1_001, 1_001);
    fixture
}

#[test]
fn interaction_diagnostics_survive_new_catalog_incremental_publish_and_revocation() {
    let fixture = seeded_catalog_diagnostic_fixture();
    let reconciled = fixture.reconcile_interaction();
    let revoked = reconciled
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == fixture.revoked_capability.account_id)
        .expect("revoked account remains as a tombstone");
    assert_eq!(revoked.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(reconciled.unresolved.len(), 1);
    assert_eq!(
        reconciled.unresolved[0].state,
        UsageLifecycleV1::NeedsSecret
    );
    assert_eq!(reconciled.unresolved[0].capability_id, "opaque-candidate");

    fixture.refresh_and_publish(&fixture.current_capability, 1_003, 1_004, 1_005);
    let published = fixture.publisher.current_projection().unwrap();
    assert_interaction_issue(&published);
    assert!(
        published
            .providers
            .iter()
            .flat_map(|provider| provider.accounts.iter())
            .any(|account| account.canonical_account_id == fixture.revoked_capability.account_id)
    );
    assert_eq!(published.unresolved.len(), 1);
    assert_eq!(published.unresolved[0].state, UsageLifecycleV1::NeedsSecret);
    assert_eq!(fixture.store.load().unwrap().unwrap().projection, published);
}

#[test]
fn clean_catalog_scan_clears_catalog_diagnostics_and_keeps_unrelated_provider_issues() {
    let fixture = seeded_catalog_diagnostic_fixture();
    fixture.reconcile_interaction();
    fixture
        .projection
        .lock()
        .unwrap()
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "anthropic")
        .unwrap()
        .issues
        .push(UsageIssueV1 {
            code: "provider_specific_issue".to_owned(),
            scope: UsageIssueScopeV1::Provider,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "Rust-owned provider issue".to_owned(),
            retry_at_epoch: Some(1_006),
        });

    let before = fixture.publisher.current_projection().unwrap();
    let clean = fixture
        .publisher
        .reconcile_catalog_if_projection(
            Some(&before.projection_id),
            catalog_reconciliation("catalog-clean", Vec::new(), CatalogDiagnostics::default()),
            1_006,
        )
        .unwrap();
    let provider = clean
        .providers
        .iter()
        .find(|provider| provider.provider_id == "anthropic")
        .unwrap();
    assert!(
        provider
            .issues
            .iter()
            .any(|issue| issue.code == "provider_specific_issue")
    );
    assert!(
        !provider
            .issues
            .iter()
            .any(|issue| issue.code == "interaction_required")
    );
    assert!(clean.unresolved.is_empty());
    assert!(fixture.publisher.catalog_capabilities().is_empty());
}

fn assert_interaction_issue(projection: &UsageProjectionV1) {
    let provider = projection
        .providers
        .iter()
        .find(|provider| provider.provider_id == "anthropic")
        .unwrap();
    let issue = provider
        .issues
        .iter()
        .find(|issue| issue.code == "interaction_required")
        .expect("provider diagnostic survives incremental publication");
    assert_eq!(issue.scope, UsageIssueScopeV1::Provider);
    assert_eq!(
        issue.recoverability,
        UsageIssueRecoverabilityV1::ActionRequired
    );
    assert_eq!(issue.message, "Credential access requires interaction");
}

#[test]
fn catalog_helpers_read_only_the_attached_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let catalog_entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [catalog_entry.clone()],
    ));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::new(Mutex::new(empty_projection())),
        FileProjectionStateStore::under_data_dir(temp.path()),
    );

    assert!(!publisher.is_catalog_member(&account));
    assert!(publisher.catalog_capabilities().is_empty());

    let publisher = publisher.with_catalog([catalog_entry]);
    assert!(publisher.is_catalog_member(&account));
    assert_eq!(publisher.catalog_capabilities(), vec![account.clone()]);
    assert!(!publisher.is_catalog_member(&UsageAccountCapability {
        account_id: "other-account".to_owned(),
        surface_id: account.surface_id,
    }));
}

#[test]
fn broker_conflict_is_unavailable_and_keeps_its_issue_code() {
    let account = account_for_view(
        &UsageGenerationView {
            capability: capability(),
            generation: 1,
            phase: UsageRefreshPhase::Failed,
            snapshot: None,
            error: Some(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::BrokerConflict,
                message: "another broker owns the lease".to_owned(),
            }),
            retry_at_epoch: None,
        },
        0,
        None,
    );

    assert_eq!(account.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(account.issues[0].code, "broker_conflict");
    assert_eq!(
        account.issues[0].recoverability,
        UsageIssueRecoverabilityV1::Terminal
    );
}

#[test]
fn catalog_reconciliation_publishes_removed_tombstone_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let catalog_entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [catalog_entry.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        store.clone(),
    )
    .with_catalog([catalog_entry.clone()]);

    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        coordinator
            .join_generation(&account, queued.generation, Duration::from_secs(1), 1_001)
            .unwrap()
            .phase,
        UsageRefreshPhase::Completed
    );
    publisher.observe(&account);
    assert!(publisher.publish_due(1_001));

    let removed = publisher
        .reconcile_catalog_if_projection(
            None,
            catalog_reconciliation("catalog-2", Vec::new(), CatalogDiagnostics::default()),
            1_002,
        )
        .unwrap();
    let account_row = &removed.providers[0].accounts[0];
    assert_eq!(account_row.canonical_account_id, account.account_id);
    assert_eq!(account_row.display_label, "account@example.test");
    assert_eq!(account_row.status_label.as_deref(), Some("removed"));
    assert_eq!(account_row.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(account_row.freshness.phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(removed.discovery_revision, "catalog-2");

    let persisted = store.load().unwrap().unwrap();
    assert!(persisted.catalog.is_empty());
    assert_eq!(persisted.projection, removed);
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

    merge_views(&mut projection, &views, &metadata).unwrap();

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
fn active_publisher_uses_canonical_provider_and_icu_account_ranks() {
    let temp = tempfile::tempdir().unwrap();
    let mut capabilities = Vec::new();
    for surface in HostSurfaceId::ALL.iter().rev().copied() {
        let account_ids = match surface {
            HostSurfaceId::Codex => [
                "account-z",
                "account-umlaut",
                "account-b",
                "account-ana",
                "account-ring",
                "account-a",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
            HostSurfaceId::Claude => vec!["claude-account".to_owned()],
            _ => vec![format!("{}-account", surface.id())],
        };
        capabilities.extend(
            account_ids
                .into_iter()
                .map(|account_id| UsageAccountCapability {
                    account_id,
                    surface_id: surface.id().to_owned(),
                }),
        );
    }
    let catalog = capabilities
        .iter()
        .enumerate()
        .map(|(index, capability)| UsageCatalogEntry {
            capability: capability.clone(),
            revision: format!("revision-{index}"),
        })
        .collect::<Vec<_>>();
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(RankedAccountExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        catalog.clone(),
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog(catalog);
    for (index, capability) in capabilities.iter().enumerate() {
        let request_at = 1_000 + i64::try_from(index).unwrap() * 2;
        let generation = coordinator
            .request_refresh(capability, 0, true, request_at)
            .unwrap()
            .generation;
        coordinator
            .join_generation(
                capability,
                generation,
                Duration::from_secs(1),
                request_at + 1,
            )
            .unwrap();
        publisher.observe(capability);
    }
    assert!(publisher.publish_due(2_000));
    let projection = publisher.current_projection().unwrap();

    assert_eq!(
        projection
            .providers
            .iter()
            .map(|provider| provider.provider_id.as_str())
            .collect::<Vec<_>>(),
        [
            "openai",
            "anthropic",
            "amp",
            "xai",
            "zai",
            "kimi",
            "minimax",
            "opencode",
            "google",
            "cursor",
            "meta",
            "openrouter",
        ]
    );
    assert_eq!(projection.providers[0].rank, 0);
    assert!(
        projection
            .providers
            .iter()
            .enumerate()
            .all(|(rank, provider)| provider.rank == u32::try_from(rank).unwrap())
    );
    assert_eq!(
        projection.providers[0]
            .accounts
            .iter()
            .map(|account| account.display_label.as_str())
            .collect::<Vec<_>>(),
        ["Åke", "Ana", "Änne", "Same", "same", "Zulu"]
    );
    assert_eq!(
        projection.providers[0]
            .accounts
            .iter()
            .map(|account| account.canonical_account_id.as_str())
            .collect::<Vec<_>>(),
        [
            "account-ring",
            "account-ana",
            "account-umlaut",
            "account-a",
            "account-b",
            "account-z",
        ]
    );
    assert!(
        projection.providers[0]
            .accounts
            .iter()
            .enumerate()
            .all(|(rank, account)| account.rank == u32::try_from(rank).unwrap())
    );
    assert!(
        projection.providers[0]
            .accounts
            .iter()
            .all(|account| account.identity_kind == UsageIdentityKindV1::UnverifiedHandle)
    );
    projection.validate().unwrap();
}

#[test]
fn no_snapshot_incremental_publish_preserves_provider_label_and_issue() {
    let capability = capability();
    let mut projection = empty_projection();
    let mut diagnostics = CatalogDiagnostics::default();
    diagnostics.push_provider_issue(
        "claude",
        "Anthropic",
        CatalogDiagnosticCode::InteractionRequired,
    );
    apply_catalog_diagnostics(&mut projection, &diagnostics).unwrap();

    merge_views(
        &mut projection,
        &[UsageGenerationView {
            capability,
            generation: 2,
            phase: UsageRefreshPhase::Updating,
            snapshot: None,
            error: None,
            retry_at_epoch: None,
        }],
        &BTreeMap::new(),
    )
    .unwrap();

    let provider = projection
        .providers
        .iter()
        .find(|provider| provider.provider_id == "anthropic")
        .expect("canonical provider row remains during refresh");
    assert_eq!(provider.display_name, "Anthropic");
    assert_eq!(provider.freshness.phase, UsageFreshnessPhaseV1::Refreshing);
    assert!(
        provider
            .issues
            .iter()
            .any(|issue| issue.code == "interaction_required")
    );
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

    merge_views(&mut projection, &views, &BTreeMap::new()).unwrap();

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
        merge_views(&mut projection, &views, &BTreeMap::new()).unwrap();
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
    merge_views(&mut projection, &views, &BTreeMap::new()).unwrap();
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
    merge_views(&mut projection, &views, &BTreeMap::new()).unwrap();

    assert_eq!(
        projection.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    assert_eq!(
        projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == "anthropic")
            .map(|provider| provider.freshness.phase),
        Some(UsageFreshnessPhaseV1::Refreshing)
    );
    assert_eq!(
        projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == "openai")
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

#[test]
fn catalog_publication_retains_removed_rows_without_expanding_to_new_members() {
    let temp = tempfile::tempdir().unwrap();
    let account_a = capability();
    let account_b = UsageAccountCapability {
        account_id: "account-b".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [
            UsageCatalogEntry {
                capability: account_a.clone(),
                revision: "revision-a".to_owned(),
            },
            UsageCatalogEntry {
                capability: account_b.clone(),
                revision: "revision-b".to_owned(),
            },
        ],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([
        UsageCatalogEntry {
            capability: account_a.clone(),
            revision: "revision-a".to_owned(),
        },
        UsageCatalogEntry {
            capability: account_b.clone(),
            revision: "revision-b".to_owned(),
        },
    ]);

    let queued = coordinator
        .request_refresh(&account_a, 0, true, 1_000)
        .unwrap();
    coordinator
        .join_generation(&account_a, queued.generation, Duration::from_secs(2), 1_001)
        .unwrap();
    publisher.observe(&account_a);
    assert!(publisher.publish_due(1_002));
    assert_eq!(projection.lock().unwrap().providers[0].accounts.len(), 1);

    let current = publisher
        .reconcile_catalog_if_projection(
            None,
            catalog_reconciliation(
                "catalog-2",
                vec![UsageCatalogEntry {
                    capability: account_b.clone(),
                    revision: "revision-b".to_owned(),
                }],
                CatalogDiagnostics::default(),
            ),
            1_003,
        )
        .unwrap();
    let removed = current
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == account_a.account_id)
        .expect("removed account remains visible as a tombstone");
    assert_eq!(removed.status_label.as_deref(), Some("removed"));
    assert_eq!(removed.lifecycle, UsageLifecycleV1::Unavailable);
    assert!(publisher.known_capabilities().is_empty());
    let persisted = FileProjectionStateStore::under_data_dir(temp.path())
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(persisted.catalog.len(), 1);
    assert_eq!(persisted.catalog[0].capability, account_b);

    let reintroduced = publisher
        .reconcile_catalog_if_projection(
            None,
            catalog_reconciliation(
                "catalog-3",
                vec![UsageCatalogEntry {
                    capability: account_a,
                    revision: "revision-a".to_owned(),
                }],
                CatalogDiagnostics::default(),
            ),
            1_004,
        )
        .unwrap();
    assert!(reintroduced.providers.is_empty());
}

#[test]
fn same_capability_revision_purges_stale_published_quota() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [old.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([old]);

    let generation = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    coordinator
        .join_generation(&account, generation, Duration::from_secs(1), 1_001)
        .unwrap();
    publisher.observe(&account);
    assert!(publisher.publish_due(1_001));

    let current = publisher
        .reconcile_catalog_if_projection(
            None,
            catalog_reconciliation(
                "catalog",
                vec![UsageCatalogEntry {
                    capability: account.clone(),
                    revision: "credential-b".to_owned(),
                }],
                CatalogDiagnostics::default(),
            ),
            1_002,
        )
        .unwrap();
    let row = &current.providers[0].accounts[0];
    assert_eq!(row.status_label.as_deref(), Some("removed"));
    assert!(row.windows.is_empty());
    assert!(row.metric_groups.is_empty());
    let reset = coordinator.current(&account, 1_002).unwrap();
    assert_eq!(reset.phase, UsageRefreshPhase::Idle);
    assert!(reset.snapshot.is_none());
}

#[test]
fn accepted_discovery_metadata_updates_rows_and_bootstrap_none_preserves_it() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [entry.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([entry.clone()])
    .with_identity_metadata(BTreeMap::from([(
        account.clone(),
        AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::LocalSourceHandle,
            provenance_count: 1,
        },
    )]));

    let generation = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    coordinator
        .join_generation(&account, generation, Duration::from_secs(1), 1_001)
        .unwrap();
    publisher.observe(&account);
    assert!(publisher.publish_due(1_001));

    let discovered_identity = AccountIdentityMetadata {
        identity_kind: UsageIdentityKindV1::ProviderStableHandle,
        provenance_count: 2,
    };
    let updated = publisher
        .reconcile_catalog_if_projection(
            None,
            CatalogReconciliation {
                catalog_revision: "discovery-accepted".to_owned(),
                entries: vec![entry.clone()],
                diagnostics: CatalogDiagnostics::default(),
                identity_metadata: Some(BTreeMap::from([(account.clone(), discovered_identity)])),
            },
            1_002,
        )
        .unwrap();
    let account_row = &updated.providers[0].accounts[0];
    assert_eq!(account_row.identity_kind, discovered_identity.identity_kind);
    assert_eq!(
        account_row.provenance_count,
        discovered_identity.provenance_count
    );

    let preserved = publisher
        .reconcile_catalog_if_projection(
            Some(&updated.projection_id),
            catalog_reconciliation(
                "foreground-bootstrap",
                vec![entry],
                CatalogDiagnostics::default(),
            ),
            1_003,
        )
        .unwrap();
    assert_eq!(
        preserved.providers[0].accounts[0].identity_kind,
        discovered_identity.identity_kind
    );
    assert_eq!(
        preserved.providers[0].accounts[0].provenance_count,
        discovered_identity.provenance_count
    );
}

#[test]
fn stale_projection_cas_preserves_identity_metadata_projection_and_durable_envelope() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [entry.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        store.clone(),
    )
    .with_catalog([entry.clone()]);

    let generation = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    coordinator
        .join_generation(&account, generation, Duration::from_secs(1), 1_001)
        .unwrap();
    publisher.observe(&account);
    assert!(publisher.publish_due(1_001));

    let committed_metadata = AccountIdentityMetadata {
        identity_kind: UsageIdentityKindV1::ProviderStableHandle,
        provenance_count: 2,
    };
    let accepted = publisher
        .reconcile_catalog_if_projection(
            None,
            CatalogReconciliation {
                catalog_revision: "accepted-discovery".to_owned(),
                entries: vec![entry.clone()],
                diagnostics: CatalogDiagnostics::default(),
                identity_metadata: Some(BTreeMap::from([(account.clone(), committed_metadata)])),
            },
            1_002,
        )
        .unwrap();
    assert_eq!(
        accepted.providers[0].accounts[0].identity_kind,
        committed_metadata.identity_kind
    );
    let projection_before = publisher.current_projection().unwrap();
    let metadata_before = publisher.identity_metadata.lock().unwrap().clone();
    let envelope_before = store
        .load()
        .unwrap()
        .expect("accepted projection is durable");

    let error = publisher
        .reconcile_catalog_if_projection(
            Some("test:0"),
            CatalogReconciliation {
                catalog_revision: "stale-discovery-must-not-commit".to_owned(),
                entries: vec![UsageCatalogEntry {
                    capability: account.clone(),
                    revision: "credential-b".to_owned(),
                }],
                diagnostics: CatalogDiagnostics::default(),
                identity_metadata: Some(BTreeMap::from([(
                    account,
                    AccountIdentityMetadata {
                        identity_kind: UsageIdentityKindV1::UnverifiedHandle,
                        provenance_count: 9,
                    },
                )])),
            },
            1_003,
        )
        .unwrap_err();

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(publisher.current_projection().unwrap(), projection_before);
    assert_eq!(
        publisher.identity_metadata.lock().unwrap().clone(),
        metadata_before,
        "stale discovery metadata must not replace the accepted map"
    );
    assert_eq!(store.load().unwrap(), Some(envelope_before));
}

#[test]
fn failed_catalog_executor_rolls_back_projection_and_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let executor = Arc::new(FailingCatalogExecutor {
        reconciles: AtomicUsize::new(0),
    });
    let broker_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = broker_executor;
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        broker_executor,
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [old.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        store.clone(),
    )
    .with_catalog([old.clone()])
    .with_identity_metadata(BTreeMap::from([(
        account.clone(),
        AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::LocalSourceHandle,
            provenance_count: 4,
        },
    )]));

    let error = publisher
        .reconcile_catalog_if_projection(
            None,
            CatalogReconciliation {
                catalog_revision: "new-catalog".to_owned(),
                entries: Vec::new(),
                diagnostics: CatalogDiagnostics::default(),
                identity_metadata: Some(BTreeMap::new()),
            },
            1_001,
        )
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::ProviderUnavailable);
    assert_eq!(executor.reconciles.load(Ordering::SeqCst), 2);
    assert_eq!(projection.lock().unwrap().discovery_revision, "catalog");
    assert_eq!(
        publisher.identity_metadata.lock().unwrap().get(&account),
        Some(&AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::LocalSourceHandle,
            provenance_count: 4,
        }),
        "failed catalog reconciliation must preserve the committed metadata"
    );
    assert_eq!(
        publisher.known_capabilities(),
        Vec::<UsageAccountCapability>::new()
    );
    assert!(
        store.load().unwrap().is_none(),
        "executor rejection must not create a durable projection"
    );
    assert_eq!(coordinator.current(&account, 1_001).unwrap().generation, 0);
}

#[test]
fn durable_projection_failure_does_not_activate_new_executor_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let broker_dir = temp.path().join("usage-broker");
    fs::create_dir_all(&broker_dir).unwrap();
    fs::create_dir(broker_dir.join("projection.json")).unwrap();
    let account = capability();
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let executor = Arc::new(FailingCatalogExecutor {
        reconciles: AtomicUsize::new(0),
    });
    let broker_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = broker_executor;
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        broker_executor,
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [old.clone()],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([old]);

    let error = publisher
        .reconcile_catalog_if_projection(
            None,
            catalog_reconciliation("new-catalog", Vec::new(), CatalogDiagnostics::default()),
            1_001,
        )
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(executor.reconciles.load(Ordering::SeqCst), 0);
    assert_eq!(projection.lock().unwrap().discovery_revision, "catalog");
}
#[test]
fn account_for_view_preserves_canonical_access_lifecycles() {
    for (status, expected) in [
        (
            UsageSnapshotStatus::NeedsLogin,
            UsageLifecycleV1::NeedsLogin,
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageLifecycleV1::NeedsSecret,
        ),
    ] {
        let mut snapshot = fresh_view();
        snapshot.status = status;
        let account = account_for_view(
            &UsageGenerationView {
                capability: capability(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(snapshot),
                error: None,
                retry_at_epoch: None,
            },
            0,
            None,
        );
        assert_eq!(account.lifecycle, expected);
    }
    for (kind, expected) in [
        (
            UsageCoordinationErrorKind::Unauthorized,
            UsageLifecycleV1::NeedsLogin,
        ),
        (
            UsageCoordinationErrorKind::NeedsSecret,
            UsageLifecycleV1::NeedsSecret,
        ),
        (
            UsageCoordinationErrorKind::ProtocolMismatch,
            UsageLifecycleV1::Unsupported,
        ),
    ] {
        let account = account_for_view(
            &UsageGenerationView {
                capability: capability(),
                generation: 1,
                phase: UsageRefreshPhase::Failed,
                snapshot: None,
                error: Some(UsageCoordinationError {
                    kind,
                    message: "Account access unavailable".to_owned(),
                }),
                retry_at_epoch: None,
            },
            0,
            None,
        );
        assert_eq!(account.lifecycle, expected);
    }
}
