use super::*;

#[test]
fn typed_same_label_accounts_remain_distinct_through_host_catalog_membership() {
    let temp = tempfile::tempdir().unwrap();
    let forwarded_accounts = ["source-account-one", "source-account-two"]
        .into_iter()
        .map(|id| {
            let identity = CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::SourceCapability(id.to_owned()),
            };
            ForwardedUsageAccount {
                canonical_identity: Some(identity.protocol_identity()),
                surface_id: HostSurfaceId::Claude.id().to_owned(),
                capability_id: format!("route-{id}"),
                account_label: Some("same-display-label".to_owned()),
            }
        })
        .collect();
    let discovery_catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule { forwarded_accounts },
        &NoEnvResolver,
    )
    .unwrap();
    let discovery = validate_usage_sources(discovery_catalog, &NoEnvResolver);
    let current_catalog = super::super::super::broker::usage_catalog_entries(&discovery);
    let mut views = Vec::new();
    for account in &discovery.accounts {
        let proof = account.identity.protocol_identity();
        let route = current_catalog
            .iter()
            .find(|entry| entry.canonical_identity.as_ref() == Some(&proof))
            .expect("current catalog route");
        let mut view = claude_api_key_diagnostic();
        view.canonical_identity = Some(proof);
        view.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: route.capability.account_id.clone(),
            surface_id: route.capability.surface_id.clone(),
            source_revision: Some(route.revision.clone()),
        });
        views.push((HostSurfaceId::Claude, view, true));
    }
    let catalog = super::super::super::accounts::materialize_account_catalog(
        &views,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &temp.path().join("absent-store.sqlite"),
        Some(&discovery),
    )
    .unwrap();
    let rows = catalog.entries_for_surface(HostSurfaceId::Claude);
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].account_key, rows[1].account_key);
    assert_ne!(rows[0].identity, rows[1].identity);
    assert!(
        rows.iter()
            .all(|entry| entry.account_label == "same-display-label")
    );

    let mut forged = claude_api_key_diagnostic();
    forged.account.account_label = discovery.accounts[0].account_label.clone();
    forged.confidence = UsageConfidence::Authoritative;
    let identityless = super::super::super::accounts::materialize_account_catalog(
        &[(HostSurfaceId::Claude, forged, true)],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &temp.path().join("absent-store.sqlite"),
        Some(&discovery),
    )
    .unwrap();
    assert!(
        identityless
            .entries_for_surface(HostSurfaceId::Claude)
            .is_empty()
    );
}

#[test]
fn logical_identity_survives_display_and_route_revision_changes() {
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Claude,
        subject: CanonicalAccountSubject::ProviderId("provider-issued-subject".to_owned()),
    };
    let mut first = claude_api_key_diagnostic();
    first.canonical_identity = Some(identity.protocol_identity());
    first.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
        account_id: "old-catalog-route".to_owned(),
        surface_id: "claude".to_owned(),
        source_revision: None,
    });
    let mut next = first.clone();
    next.account_identity.as_mut().unwrap().account_id = "new-catalog-route".to_owned();
    next.account.account_label = "renamed-display-label".to_owned();
    next.account.provider_label = "arbitrary presentation".to_owned();
    assert_eq!(
        super::super::super::accounts::account_key_for_view(&first),
        Some(identity.account_key())
    );
    assert_eq!(
        super::super::super::accounts::account_key_for_view(&first),
        super::super::super::accounts::account_key_for_view(&next)
    );
    assert_eq!(
        super::super::super::accounts::canonical_account_id_for_view(&first),
        super::super::super::accounts::canonical_account_id_for_view(&next)
    );
    next.canonical_identity.as_mut().unwrap().surface_id = "codex".to_owned();
    assert!(CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &next).is_none());
}

fn claude_api_key_diagnostic() -> FocusedUsageView {
    let mut view = crate::usage::claude_api_key_snapshot(
        "claude",
        Some("Claude"),
        "ANTHROPIC_API_KEY",
        "fixture-only",
        1_800_000_000,
    );
    view.account.account_label = "same-display-label".to_owned();
    view
}

#[test]
fn exact_route_mapping_preserves_stable_subject_across_catalog_revisions() {
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![ForwardedUsageAccount {
                canonical_identity: Some(
                    CanonicalAccountIdentity {
                        surface: HostSurfaceId::Claude,
                        subject: CanonicalAccountSubject::SourceCapability(
                            "stable-forwarded-source".to_owned(),
                        ),
                    }
                    .protocol_identity(),
                ),
                surface_id: "claude".to_owned(),
                capability_id: "forwarded-source-subject".to_owned(),
                account_label: Some("same-display-label".to_owned()),
            }],
        },
        &NoEnvResolver,
    )
    .unwrap();
    let mut discovery = validate_usage_sources(catalog, &NoEnvResolver);
    discovery.config_generation = Some("catalog-one".to_owned());
    let forwarded = crate::host::usage_broker_capabilities(&discovery)
        .pop()
        .unwrap();
    assert_eq!(forwarded.account_id, "forwarded-source-subject");
    discovery.config_generation = Some("another-local-generation".to_owned());
    assert_eq!(
        crate::host::usage_broker_capabilities(&discovery),
        vec![forwarded]
    );
    // The same authenticated subject on a host-owned source gets local
    // revision fencing; an upstream forwarded route is never reissued.
    discovery.bindings[0].source = ValidatedCredentialSource::Unpollable;
    discovery.config_generation = Some("catalog-one".to_owned());
    let first = crate::host::usage_broker_capabilities(&discovery)
        .pop()
        .unwrap();
    let logical = crate::host::usage_projection_account_for_capability(&discovery, &first).unwrap();
    let entries = super::super::super::broker::usage_catalog_entries(&discovery);
    assert_eq!(
        entries[0].canonical_identity,
        Some(discovery.accounts[0].identity.protocol_identity())
    );
    discovery.config_generation = Some("catalog-two".to_owned());
    let next = crate::host::usage_broker_capabilities(&discovery)
        .pop()
        .unwrap();
    assert_ne!(first, next);
    assert!(crate::host::usage_projection_account_for_capability(&discovery, &first).is_none());
    assert_eq!(
        crate::host::usage_projection_account_for_capability(&discovery, &next),
        Some(logical)
    );
}

#[test]
fn forwarded_display_label_without_host_subject_cannot_authenticate() {
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![ForwardedUsageAccount {
                canonical_identity: None,
                surface_id: "claude".to_owned(),
                capability_id: "catalog-revision-route".to_owned(),
                account_label: Some("configured-display-label".to_owned()),
            }],
        },
        &NoEnvResolver,
    )
    .unwrap();
    let discovery = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(discovery.accounts.is_empty());
    assert_eq!(discovery.unresolved_capabilities().count(), 1);
    let capability = crate::host::usage_broker_capabilities(&discovery)
        .pop()
        .unwrap();
    assert!(
        crate::host::usage_projection_account_for_capability(&discovery, &capability).is_none()
    );
}

#[test]
fn conflicting_forwarded_route_subjects_are_rejected_without_first_winner() {
    let accounts = [
        "typed-subject-one",
        "typed-subject-two",
        "typed-subject-one",
    ]
    .into_iter()
    .map(|id| ForwardedUsageAccount {
        canonical_identity: Some(
            CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderId(id.to_owned()),
            }
            .protocol_identity(),
        ),
        surface_id: "claude".to_owned(),
        capability_id: "same-opaque-route".to_owned(),
        account_label: Some("same-display-label".to_owned()),
    })
    .collect();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: accounts,
        },
        &NoEnvResolver,
    )
    .unwrap();
    let discovery = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(discovery.accounts.is_empty());
    assert!(discovery.bindings.is_empty());
    assert_eq!(discovery.diagnostics.len(), 1);
    assert_eq!(
        discovery.diagnostics[0].issue,
        UsageDiscoveryIssue::CredentialMalformed
    );
}

#[test]
fn forwarded_route_aliases_keep_route_provenance_distinct() {
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Claude,
        subject: CanonicalAccountSubject::ProviderId("one-authenticated-subject".to_owned()),
    };
    let forwarded_accounts = ["issuer-route-one", "issuer-route-two"]
        .into_iter()
        .map(|route| ForwardedUsageAccount {
            canonical_identity: Some(identity.protocol_identity()),
            surface_id: "claude".to_owned(),
            capability_id: route.to_owned(),
            account_label: Some("same-display-label".to_owned()),
        })
        .collect();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule { forwarded_accounts },
        &NoEnvResolver,
    )
    .unwrap();
    let discovery = validate_usage_sources(catalog, &NoEnvResolver);
    assert_eq!(discovery.accounts.len(), 1);
    let entries = super::super::super::broker::usage_catalog_entries(&discovery);
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.provenance_count == 2));
    assert!(
        entries
            .iter()
            .all(|entry| entry.canonical_identity == Some(identity.protocol_identity()))
    );
    assert_ne!(entries[0].capability, entries[1].capability);
    for entry in &entries {
        assert_eq!(
            crate::host::usage_projection_account_for_capability(&discovery, &entry.capability),
            Some(identity.canonical_id_v1())
        );
    }
}

#[test]
fn distinct_exact_provider_subject_bytes_never_collapse() {
    for subjects in [
        [
            CanonicalAccountSubject::ProviderStableHandle("CaseHandle".to_owned()),
            CanonicalAccountSubject::ProviderStableHandle("casehandle".to_owned()),
        ],
        [
            CanonicalAccountSubject::ProviderStableHandle("handle".to_owned()),
            CanonicalAccountSubject::ProviderStableHandle(" handle ".to_owned()),
        ],
        [
            CanonicalAccountSubject::ProviderId("subject".to_owned()),
            CanonicalAccountSubject::ProviderId(" subject ".to_owned()),
        ],
    ] {
        let first = CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: subjects[0].clone(),
        };
        let second = CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: subjects[1].clone(),
        };
        assert_ne!(first.account_key(), second.account_key());
        assert_ne!(first.canonical_id_v1(), second.canonical_id_v1());
        let mut first_view = claude_api_key_diagnostic();
        first_view.canonical_identity = Some(first.protocol_identity());
        let mut second_view = first_view.clone();
        second_view.canonical_identity = Some(second.protocol_identity());
        assert_eq!(
            CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &first_view),
            Some(first)
        );
        assert_eq!(
            CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &second_view),
            Some(second)
        );
    }
}

#[test]
fn exact_subject_identity_is_partitioned_by_surface_and_evidence_kind() {
    let subjects = [
        CanonicalAccountSubject::ProviderId("same-bytes".to_owned()),
        CanonicalAccountSubject::ProviderStableHandle("same-bytes".to_owned()),
        CanonicalAccountSubject::SourceCapability("same-bytes".to_owned()),
    ];
    let identities = HostSurfaceId::ALL
        .iter()
        .flat_map(|surface| {
            subjects
                .iter()
                .map(move |subject| CanonicalAccountIdentity {
                    surface: *surface,
                    subject: subject.clone(),
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        identities
            .iter()
            .map(CanonicalAccountIdentity::account_key)
            .collect::<BTreeSet<_>>()
            .len(),
        identities.len()
    );
    assert_eq!(
        identities
            .iter()
            .map(CanonicalAccountIdentity::canonical_id_v1)
            .collect::<BTreeSet<_>>()
            .len(),
        identities.len()
    );
}

#[test]
fn authenticated_discovery_preserves_exact_provider_subject_bytes() {
    struct AuthenticatedResolver {
        inner: SecretDedupFakeResolver,
        provider_id: String,
    }
    impl ProviderCredentialEnvResolver for AuthenticatedResolver {
        fn resolve_provider_credentials(
            &self,
            config: &AppConfig,
            workspace: Option<&WorkspaceName>,
            role: Option<&str>,
            keys: &[UsageCredentialEnvName],
        ) -> Vec<ProviderCredentialEnvResolution> {
            self.inner
                .resolve_provider_credentials(config, workspace, role, keys)
        }
        fn identify_provider_credential(
            &self,
            _surface: HostSurfaceId,
            _handle: &OpaqueCredentialHandle,
        ) -> ProviderCredentialIdentityOutcome {
            ProviderCredentialIdentityOutcome::Authenticated {
                provider_id: Some(self.provider_id.clone()),
                account_label: Some("same-display-label".to_owned()),
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[("configured-key", AiProvider::Anthropic, "fixture-only")],
    );
    let mut keys = BTreeSet::new();
    for provider_id in ["provider-subject", " provider-subject "] {
        let resolver = AuthenticatedResolver {
            inner: SecretDedupFakeResolver::default(),
            provider_id: provider_id.to_owned(),
        };
        let discovery = validate_usage_sources(
            discover_with(&config_root, &temp.path().join("home"), &resolver),
            &resolver,
        );
        assert_eq!(discovery.accounts.len(), 1);
        assert_eq!(
            discovery.accounts[0].identity.subject,
            CanonicalAccountSubject::ProviderId(provider_id.to_owned())
        );
        keys.insert(discovery.accounts[0].account_key.clone());
    }
    assert_eq!(keys.len(), 2);
}
