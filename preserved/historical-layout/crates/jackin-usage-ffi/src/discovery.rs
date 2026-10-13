// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Explicit account-declaration adapter for the Rust-owned credential cache.

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_usage::host::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};

#[derive(Default)]
pub(crate) struct DesktopSecretSource;

impl ProviderCredentialSecretSource for DesktopSecretSource {
    fn lookup_declaration(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        config.env.get(entry.name).cloned()
    }

    fn resolve_secret(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        let declaration = config.env.get(entry.name).cloned()?;
        let result = jackin_env::resolve_account_declaration(entry.name, &declaration);
        let outcome = match result.status() {
            jackin_env::OperatorEnvKeyStatus::Resolved => result
                .resolved_value()
                .map_or(ProviderCredentialSecretOutcome::Malformed, |value| {
                    ProviderCredentialSecretOutcome::Resolved(value.to_owned())
                }),
            jackin_env::OperatorEnvKeyStatus::Missing => ProviderCredentialSecretOutcome::Missing,
            jackin_env::OperatorEnvKeyStatus::DeniedOrUnavailable => {
                ProviderCredentialSecretOutcome::Denied
            }
            jackin_env::OperatorEnvKeyStatus::Malformed => {
                ProviderCredentialSecretOutcome::Malformed
            }
            jackin_env::OperatorEnvKeyStatus::InteractionRequired => {
                ProviderCredentialSecretOutcome::InteractionRequired
            }
        };
        Some(ProviderCredentialSecretResolution {
            declaration,
            outcome,
        })
    }
}

pub(crate) type DesktopCredentialResolver = CachedProviderCredentialResolver<DesktopSecretSource>;

#[cfg(test)]
mod tests;
