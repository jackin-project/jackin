// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Default)]
pub(super) struct CountingSecretSource {
    pub(super) resolutions: AtomicUsize,
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
