// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

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
        ProviderCredentialRefreshOutcome::Snapshot(view) => {
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
