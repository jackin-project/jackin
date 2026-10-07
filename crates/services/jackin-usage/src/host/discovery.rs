// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Account-registry authority, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use jackin_config::{AccountCredential, AiProvider, AppConfig};
#[cfg(test)]
use jackin_core::{Agent, UsageCredentialEnvName, UsageCredentialOwner, WorkspaceName};
#[cfg(test)]
use jackin_protocol::control::FocusedUsageView;

#[cfg(test)]
use super::{CanonicalAccountSubject, HostSurfaceId, HostUsageRuntime};

mod accumulate;
mod catalog;
mod identity;
mod issues;
mod ownership;
mod profiles;
mod providers;
mod refresh;
mod scan;
mod scope;
mod source;
mod stage;
mod validate;

pub(crate) use accumulate::{ValidatedSourceParts, accumulate_validated_source};
pub(crate) use catalog::{CandidateAccumulator, CredentialSourceKey, merge_env_candidate};
pub use catalog::{DiscoveredAccountDescriptor, UsageDiscoveryCatalog, ValidatedUsageDiscovery};
pub(super) use catalog::{
    DiscoveredCredentialSource, ProfileCredentialMaterial, ValidatedCredentialBinding,
    ValidatedCredentialSource,
};
pub(crate) use identity::{
    opaque_credential_revision, profile_credential_revision, profile_identity,
};
pub use issues::{UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageSourceCandidateDescriptor};
pub(super) use jackin_usage_host_credentials::governed_name_for_account_alias;
pub use jackin_usage_host_credentials::{
    ForwardedUsageAccount, OpaqueCredentialHandle, ProviderCredentialEnvOutcome,
    ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
    ProviderCredentialIdentityOutcome, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial, UsageCredentialKind,
};
#[cfg(test)]
pub(crate) use ownership::source_capability_id;
pub(crate) use ownership::{
    account_diagnostic, canonical_owner_for_account, canonical_usage_env_name, config_diagnostics,
    materialize_catalog, provider_surface, resolve_profile_root,
};
pub(crate) use profiles::{
    AccountAccumulator, CachingProfileCredentialReader, ProfileCredentialReader,
    ProfileReadOutcome, ProfileValidation, SystemProfileCredentialReader,
};
pub(crate) use providers::{
    amp_profile_identity, antigravity_profile_identity, claude_profile_identity,
    codex_profile_identity, cursor_profile_identity, gemini_profile_identity,
    grok_profile_identity, muse_profile_identity, opencode_profile_identity,
};
pub(super) use refresh::refresh_credential_binding;
pub(crate) use refresh::{first_recursive_string, read_json};
pub use scan::discover_usage_sources;
#[cfg(test)]
pub(crate) use scan::usage_account_alias_entry;
pub use scope::{HostCredentialRootRow, UsageDiscoveryScope, host_credential_root_matrix};
pub(crate) use source::{source_diagnostic, validate_source};
pub use validate::validate_usage_sources;
#[cfg(test)]
pub(crate) use validate::validate_usage_sources_with_reader;

#[cfg(test)]
mod tests;
