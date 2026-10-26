// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One table-style forwarding matrix: six source fixtures share one discovery setup; splitting would duplicate the binding fixtures per case."
)]
fn forwarded_scope_selects_only_accounts_backed_by_forwarded_sources() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let profile_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("profile@example.test".to_owned()),
    };
    let env_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("env@example.test".to_owned()),
    };
    let env_material = ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: "AMP_API_KEY".to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint("env-secret"),
    };
    let scope = "workspace sample role test";
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(profile_identity),
                source_id: "profile-source".to_owned(),
                capability_id: "profile-capability".to_owned(),
                credential_revision: "profile-revision".to_owned(),
                provenance: BTreeSet::from([
                    scope.to_owned(),
                    "account account-profile".to_owned(),
                ]),
                source: ValidatedCredentialSource::Profile(
                    discovery::ProfileCredentialMaterial::Amp {
                        key: "profile-secret".to_owned(),
                    },
                ),
            },
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(env_identity),
                source_id: "env-source".to_owned(),
                capability_id: "env-capability".to_owned(),
                credential_revision: "env-revision".to_owned(),
                provenance: BTreeSet::from([scope.to_owned(), "account account-env".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("env-handle"),
                    key: "AMP_API_KEY".to_owned(),
                    dispatch_key: "AMP_API_KEY".to_owned(),
                    launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
                    material: Some(env_material.clone()),
                },
            },
        ],
    };
    let profile_capability = capability_for_binding(
        &discovery.bindings[0],
        discovery.config_generation.as_deref(),
    );
    let env_capability = capability_for_binding(
        &discovery.bindings[1],
        discovery.config_generation.as_deref(),
    );

    let profile_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(profile_only, vec![profile_capability.clone()]);

    let env_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(
        env_only.is_empty(),
        "a forwarded environment key name without an account/source/material proof grants no capability"
    );

    let env_with_source_proof = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope {
                sources: BTreeSet::from([UsageCredentialSourceProof {
                    account_id: "account-env".to_owned(),
                    surface_id: "amp".to_owned(),
                    key: "AMP_API_KEY".to_owned(),
                    source: env_material.source.clone(),
                    material_fingerprint: env_material.material_fingerprint.clone(),
                }]),
            },
        },
    );
    assert_eq!(env_with_source_proof, vec![env_capability.clone()]);

    let selected_profile = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(selected_profile, vec![profile_capability.clone()]);

    let selected_env_sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from(["account-env".to_owned()]),
        selected_account_surfaces: BTreeMap::from([("account-env".to_owned(), "amp".to_owned())]),
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        credential_scope: UsageCredentialScope {
            sources: BTreeSet::from([UsageCredentialSourceProof {
                account_id: "account-env".to_owned(),
                surface_id: "amp".to_owned(),
                key: "AMP_API_KEY".to_owned(),
                source: env_material.source.clone(),
                material_fingerprint: env_material.material_fingerprint.clone(),
            }]),
        },
    };
    let selected_env = forwarded_usage_capabilities(&discovery, scope, &selected_env_sources);
    assert_eq!(selected_env, vec![env_capability.clone()]);
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-env",
            "amp",
            Some(&selected_env_sources),
        ),
        Some(env_capability.clone())
    );

    let wrong_env_source = ForwardedUsageSources {
        credential_scope: UsageCredentialScope::default(),
        ..selected_env_sources.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-env",
            "amp",
            Some(&wrong_env_source),
        ),
        None,
        "selected account and provider surface need matching source proof"
    );

    let wrong_surface = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "codex".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(
        wrong_surface.is_empty(),
        "account proof must bind both configured account and provider surface"
    );

    let wrong_account = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-does-not-exist".to_owned()]),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(wrong_account.is_empty());

    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-profile",
            "amp",
            None,
        ),
        Some(profile_capability.clone())
    );
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-profile",
            "claude",
            None,
        ),
        None
    );

    let publication = publication_identity_metadata(&discovery);
    assert_eq!(
        publication[&profile_capability].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(publication[&profile_capability].provenance_count, 2);
}

#[test]
fn local_source_capability_metadata_stays_distinct_from_provider_identity() {
    use jackin_usage_host_accounts::CanonicalAccountIdentity;
    use jackin_usage_host_presentation::HostSurfaceId;

    let source_id = "a".repeat(64);
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity::source_capability(
                HostSurfaceId::Claude,
                &source_id,
            )),
            source_id: "selected-source".to_owned(),
            capability_id: "broker-capability".to_owned(),
            credential_revision: "revision".to_owned(),
            provenance: BTreeSet::from(["workspace sample".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let capability = capability_for_binding(&discovery.bindings[0], Some("generation"));

    let metadata = publication_identity_metadata(&discovery);
    let identity_kind = metadata[&capability].identity_kind;
    assert_eq!(identity_kind, UsageIdentityKindV1::LocalSourceHandle);
    assert_ne!(identity_kind, UsageIdentityKindV1::ProviderAccountId);
    assert_ne!(identity_kind, UsageIdentityKindV1::ProviderStableHandle);
}

#[test]
fn missing_discovery_identity_is_unverified_not_provider_authority() {
    use jackin_usage_host_presentation::HostSurfaceId;

    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: None,
            source_id: "selected-source".to_owned(),
            capability_id: "broker-capability".to_owned(),
            credential_revision: "revision".to_owned(),
            provenance: BTreeSet::from(["workspace sample".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let capability = capability_for_binding(&discovery.bindings[0], Some("generation"));

    let metadata = publication_identity_metadata(&discovery);
    let identity_kind = metadata[&capability].identity_kind;
    assert_eq!(identity_kind, UsageIdentityKindV1::UnverifiedHandle);
    assert_ne!(identity_kind, UsageIdentityKindV1::ProviderAccountId);
    assert_ne!(identity_kind, UsageIdentityKindV1::ProviderStableHandle);
    assert_ne!(identity_kind, UsageIdentityKindV1::LocalSourceHandle);
}
