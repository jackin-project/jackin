// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_protocol::usage_broker::UsageCredentialSourceIdentity;

use super::*;

#[derive(Default)]
struct CountingSecretSource {
    resolutions: AtomicUsize,
}

impl ProviderCredentialSecretSource for CountingSecretSource {
    fn lookup_declaration(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<EnvValue> {
        config.env.get(entry.name).cloned()
    }

    fn resolve_secret(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        self.resolutions.fetch_add(1, Ordering::Relaxed);
        Some(ProviderCredentialSecretResolution {
            declaration: config.env.get(entry.name)?.clone(),
            outcome: ProviderCredentialSecretOutcome::Resolved("fixture-secret".to_owned()),
        })
    }
}

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
