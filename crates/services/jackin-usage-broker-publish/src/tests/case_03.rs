// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIssueRecoverabilityV1, UsageIssueScopeV1,
    UsageIssueV1, UsageLifecycleV1, UsageMembershipStateV1, UsageProjectionV1, UsageProviderV1,
};

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

    fn reconcile_keychain_access_unavailable(&self) -> UsageProjectionV1 {
        let mut diagnostics = CatalogDiagnostics::default();
        diagnostics.push_provider_issue(
            "claude",
            "Claude",
            CatalogDiagnosticCode::KeychainAccessUnavailable,
        );
        diagnostics.push_unresolved("claude", "Claude", "opaque-candidate".to_owned(), 1);

        let before = self.publisher.current_projection().unwrap();
        self.publisher
            .reconcile_catalog_if_projection_with_diagnostics(
                Some(&before.projection_id),
                "catalog-keychain-access-unavailable".to_owned(),
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

#[test]
fn unavailable_diagnostic_replaces_retired_catalog_rows_and_clean_scan_removes_it() {
    let fixture = seeded_catalog_fixture();
    seed_retired_keychain_catalog_rows(&fixture);

    let current = fixture.reconcile_keychain_access_unavailable();
    assert!(
        current
            .issues
            .iter()
            .any(|issue| issue.code == "unrelated_projection_issue")
    );
    assert!(current.issues.iter().all(|issue| {
        issue.code != "keychain_consent_required" && issue.code != "keychain_access_unavailable"
    }));
    assert!(
        current
            .providers
            .iter()
            .flat_map(|provider| &provider.issues)
            .all(|issue| issue.code != "keychain_consent_required")
    );
    assert!(
        current
            .providers
            .iter()
            .all(|provider| provider.provider_id != "legacy-only")
    );
    let claude = current
        .providers
        .iter()
        .find(|provider| provider.provider_id == "claude")
        .unwrap();
    let unavailable = claude
        .issues
        .iter()
        .filter(|issue| issue.code == "keychain_access_unavailable")
        .collect::<Vec<_>>();
    assert_eq!(unavailable.len(), 1);
    assert_eq!(unavailable[0].scope, UsageIssueScopeV1::Provider);
    assert_eq!(
        unavailable[0].recoverability,
        UsageIssueRecoverabilityV1::ActionRequired
    );
    assert_eq!(
        unavailable[0].message,
        "Keychain credential access is unavailable"
    );
    assert!(
        current
            .providers
            .iter()
            .find(|provider| provider.provider_id == "unrelated")
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.code == "provider_specific_issue")
    );
    assert_eq!(current.unresolved.len(), 1);
    assert_eq!(current.unresolved[0].state, UsageLifecycleV1::Unavailable);

    let before = fixture.publisher.current_projection().unwrap();
    let clean = fixture
        .publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            Some(&before.projection_id),
            "catalog-clean-after-unavailable".to_owned(),
            vec![fixture.current_entry.clone()],
            CatalogDiagnostics::default(),
            1_006,
        )
        .unwrap();
    assert!(
        clean
            .providers
            .iter()
            .all(|provider| provider.provider_id != "legacy-only")
    );
    let clean_claude = clean
        .providers
        .iter()
        .find(|provider| provider.provider_id == "claude")
        .unwrap();
    assert!(clean_claude.issues.iter().all(|issue| {
        issue.code != "keychain_consent_required" && issue.code != "keychain_access_unavailable"
    }));
    assert!(
        clean
            .providers
            .iter()
            .find(|provider| provider.provider_id == "unrelated")
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.code == "provider_specific_issue")
    );
    assert!(
        clean
            .issues
            .iter()
            .any(|issue| issue.code == "unrelated_projection_issue")
    );
    assert!(clean.issues.iter().all(|issue| {
        issue.code != "keychain_consent_required" && issue.code != "keychain_access_unavailable"
    }));
    assert!(
        clean
            .providers
            .iter()
            .flat_map(|provider| &provider.issues)
            .all(|issue| issue.code != "keychain_consent_required"
                && issue.code != "keychain_access_unavailable")
    );

    let before_repeat = fixture.publisher.current_projection().unwrap();
    let repeated_clean = fixture
        .publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            Some(&before_repeat.projection_id),
            "catalog-clean-repeat".to_owned(),
            vec![fixture.current_entry.clone()],
            CatalogDiagnostics::default(),
            1_007,
        )
        .unwrap();
    assert!(repeated_clean.issues.iter().all(|issue| {
        issue.code != "keychain_consent_required" && issue.code != "keychain_access_unavailable"
    }));
    assert!(
        repeated_clean
            .providers
            .iter()
            .flat_map(|provider| &provider.issues)
            .all(|issue| {
                issue.code != "keychain_consent_required"
                    && issue.code != "keychain_access_unavailable"
            })
    );
}

fn seed_retired_keychain_catalog_rows(fixture: &CatalogFixture) {
    let mut projection = fixture.projection.lock().unwrap();
    projection.issues.extend([
        issue_fixture("keychain_consent_required", UsageIssueScopeV1::Projection),
        issue_fixture("keychain_access_unavailable", UsageIssueScopeV1::Projection),
        issue_fixture("unrelated_projection_issue", UsageIssueScopeV1::Projection),
    ]);
    projection.providers.push(UsageProviderV1 {
        provider_id: "legacy-only".to_owned(),
        display_name: "Legacy-only".to_owned(),
        rank: 99,
        membership_state: UsageMembershipStateV1::Current,
        freshness: UsageFreshnessV1 {
            generation: 0,
            phase: UsageFreshnessPhaseV1::Failed,
            last_good_at_epoch: None,
            retry_at_epoch: None,
            is_stale: false,
        },
        accounts: Vec::new(),
        issues: vec![
            issue_fixture("keychain_consent_required", UsageIssueScopeV1::Provider),
            issue_fixture("keychain_access_unavailable", UsageIssueScopeV1::Provider),
        ],
    });
    projection.providers.push(UsageProviderV1 {
        provider_id: "unrelated".to_owned(),
        display_name: "Unrelated".to_owned(),
        rank: 100,
        membership_state: UsageMembershipStateV1::Current,
        freshness: UsageFreshnessV1 {
            generation: 0,
            phase: UsageFreshnessPhaseV1::Failed,
            last_good_at_epoch: None,
            retry_at_epoch: None,
            is_stale: false,
        },
        accounts: Vec::new(),
        issues: vec![issue_fixture(
            "provider_specific_issue",
            UsageIssueScopeV1::Provider,
        )],
    });
}

fn issue_fixture(code: &str, scope: UsageIssueScopeV1) -> UsageIssueV1 {
    UsageIssueV1 {
        code: code.to_owned(),
        scope,
        recoverability: UsageIssueRecoverabilityV1::ActionRequired,
        message: "fixture diagnostic".to_owned(),
        retry_at_epoch: None,
    }
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

#[test]
fn publication_preserves_unlimited_state_without_inventing_quota_values() {
    let mut view = fresh_view();
    let mut credit = view.buckets.remove(0);
    credit.label = "Credits".to_owned();
    credit.used_label = None;
    credit.limit_label = None;
    credit.used_percent = None;
    credit.used_percent_origin = None;
    credit.remaining_percent = None;
    credit.remaining_percent_origin = None;
    credit.count_quota = None;
    credit.provider_quota_state = Some(jackin_protocol::control::ProviderQuotaState::Unlimited);
    view.buckets = vec![credit];

    let generation = UsageGenerationView {
        capability: capability(),
        generation: 1,
        phase: UsageRefreshPhase::Completed,
        snapshot: Some(view),
        error: None,
        retry_at_epoch: None,
    };
    let mut projection = empty_projection();
    merge_views(&mut projection, &[generation], &BTreeMap::new())
        .expect("unlimited provider state publishes");

    let account = &projection.providers[0].accounts[0];
    let window = &account.windows[0];
    assert_eq!(
        window.provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
    assert_eq!(window.used_percent, None);
    assert_eq!(window.remaining_percent, None);
    assert!(window.count_quota.is_none());
    assert!(window.meter_percent.is_none());
    assert!(window.meter_side.is_none());
    assert_eq!(window.quota_state, UsageQuotaStateV1::NotApplicable);

    let group = account
        .metric_groups
        .iter()
        .find(|group| group.label == "Credits")
        .expect("published Credits metric group");
    assert!(matches!(
        &group.value,
        UsageMetricValueV1::Window {
            used_percent: None,
            remaining_percent: None,
            meter_percent: None,
            meter_side: None,
            count_quota: None,
            provider_quota_state: Some(jackin_protocol::control::ProviderQuotaState::Unlimited),
            ..
        }
    ));
}
