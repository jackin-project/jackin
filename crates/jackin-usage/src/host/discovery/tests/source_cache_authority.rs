use std::collections::BTreeMap;

use jackin_protocol::control::{FocusedUsageView, UsageAccountIdentity, UsageSnapshotStatus};
use jackin_protocol::usage_broker::UsageCatalogEntry;

use super::*;

fn forwarded_discovery(
    subject: CanonicalAccountSubject,
    capability_id: &str,
) -> (
    ValidatedUsageDiscovery,
    CanonicalAccountIdentity,
    UsageCatalogEntry,
) {
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Claude,
        subject,
    };
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![ForwardedUsageAccount {
                canonical_identity: Some(identity.protocol_identity()),
                surface_id: HostSurfaceId::Claude.id().to_owned(),
                capability_id: capability_id.to_owned(),
                account_label: Some("cache-authority@example.test".to_owned()),
            }],
        },
        &NoEnvResolver,
    )
    .unwrap();
    let discovery = validate_usage_sources(catalog, &NoEnvResolver);
    let proof = identity.protocol_identity();
    let entry = super::super::super::broker::usage_catalog_entries(&discovery)
        .into_iter()
        .find(|entry| entry.canonical_identity.as_ref() == Some(&proof))
        .expect("current catalog entry");
    (discovery, identity, entry)
}

fn cached_view(
    identity: &CanonicalAccountIdentity,
    route_account_id: &str,
    source_revision: Option<&str>,
) -> FocusedUsageView {
    let mut view = crate::usage::claude_api_key_snapshot(
        "claude",
        Some("Claude"),
        "ANTHROPIC_API_KEY",
        "fixture-only",
        1_800_000_000,
    );
    view.canonical_identity = Some(identity.protocol_identity());
    view.account_identity = Some(UsageAccountIdentity {
        account_id: route_account_id.to_owned(),
        surface_id: HostSurfaceId::Claude.id().to_owned(),
        source_revision: source_revision.map(str::to_owned),
    });
    view.status = UsageSnapshotStatus::Fresh;
    view
}

fn materialize(
    discovery: &ValidatedUsageDiscovery,
    live: &[(HostSurfaceId, FocusedUsageView, bool)],
    discovered: &BTreeMap<(HostSurfaceId, String), FocusedUsageView>,
    store: &std::path::Path,
) -> super::super::super::accounts::AccountCatalog {
    super::super::super::accounts::materialize_account_catalog(
        live,
        discovered,
        &BTreeMap::new(),
        store,
        Some(discovery),
    )
    .unwrap()
}

#[test]
fn replaced_source_revision_rejects_durable_live_and_discovered_views() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("usage.db");
    let (discovery, identity, current) = forwarded_discovery(
        CanonicalAccountSubject::SourceCapability("replaced-profile".to_owned()),
        "current-route",
    );
    let old = cached_view(&identity, "old-route", Some("old-revision"));
    crate::usage_snapshot_store::store_usage_snapshot(&store, &old).unwrap();
    let mut discovered = BTreeMap::new();
    discovered.insert((HostSurfaceId::Claude, identity.account_key()), old.clone());

    let catalog = materialize(
        &discovery,
        &[(HostSurfaceId::Claude, old, true)],
        &discovered,
        &store,
    );
    let rows = catalog.entries_for_surface(HostSurfaceId::Claude);
    assert_eq!(rows.len(), 1, "membership placeholder remains");
    assert!(
        rows[0].provenance.is_empty(),
        "old source must not materialize"
    );
    assert!(catalog.provider_state(HostSurfaceId::Claude).is_none());
    assert_ne!(current.revision, "old-revision");
}

#[test]
fn source_revision_keeps_benign_route_rotation() {
    let temp = tempfile::tempdir().unwrap();
    let (discovery, identity, current) = forwarded_discovery(
        CanonicalAccountSubject::SourceCapability("rotated-route".to_owned()),
        "current-route",
    );
    let rotated = cached_view(
        &identity,
        "new-opaque-route",
        Some(current.revision.as_str()),
    );
    let catalog = materialize(
        &discovery,
        &[(HostSurfaceId::Claude, rotated, true)],
        &BTreeMap::new(),
        &temp.path().join("missing.db"),
    );
    let rows = catalog.entries_for_surface(HostSurfaceId::Claude);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].provenance.is_empty());
    assert_eq!(rows[0].view.status, UsageSnapshotStatus::Fresh);
}

#[test]
fn malformed_route_ids_cannot_admit_source_cache() {
    let temp = tempfile::tempdir().unwrap();
    let (discovery, identity, current) = forwarded_discovery(
        CanonicalAccountSubject::SourceCapability("malformed-route".to_owned()),
        "current-route",
    );
    let malformed = vec![
        String::new(),
        "route/control\n".to_owned(),
        "route/slash".to_owned(),
        "é".to_owned(),
        "a".repeat(129),
    ];
    for route_account_id in malformed {
        let catalog = materialize(
            &discovery,
            &[(
                HostSurfaceId::Claude,
                cached_view(
                    &identity,
                    &route_account_id,
                    Some(current.revision.as_str()),
                ),
                true,
            )],
            &BTreeMap::new(),
            &temp.path().join("missing-source-cache.db"),
        );
        let rows = catalog.entries_for_surface(HostSurfaceId::Claude);
        assert_eq!(rows.len(), 1, "membership placeholder remains");
        assert!(rows[0].provenance.is_empty(), "route {route_account_id:?}");
        assert!(catalog.provider_state(HostSurfaceId::Claude).is_none());
    }
}

#[test]
fn strong_subject_keeps_last_good_as_stale_when_route_backing_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (discovery, identity, current) = forwarded_discovery(
        CanonicalAccountSubject::ProviderId("stable-provider-account".to_owned()),
        "current-route",
    );
    let old = cached_view(&identity, "old-route", Some("old-revision"));
    let catalog = materialize(
        &discovery,
        &[(HostSurfaceId::Claude, old, true)],
        &BTreeMap::new(),
        &temp.path().join("missing.db"),
    );
    let row = catalog
        .entries_for_surface(HostSurfaceId::Claude)
        .into_iter()
        .find(|entry| !entry.provenance.is_empty())
        .expect("strong cached account");
    assert_eq!(row.view.status, UsageSnapshotStatus::Stale);
    assert_ne!(current.revision, "old-revision");
}

#[test]
fn strong_descriptor_without_current_binding_keeps_only_placeholder() {
    let temp = tempfile::tempdir().unwrap();
    let (mut discovery, identity, _) = forwarded_discovery(
        CanonicalAccountSubject::ProviderId("descriptor-only-subject".to_owned()),
        "current-route",
    );
    discovery.bindings.clear();
    let cached = cached_view(&identity, "old-route", Some("old-revision"));
    let catalog = materialize(
        &discovery,
        &[(HostSurfaceId::Claude, cached, true)],
        &BTreeMap::new(),
        &temp.path().join("missing.db"),
    );
    let rows = catalog.entries_for_surface(HostSurfaceId::Claude);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].provenance.is_empty());
}
