// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1};

#[test]
fn broker_conflict_requires_operator_action_without_retry() {
    let kind = UsageCoordinationErrorKind::BrokerConflict;

    assert_eq!(issue_code(kind), "broker_conflict");
    assert_eq!(failure_lifecycle(kind), UsageLifecycleV1::Error);
    assert_eq!(
        issue_recoverability(kind),
        UsageIssueRecoverabilityV1::ActionRequired
    );
}

struct CatalogFixture {
    _temp: tempfile::TempDir,
    coordinator: Arc<UsageCoordinator>,
    projection: Arc<Mutex<UsageProjectionV1>>,
    store: FileProjectionStateStore,
    publisher: ProjectionPublisher,
    revoked_capability: UsageAccountCapability,
    current_capability: UsageAccountCapability,
    current_entry: UsageCatalogEntry,
}

impl CatalogFixture {
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
            .reconcile_catalog_if_projection_with_diagnostics(
                Some(&before.projection_id),
                "catalog-current".to_owned(),
                vec![self.current_entry.clone()],
                diagnostics,
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

fn seeded_catalog_fixture() -> CatalogFixture {
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
    let fixture = CatalogFixture {
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
    let fixture = seeded_catalog_fixture();
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
    let fixture = seeded_catalog_fixture();
    fixture.reconcile_interaction();
    fixture
        .projection
        .lock()
        .unwrap()
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "claude")
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
        .reconcile_catalog_if_projection_with_diagnostics(
            Some(&before.projection_id),
            "catalog-clean".to_owned(),
            vec![fixture.current_entry.clone()],
            CatalogDiagnostics::default(),
            1_006,
        )
        .unwrap();
    let provider = clean
        .providers
        .iter()
        .find(|provider| provider.provider_id == "claude")
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
}

fn assert_interaction_issue(projection: &UsageProjectionV1) {
    let provider = projection
        .providers
        .iter()
        .find(|provider| provider.provider_id == "claude")
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
