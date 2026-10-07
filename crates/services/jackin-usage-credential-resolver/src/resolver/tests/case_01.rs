// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_source_cache_skips_duplicate_protected_resolution() {
    let mut config = AppConfig::default();
    config.env.insert(
        "ZAI_API_KEY".to_owned(),
        EnvValue::Plain("fixture-declaration".to_owned()),
    );
    let resolver = CachedProviderCredentialResolver::new(CountingSecretSource::default());
    let entry = UsageCredentialEnvName {
        name: "ZAI_API_KEY",
        owner: UsageCredentialOwner::Zai,
    };

    let first = resolver.resolve_provider_credentials(&config, None, None, &[entry]);
    let second = resolver.resolve_provider_credentials(&config, None, None, &[entry]);

    assert_eq!(first, second);
    assert_eq!(resolver.source.resolutions.load(Ordering::Relaxed), 1);
}

#[test]
fn disc_source_cache_alias_request_refreshes_through_governed_name() {
    let mut config = AppConfig::default();
    config.env.insert(
        "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY".to_owned(),
        EnvValue::Plain("fixture-declaration".to_owned()),
    );
    let resolver = CachedProviderCredentialResolver::new(CountingSecretSource::default());
    let alias = UsageCredentialEnvName {
        name: "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY",
        owner: UsageCredentialOwner::Codex,
    };

    let resolutions = resolver.resolve_provider_credentials(&config, None, None, &[alias]);
    assert_eq!(resolutions.len(), 1);
    assert_eq!(resolutions[0].key, "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY");
    let ProviderCredentialEnvOutcome::Resolved(handle) = &resolutions[0].outcome else {
        panic!("alias declaration must resolve");
    };

    // Refresh routing addresses the governed name; the alias-resolved secret
    // must be reachable through it.
    match resolver.refresh_provider_credential(HostSurfaceId::Codex, "OPENAI_API_KEY", handle) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(
                view.last_error.as_deref(),
                Some("OpenAI API-key subscription quota is unavailable")
            );
        }
        other => panic!("governed-name refresh must hit the alias cache: {other:?}"),
    }
    // A direct governed-name request for the same declaration shares the entry.
    let mut governed_config = AppConfig::default();
    governed_config.env.insert(
        "OPENAI_API_KEY".to_owned(),
        EnvValue::Plain("fixture-declaration".to_owned()),
    );
    let governed = UsageCredentialEnvName {
        name: "OPENAI_API_KEY",
        owner: UsageCredentialOwner::Codex,
    };
    let repeat = resolver.resolve_provider_credentials(&governed_config, None, None, &[governed]);
    assert_eq!(repeat.len(), 1);
    assert_eq!(repeat[0].outcome, resolutions[0].outcome);
    assert_eq!(resolver.source.resolutions.load(Ordering::Relaxed), 1);
}

#[test]
fn disc_zai_documented_aliases_share_canonical_refresh_route_without_material_fallback() {
    for alias_name in [
        "JACKIN_USAGE_ACCOUNT_ZAI_API_KEY",
        "JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY",
        "JACKIN_USAGE_ACCOUNT_Z_AI_API_KEY",
    ] {
        let mut config = AppConfig::default();
        config.env.insert(
            alias_name.to_owned(),
            EnvValue::Plain("fixture-declaration".to_owned()),
        );
        let resolver = CachedProviderCredentialResolver::new(CountingSecretSource::default());
        let entry = UsageCredentialEnvName {
            name: alias_name,
            owner: UsageCredentialOwner::Zai,
        };
        let resolutions = resolver.resolve_provider_credentials(&config, None, None, &[entry]);
        let ProviderCredentialEnvOutcome::Resolved(handle) = &resolutions[0].outcome else {
            panic!("documented Z.AI alias must resolve");
        };
        assert!(
            resolver
                .source_material(HostSurfaceId::Zai, "ZAI_API_KEY", handle)
                .is_some()
        );
        assert!(matches!(
            resolver.refresh_provider_credential(HostSurfaceId::Zai, "ZAI_API_KEY", handle),
            ProviderCredentialRefreshOutcome::Snapshot { .. }
        ));
        assert!(matches!(
            resolver.refresh_provider_credential(HostSurfaceId::Zai, "ZHIPU_API_KEY", handle),
            ProviderCredentialRefreshOutcome::Missing
        ));
    }
}

#[test]
fn semantic_dispatch_routes_keep_claude_api_oauth_and_grok_deployment_distinct() {
    let mut config = AppConfig::default();
    for name in [
        jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
        jackin_core::XAI_API_KEY_ENV_NAME,
        jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
    ] {
        config.env.insert(
            name.to_owned(),
            EnvValue::Plain("fixture-declaration".to_owned()),
        );
    }
    let resolver = CachedProviderCredentialResolver::new(CountingSecretSource::default());
    let entries = [
        UsageCredentialEnvName {
            name: jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
            owner: UsageCredentialOwner::Claude,
        },
        UsageCredentialEnvName {
            name: jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
            owner: UsageCredentialOwner::Claude,
        },
        UsageCredentialEnvName {
            name: jackin_core::XAI_API_KEY_ENV_NAME,
            owner: UsageCredentialOwner::Grok,
        },
        UsageCredentialEnvName {
            name: jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
            owner: UsageCredentialOwner::Grok,
        },
    ];
    let resolutions = resolver.resolve_provider_credentials(&config, None, None, &entries);
    assert_eq!(resolutions.len(), entries.len());
    let handles = resolutions
        .iter()
        .map(|resolution| {
            let ProviderCredentialEnvOutcome::Resolved(handle) = &resolution.outcome else {
                panic!("route declaration must resolve");
            };
            handle.clone()
        })
        .collect::<Vec<_>>();
    assert_ne!(handles[0], handles[1], "Claude semantic routes must split");
    assert_ne!(handles[2], handles[3], "Grok semantic routes must split");

    let ProviderCredentialRefreshOutcome::Snapshot { view, .. } = resolver
        .refresh_provider_credential(HostSurfaceId::Claude, "ANTHROPIC_API_KEY", &handles[0])
    else {
        panic!("Claude API route must refresh");
    };
    assert_eq!(
        view.last_error.as_deref(),
        Some("Claude API-key quota is unavailable; OAuth usage requires CLAUDE_CODE_OAUTH_TOKEN")
    );

    let ProviderCredentialRefreshOutcome::Snapshot { view, .. } = resolver
        .refresh_provider_credential(
            HostSurfaceId::Claude,
            "CLAUDE_CODE_OAUTH_TOKEN",
            &handles[1],
        )
    else {
        panic!("Claude OAuth route must refresh");
    };
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("OAuth · configured source")
    );

    let ProviderCredentialRefreshOutcome::Snapshot { view, .. } = resolver
        .refresh_provider_credential(HostSurfaceId::Grok, "GROK_DEPLOYMENT_KEY", &handles[3])
    else {
        panic!("Grok deployment route must refresh");
    };
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("API token · env GROK_DEPLOYMENT_KEY")
    );
}

#[test]
fn source_cache_does_not_reuse_handle_across_repointed_declarations() {
    let mut config = AppConfig::default();
    config.env.insert(
        "ZAI_API_KEY".to_owned(),
        EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item-a/field".to_owned(),
            path: "Vault/Item A/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let resolver = CachedProviderCredentialResolver::new(CountingSecretSource::default());
    let entry = UsageCredentialEnvName {
        name: "ZAI_API_KEY",
        owner: UsageCredentialOwner::Zai,
    };

    let first = resolver.resolve_provider_credentials(&config, None, None, &[entry]);
    config.env.insert(
        "ZAI_API_KEY".to_owned(),
        EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item-b/field".to_owned(),
            path: "Vault/Item B/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let second = resolver.resolve_provider_credentials(&config, None, None, &[entry]);
    let ProviderCredentialEnvOutcome::Resolved(first_handle) =
        &first.first().expect("first result missing").outcome
    else {
        panic!("first declaration did not resolve");
    };
    let ProviderCredentialEnvOutcome::Resolved(second_handle) =
        &second.first().expect("second result missing").outcome
    else {
        panic!("repointed declaration did not resolve");
    };
    assert_ne!(first_handle, second_handle);
    assert_eq!(
        resolver
            .source_material(HostSurfaceId::Zai, entry.name, first_handle)
            .unwrap()
            .source,
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://vault/item-a/field".to_owned(),
            account: None,
        }
    );
    assert_eq!(
        resolver
            .source_material(HostSurfaceId::Zai, entry.name, second_handle)
            .unwrap()
            .source,
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://vault/item-b/field".to_owned(),
            account: None,
        }
    );
}
