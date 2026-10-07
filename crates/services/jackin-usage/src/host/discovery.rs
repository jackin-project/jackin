// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Account-registry authority, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

mod stage;

#[cfg(test)]
pub(crate) use jackin_usage_discovery::{ProfileCredentialMaterial, UsageDiscoveryDiagnostic};
pub(crate) use jackin_usage_discovery::{
    UsageDiscoveryIssue, ValidatedCredentialBinding, ValidatedCredentialSource,
    discover_usage_sources, refresh_credential_binding, validate_usage_sources,
};
pub(crate) use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial,
};
