// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;

use jackin_config::{AppConfig, EnvValue};
use jackin_core::{UsageCredentialEnvName, UsageCredentialOwner, WorkspaceName};

use jackin_protocol::usage_broker::{
    UsageCredentialSourceIdentity, usage_credential_material_fingerprint,
};
use jackin_usage_host_credentials::{
    OpaqueCredentialHandle, ProviderCredentialEnvOutcome, ProviderCredentialEnvResolution,
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
    ProviderCredentialRefreshOutcome, ProviderCredentialSourceMaterial,
};
use jackin_usage_host_presentation::HostSurfaceId;
use jackin_usage_provider_core::{canonical_usage_key, dispatch_key_for_route};

pub enum ProviderCredentialSecretOutcome {
    Resolved(String),
    Missing,
    Denied,
    Malformed,
    InteractionRequired,
}

impl std::fmt::Debug for ProviderCredentialSecretOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolved(_) => formatter.write_str("Resolved(REDACTED)"),
            Self::Missing => formatter.write_str("Missing"),
            Self::Denied => formatter.write_str("Denied"),
            Self::Malformed => formatter.write_str("Malformed"),
            Self::InteractionRequired => formatter.write_str("InteractionRequired"),
        }
    }
}

pub struct ProviderCredentialSecretResolution {
    pub declaration: EnvValue,
    pub outcome: ProviderCredentialSecretOutcome,
}

impl std::fmt::Debug for ProviderCredentialSecretResolution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderCredentialSecretResolution")
            .field("declaration", &"REDACTED")
            .field("outcome", &self.outcome)
            .finish()
    }
}

pub trait ProviderCredentialSecretSource: Send + Sync {
    fn lookup_declaration(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<EnvValue>;

    fn resolve_secret(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution>;
}

struct CachedResolution {
    key: String,
    dispatch_key: String,
    owner: UsageCredentialOwner,
    declaration: EnvValue,
    handle: Option<OpaqueCredentialHandle>,
    secret: Option<String>,
    outcome: ProviderCredentialEnvOutcome,
}

#[derive(Default)]
struct ResolverState {
    next_handle: u64,
    cache: Vec<CachedResolution>,
}

pub struct CachedProviderCredentialResolver<S> {
    source: S,
    state: Mutex<ResolverState>,
}

impl<S> std::fmt::Debug for CachedProviderCredentialResolver<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedProviderCredentialResolver")
            .field("cached_resolution_count", &self.cached_resolution_count())
            .finish_non_exhaustive()
    }
}

impl<S: Default> Default for CachedProviderCredentialResolver<S> {
    fn default() -> Self {
        Self {
            source: S::default(),
            state: Mutex::new(ResolverState::default()),
        }
    }
}

impl<S> CachedProviderCredentialResolver<S> {
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            source,
            state: Mutex::new(ResolverState::default()),
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn cached_resolution_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cache
            .len()
    }
}

impl<S: ProviderCredentialSecretSource> CachedProviderCredentialResolver<S> {
    fn resolve_one(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<ProviderCredentialEnvResolution> {
        let declaration = self
            .source
            .lookup_declaration(config, workspace, role, entry)?;
        let cache_key = canonical_usage_key(entry.owner);
        let governed_name =
            jackin_usage_host_credentials::governed_name_for_account_alias(entry.name);
        let dispatch_key = dispatch_key_for_route(entry.owner, governed_name);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = state.cache.iter().find(|cached| {
            cached.key == cache_key
                && cached.dispatch_key == dispatch_key
                && cached.declaration == declaration
        }) {
            return Some(ProviderCredentialEnvResolution {
                key: entry.name.to_owned(),
                outcome: cached.outcome.clone(),
            });
        }
        let resolved = self.source.resolve_secret(config, workspace, role, entry)?;

        let (outcome, secret, handle) = match resolved.outcome {
            ProviderCredentialSecretOutcome::Resolved(secret) if !secret.is_empty() => {
                let reused = state.cache.iter().find_map(|cached| {
                    (cached.owner == entry.owner
                        && cached.dispatch_key == dispatch_key
                        && cached.declaration == declaration
                        && cached.secret.as_deref() == Some(&secret))
                    .then(|| cached.handle.clone())
                    .flatten()
                });
                let handle = reused.unwrap_or_else(|| {
                    state.next_handle = state.next_handle.saturating_add(1);
                    OpaqueCredentialHandle::new(format!("credential-{}", state.next_handle))
                });
                (
                    ProviderCredentialEnvOutcome::Resolved(handle.clone()),
                    Some(secret),
                    Some(handle),
                )
            }
            ProviderCredentialSecretOutcome::Resolved(_) => {
                (ProviderCredentialEnvOutcome::Malformed, None, None)
            }
            ProviderCredentialSecretOutcome::Missing => {
                (ProviderCredentialEnvOutcome::Missing, None, None)
            }
            ProviderCredentialSecretOutcome::Denied => {
                (ProviderCredentialEnvOutcome::Denied, None, None)
            }
            ProviderCredentialSecretOutcome::Malformed => {
                (ProviderCredentialEnvOutcome::Malformed, None, None)
            }
            ProviderCredentialSecretOutcome::InteractionRequired => (
                ProviderCredentialEnvOutcome::InteractionRequired,
                None,
                None,
            ),
        };
        state.cache.push(CachedResolution {
            key: cache_key.to_owned(),
            dispatch_key: dispatch_key.to_owned(),
            owner: entry.owner,
            declaration: resolved.declaration,
            handle,
            secret,
            outcome: outcome.clone(),
        });
        Some(ProviderCredentialEnvResolution {
            key: entry.name.to_owned(),
            outcome,
        })
    }
}

impl<S: ProviderCredentialSecretSource> ProviderCredentialEnvResolver
    for CachedProviderCredentialResolver<S>
{
    fn begin_manual_retry(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .cache
            .retain(|cached| matches!(cached.outcome, ProviderCredentialEnvOutcome::Resolved(_)));
    }

    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        keys.iter()
            .filter_map(|entry| self.resolve_one(config, workspace, role, *entry))
            .collect()
    }

    fn identify_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialIdentityOutcome {
        ProviderCredentialIdentityOutcome::Anonymous
    }

    fn refresh_provider_credential(
        &self,
        surface: HostSurfaceId,
        key: &str,
        handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        let secret = {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.cache.iter().find_map(|cached| {
                (cached.dispatch_key == key && cached.handle.as_ref() == Some(handle))
                    .then(|| cached.secret.clone())
                    .flatten()
            })
        };
        let Some(secret) = secret else {
            return ProviderCredentialRefreshOutcome::Missing;
        };
        let (view, rate_limit, provider_error) =
            super::dispatch::provider_credential_snapshot_with_metadata(surface.id(), key, &secret);
        ProviderCredentialRefreshOutcome::Snapshot {
            view: Box::new(view),
            rate_limit,
            provider_error,
        }
    }

    fn source_material(
        &self,
        _surface: HostSurfaceId,
        key: &str,
        handle: &OpaqueCredentialHandle,
    ) -> Option<ProviderCredentialSourceMaterial> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.cache.iter().find_map(|cached| {
            (cached.key == key && cached.handle.as_ref() == Some(handle))
                .then(|| {
                    Some(ProviderCredentialSourceMaterial {
                        source: UsageCredentialSourceIdentity::from_declaration(
                            &cached.declaration,
                        ),
                        material_fingerprint: usage_credential_material_fingerprint(
                            cached.secret.as_deref()?,
                        ),
                    })
                })
                .flatten()
        })
    }
}
