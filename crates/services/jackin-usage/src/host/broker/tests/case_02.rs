// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn broker_failure_without_snapshot_surfaces_honest_gap_in_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "codex-key".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "codex-key".to_owned(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-openai-key".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    fs::create_dir_all(&config_root).unwrap();
    fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let validated = validate_usage_sources(
        discover_usage_sources(
            &UsageDiscoveryScope::HostDesktop {
                config_root,
                operator_home: temp.path().join("home"),
            },
            &FixedHandleResolver,
        )
        .unwrap(),
        &FixedHandleResolver,
    );
    assert_eq!(validated.accounts.len(), 1);
    let capabilities = usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);

    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(
            temp.path().join("data"),
        ))
        .unwrap();
    runtime.discovery = Some(validated);
    runtime
        .apply_broker_generation(failed_generation(
            &capabilities[0],
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ))
        .unwrap();

    let view = runtime.snapshot("codex").unwrap();
    assert!(!view.is_refreshing_placeholder());
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.account.account_label, "codex-key");
    assert_eq!(
        view.last_error.as_deref(),
        Some("Grok billing requires an authenticated profile")
    );

    // A later success still replaces the recorded error view.
    let mut fresh = quota_view();
    fresh.account.provider_label = "OpenAI / Codex".to_owned();
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: capabilities[0].clone(),
            generation: 2,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(fresh),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();
    let view = runtime.snapshot("codex").unwrap();
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
}

#[test]
fn broker_failure_for_anonymous_source_stays_surface_scoped() {
    let validated = validate_usage_sources(
        discover_usage_sources(
            &UsageDiscoveryScope::Capsule {
                forwarded_accounts: vec![ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: None,
                }],
            },
            &FixedHandleResolver,
        )
        .unwrap(),
        &FixedHandleResolver,
    );
    assert!(validated.accounts.is_empty());
    assert!(validated.bindings[0].identity.is_none());
    let capabilities = usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);

    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(
            temp.path().join("data"),
        ))
        .unwrap();
    runtime.discovery = Some(validated);
    runtime
        .apply_broker_generation(failed_generation(
            &capabilities[0],
            UsageCoordinationErrorKind::NeedsSecret,
            "Gemini auth not available to Capsule",
        ))
        .unwrap();

    let view = runtime.snapshot("codex").unwrap();
    assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
    assert_eq!(
        view.last_error.as_deref(),
        Some("Gemini auth not available to Capsule")
    );
    assert!(runtime.list_accounts(None).unwrap().is_empty());
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One table-style forwarding matrix: six source fixtures share one discovery setup; splitting would duplicate the binding fixtures per case."
)]
fn forwarded_scope_selects_only_accounts_backed_by_forwarded_sources() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

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
    assert_eq!(env_only, vec![env_capability.clone()]);

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
        usage_capability_for_selected_account(&discovery, "account-profile", "amp"),
        Some(profile_capability.clone())
    );
    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "claude"),
        None
    );

    let publication = publication_identity_metadata(&discovery);
    assert_eq!(
        publication[&profile_capability].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(publication[&profile_capability].provenance_count, 2);
}
