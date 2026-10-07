// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selected_routes_require_exact_source_proofs_and_same_identity() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Zai,
        subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
    };
    let material = env_material("ZAI_HOST_SECRET", "zai-secret");
    let binding = |key: &str, handle: &str| ValidatedCredentialBinding {
        surface: HostSurfaceId::Zai,
        identity: Some(identity.clone()),
        source_id: format!("source-{handle}"),
        capability_id: format!("capability-{handle}"),
        credential_revision: format!("revision-{handle}"),
        provenance: BTreeSet::from(["account zai".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new(handle),
            key: "ZAI_API_KEY".to_owned(),
            dispatch_key: "ZAI_API_KEY".to_owned(),
            launch_keys: BTreeSet::from([key.to_owned()]),
            material: Some(material.clone()),
        },
    };
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            binding("ZHIPU_API_KEY", "zhipu"),
            binding("ZAI_API_KEY", "zai"),
        ],
    };
    let staged = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from(["zai".to_owned()]),
        selected_account_surfaces: BTreeMap::from([("zai".to_owned(), "zai".to_owned())]),
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned(), "ZAI_API_KEY".to_owned()]),
        credential_scope: UsageCredentialScope {
            sources: BTreeSet::from([
                UsageCredentialSourceProof {
                    account_id: "zai".to_owned(),
                    surface_id: "zai".to_owned(),
                    key: "Z_AI_API_KEY".to_owned(),
                    source: material.source.clone(),
                    material_fingerprint: material.material_fingerprint.clone(),
                },
                UsageCredentialSourceProof {
                    account_id: "zai".to_owned(),
                    surface_id: "zai".to_owned(),
                    key: "ZAI_API_KEY".to_owned(),
                    source: material.source.clone(),
                    material_fingerprint: material.material_fingerprint.clone(),
                },
            ]),
        },
    };
    let capability =
        usage_capability_for_selected_account_with_sources(&discovery, "zai", "zai", Some(&staged));
    assert!(
        capability.is_some(),
        "same identity may combine route proofs"
    );

    let wrong_material = ForwardedUsageSources {
        credential_scope: env_scope(
            "zai",
            "zai",
            "ZAI_API_KEY",
            &env_material("ZAI_HOST_SECRET", "other"),
        ),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_material),
        ),
        None
    );

    let wrong_account = ForwardedUsageSources {
        credential_scope: env_scope("other", "zai", "ZAI_API_KEY", &material),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_account),
        ),
        None
    );

    let wrong_source = ForwardedUsageSources {
        credential_scope: env_scope(
            "zai",
            "zai",
            "ZAI_API_KEY",
            &env_material("OTHER_HOST_SECRET", "zai-secret"),
        ),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_source),
        ),
        None
    );

    let different_identity = ValidatedUsageDiscovery {
        bindings: vec![
            discovery.bindings[0].clone(),
            ValidatedCredentialBinding {
                identity: Some(CanonicalAccountIdentity {
                    surface: HostSurfaceId::Zai,
                    subject: CanonicalAccountSubject::ProviderStableHandle(
                        "different-zai-account".to_owned(),
                    ),
                }),
                ..discovery.bindings[1].clone()
            },
        ],
        ..discovery
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &different_identity,
            "zai",
            "zai",
            Some(&staged),
        ),
        None,
        "different provider identities must not collapse into one route"
    );
}

#[test]
fn grouped_broker_authorization_accepts_sibling_proofs_and_rejects_conflicts() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Zai,
        subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
    };
    let material = env_material("ZAI_HOST_SECRET", "zai-secret");
    let binding = |handle: &str,
                   identity: Option<CanonicalAccountIdentity>,
                   material: &ProviderCredentialSourceMaterial| {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity,
            source_id: format!("source-{handle}"),
            capability_id: format!("capability-{handle}"),
            credential_revision: format!("revision-{handle}"),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned(), "ZAI_API_KEY".to_owned()]),
                material: Some(material.clone()),
            },
        }
    };
    let bindings = vec![binding("zai", Some(identity.clone()), &material)];
    let valid = UsageCredentialScope {
        sources: BTreeSet::from([
            UsageCredentialSourceProof {
                account_id: "zai".to_owned(),
                surface_id: "zai".to_owned(),
                key: "ZHIPU_API_KEY".to_owned(),
                source: material.source.clone(),
                material_fingerprint: material.material_fingerprint.clone(),
            },
            UsageCredentialSourceProof {
                account_id: "zai".to_owned(),
                surface_id: "zai".to_owned(),
                key: "ZAI_API_KEY".to_owned(),
                source: material.source.clone(),
                material_fingerprint: material.material_fingerprint.clone(),
            },
        ]),
    };
    assert!(authorize_credential_binding_group(&bindings, "zai", &valid).is_some());

    let mut unrelated = valid.clone();
    unrelated.sources.insert(UsageCredentialSourceProof {
        account_id: "other-account".to_owned(),
        surface_id: "zai".to_owned(),
        key: "Z_AI_API_KEY".to_owned(),
        source: material.source.clone(),
        material_fingerprint: material.material_fingerprint.clone(),
    });
    assert!(authorize_credential_binding_group(&bindings, "zai", &unrelated).is_some());

    let conflicting_material = env_material("ZAI_HOST_SECRET", "different-secret");
    let mut conflict = valid.clone();
    conflict.sources.insert(UsageCredentialSourceProof {
        account_id: "zai".to_owned(),
        surface_id: "zai".to_owned(),
        key: "ZHIPU_API_KEY".to_owned(),
        source: conflicting_material.source.clone(),
        material_fingerprint: conflicting_material.material_fingerprint.clone(),
    });
    assert!(authorize_credential_binding_group(&bindings, "zai", &conflict).is_none());

    let duplicate_conflict = vec![
        bindings[0].clone(),
        binding(
            "zhipu-other",
            Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Zai,
                subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
            }),
            &conflicting_material,
        ),
    ];
    assert!(
        authorize_credential_binding_group(
            &duplicate_conflict,
            "zai",
            &env_scope("zai", "zai", "ZHIPU_API_KEY", &material),
        )
        .is_none()
    );
}

#[test]
fn scoped_probe_refreshes_exact_binding_selected_by_later_sibling_proof() {
    let capability = UsageAccountCapability {
        account_id: "zai".to_owned(),
        surface_id: "zai".to_owned(),
    };
    let material_a = env_material("SOURCE_A", "secret-a");
    let material_b = env_material("SOURCE_B", "secret-b");
    let binding = |handle: &str, key: &str, material: &ProviderCredentialSourceMaterial| {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity: None,
            source_id: format!("source-{handle}"),
            capability_id: "capability-zai".to_owned(),
            credential_revision: format!("revision-{handle}"),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from([key.to_owned()]),
                material: Some(material.clone()),
            },
        }
    };
    let resolver = Arc::new(RecordingRefreshResolver::default());
    let cloned_resolver = Arc::clone(&resolver);
    let resolver_for_executor: Arc<dyn ProviderCredentialEnvResolver> = cloned_resolver;
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(
            capability.clone(),
            vec![
                binding("handle-a", "ZAI_API_KEY", &material_a),
                binding("handle-b", "ZHIPU_API_KEY", &material_b),
            ],
        )])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::Capsule {
            forwarded_accounts: Vec::new(),
        },
        resolver: resolver_for_executor,
        probe_budget: Duration::from_secs(1),
    };
    let scope = env_scope("zai", "zai", "ZHIPU_API_KEY", &material_b);
    assert!(matches!(
        probe_with_scope(&executor, &capability, Some(&scope)),
        ProviderProbeOutcome::Success(_)
    ));
    assert_eq!(
        resolver.calls.lock().unwrap().as_slice(),
        &[(
            "ZAI_API_KEY".to_owned(),
            OpaqueCredentialHandle::new("handle-b"),
        )]
    );
}

#[test]
fn mixed_profile_and_env_group_fails_closed_without_changing_pure_profile() {
    let material = env_material("AMP_SOURCE", "amp-secret");
    let profile = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        source_id: "profile".to_owned(),
        capability_id: "capability".to_owned(),
        credential_revision: "profile-revision".to_owned(),
        provenance: BTreeSet::from(["account shared".to_owned()]),
        source: ValidatedCredentialSource::Profile(discovery::ProfileCredentialMaterial::Amp {
            key: "profile-secret".to_owned(),
        }),
    };
    let env = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        source_id: "env".to_owned(),
        capability_id: "capability".to_owned(),
        credential_revision: "env-revision".to_owned(),
        provenance: BTreeSet::from(["account shared".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("env-handle"),
            key: "AMP_API_KEY".to_owned(),
            dispatch_key: "AMP_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            material: Some(material.clone()),
        },
    };
    let bindings = vec![profile.clone(), env];
    let scope = env_scope("shared", "amp", "AMP_API_KEY", &material);
    assert!(authorize_credential_binding_group(&bindings, "amp", &scope).is_none());
    assert!(authorize_credential_binding_group(&[profile], "amp", &scope).is_some());
}

#[test]
fn capability_identity_keeps_distinct_and_anonymous_sources_separate() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let make = |identity: Option<CanonicalAccountIdentity>,
                capability_id: &str,
                handle: &str|
     -> ValidatedCredentialBinding {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity,
            source_id: capability_id.to_owned(),
            capability_id: capability_id.to_owned(),
            credential_revision: "revision".to_owned(),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned()]),
                material: Some(env_material("ZAI_HOST_SECRET", "zai-secret")),
            },
        }
    };
    let first = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::ProviderStableHandle("first".to_owned()),
        }),
        "first",
        "handle-first",
    );
    let second = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::ProviderStableHandle("second".to_owned()),
        }),
        "second",
        "handle-second",
    );
    assert_ne!(
        capability_for_binding(&first, None),
        capability_for_binding(&second, None)
    );

    let anonymous_first = make(None, "anonymous-first", "handle-anonymous-first");
    let anonymous_second = make(None, "anonymous-second", "handle-anonymous-second");
    assert_ne!(
        capability_for_binding(&anonymous_first, None),
        capability_for_binding(&anonymous_second, None),
        "anonymous bindings remain source-specific"
    );
}
