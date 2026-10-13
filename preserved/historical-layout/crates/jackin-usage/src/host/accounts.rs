// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical provider-account identity used by discovery and broker routing.

use jackin_core::account_key_hash;

use super::HostSurfaceId;

/// Identity evidence used below the operator-visible account label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CanonicalAccountSubject {
    /// Provider-issued account or organization identifier, when available.
    ProviderId(String),
    /// Provider-authenticated stable non-secret handle when no stronger ID exists.
    ProviderStableHandle(String),
    /// Stable opaque source identity used when a provider exposes only a label.
    ///
    /// A display label is not unique enough to route two same-provider sources.
    /// The capability id keeps those sources separate without treating a path,
    /// ordinal, or secret as account identity.
    SourceCapability(String),
}

/// Exact provider/source identity. Probe-routing slugs and raw source paths are
/// excluded; source-scoped fallbacks use only an opaque capability handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalAccountIdentity {
    /// Exact provider ownership.
    pub surface: HostSurfaceId,
    /// Provider-owned or source-scoped subject.
    pub subject: CanonicalAccountSubject,
}

impl CanonicalAccountIdentity {
    pub(super) fn source_capability(surface: HostSurfaceId, capability_id: &str) -> Self {
        Self {
            surface,
            subject: CanonicalAccountSubject::SourceCapability(capability_id.to_owned()),
        }
    }

    pub(super) fn account_key(&self) -> String {
        let evidence = match &self.subject {
            CanonicalAccountSubject::ProviderId(id) => {
                format!("account-key-v1:provider-id:{}", id.trim())
            }
            CanonicalAccountSubject::ProviderStableHandle(handle) => format!(
                "account-key-v1:stable-handle:{}",
                normalize_stable_handle(handle)
            ),
            CanonicalAccountSubject::SourceCapability(capability_id) => format!(
                "account-key-v1:source-capability:{}:{}",
                capability_id.len(),
                capability_id
            ),
        };
        account_key_hash(self.surface.provider_id(), &evidence)
    }
}

fn normalize_stable_handle(handle: &str) -> String {
    handle.trim().to_lowercase()
}
