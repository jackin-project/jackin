// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1};

#[test]
fn interaction_diagnostics_survive_new_catalog_incremental_publish_and_revocation() {
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

    let generation = coordinator
        .request_refresh(&revoked_capability, 0, true, 1_000)
        .unwrap()
        .generation;
    coordinator
        .join_generation(
            &revoked_capability,
            generation,
            Duration::from_secs(1),
            1_001,
        )
        .unwrap();
    publisher.observe(&revoked_capability);
    assert!(publisher.publish_due(1_001));

    let mut diagnostics = CatalogDiagnostics::default();
    diagnostics.push_provider_issue(
        "claude",
        "Claude",
        CatalogDiagnosticCode::InteractionRequired,
    );
    diagnostics.push_unresolved("claude", "Claude", "opaque-candidate".to_owned(), 1);
    let before_reconcile = publisher.current_projection().unwrap();
    let reconciled = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            Some(&before_reconcile.projection_id),
            "catalog-current".to_owned(),
            vec![current_entry.clone()],
            diagnostics,
            1_002,
        )
        .unwrap();
    let revoked = reconciled
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == revoked_capability.account_id)
        .expect("revoked account remains as a tombstone");
    assert_eq!(revoked.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(reconciled.unresolved.len(), 1);
    assert_eq!(
        reconciled.unresolved[0].state,
        UsageLifecycleV1::NeedsSecret
    );
    assert_eq!(reconciled.unresolved[0].capability_id, "opaque-candidate");

    let generation = coordinator
        .request_refresh(&current_capability, 0, true, 1_003)
        .unwrap()
        .generation;
    coordinator
        .join_generation(
            &current_capability,
            generation,
            Duration::from_secs(1),
            1_004,
        )
        .unwrap();
    publisher.observe(&current_capability);
    assert!(publisher.publish_due(1_005));

    let published = publisher.current_projection().unwrap();
    let provider = published
        .providers
        .iter()
        .find(|provider| provider.provider_id == "claude")
        .unwrap();
    let interaction = provider
        .issues
        .iter()
        .find(|issue| issue.code == "interaction_required")
        .expect("provider diagnostic survives incremental publication");
    assert_eq!(interaction.scope, UsageIssueScopeV1::Provider);
    assert_eq!(
        interaction.recoverability,
        UsageIssueRecoverabilityV1::ActionRequired
    );
    assert_eq!(
        interaction.message,
        "Credential access requires interaction"
    );
    assert!(
        published
            .providers
            .iter()
            .flat_map(|provider| provider.accounts.iter())
            .any(|account| account.canonical_account_id == revoked_capability.account_id)
    );
    assert_eq!(published.unresolved.len(), 1);
    assert_eq!(published.unresolved[0].state, UsageLifecycleV1::NeedsSecret);
    assert_eq!(store.load().unwrap().unwrap().projection, published);

    projection
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

    let before_clean_scan = publisher.current_projection().unwrap();
    let clean = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            Some(&before_clean_scan.projection_id),
            "catalog-clean".to_owned(),
            vec![current_entry],
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
