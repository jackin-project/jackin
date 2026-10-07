//! jackin-usage-discovery: host account-source discovery and validation.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`discover_usage_sources`] — scan authorized sources.
//!
//! Rust-owned host account-source discovery: account-registry authority,
//! path roots, provider ownership, deduplication, validation, and the
//! binding-to-capability mapping the host broker serves.

mod accumulate;
mod capabilities;
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
mod validate;

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use jackin_config::{AccountCredential, AiProvider, AppConfig};
#[cfg(test)]
use jackin_core::{Agent, UsageCredentialEnvName, UsageCredentialOwner, WorkspaceName};
#[cfg(test)]
use jackin_usage_host_accounts::CanonicalAccountSubject;
#[cfg(test)]
use jackin_usage_host_presentation::HostSurfaceId;

#[cfg(test)]
pub(crate) use jackin_usage_host_credentials::{
    ForwardedUsageAccount, OpaqueCredentialHandle, ProviderCredentialEnvOutcome,
    ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
    ProviderCredentialRefreshOutcome, UsageCredentialKind, governed_name_for_account_alias,
};

pub(crate) use accumulate::{ValidatedSourceParts, accumulate_validated_source};
pub use capabilities::{capability_for_binding, usage_broker_capabilities, usage_catalog_entries};
pub(crate) use catalog::DiscoveredCredentialSource;
pub(crate) use catalog::{CandidateAccumulator, CredentialSourceKey, merge_env_candidate};
pub use catalog::{DiscoveredAccountDescriptor, UsageDiscoveryCatalog, ValidatedUsageDiscovery};
pub use catalog::{
    ProfileCredentialMaterial, ValidatedCredentialBinding, ValidatedCredentialSource,
};
pub(crate) use identity::{
    opaque_credential_revision, profile_credential_revision, profile_identity,
};
pub use issues::{UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageSourceCandidateDescriptor};
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
pub use refresh::refresh_credential_binding;
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
