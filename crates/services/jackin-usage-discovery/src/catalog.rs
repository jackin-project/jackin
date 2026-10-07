// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Validated catalog types and accumulators.

use crate::{UsageDiscoveryDiagnostic, UsageSourceCandidateDescriptor};
use jackin_usage_host_accounts::{AccountMembershipDescriptor, CanonicalAccountIdentity};
use jackin_usage_host_credentials::{
    OpaqueCredentialHandle, ProviderCredentialSourceMaterial, UsageCredentialKind,
};
use std::collections::BTreeSet;

use std::path::PathBuf;

use jackin_core::Agent;
use jackin_usage_host_presentation::HostSurfaceId;

/// One complete source discovery generation.
#[derive(Clone, PartialEq, Eq)]
pub struct UsageDiscoveryCatalog {
    /// SHA-256 config generation; absent for capability-only Capsule discovery.
    pub config_generation: Option<String>,
    /// Deduplicated sanitized source descriptors.
    pub candidates: Vec<UsageSourceCandidateDescriptor>,
    /// Isolated config/credential diagnostics.
    pub diagnostics: Vec<UsageDiscoveryDiagnostic>,
    pub(crate) sources: Vec<DiscoveredCredentialSource>,
}

/// One post-auth canonical account discovered from current config membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAccountDescriptor {
    /// Exact provider surface.
    pub surface_id: String,
    /// Stable canonical account key.
    pub account_key: String,
    /// Authenticated provider label.
    pub account_label: String,
    /// Every effective config scope contributing this account.
    pub provenance: Vec<String>,
    /// Opaque source ids merged into this account.
    pub source_ids: Vec<String>,
    pub identity: CanonicalAccountIdentity,
}

impl AccountMembershipDescriptor for DiscoveredAccountDescriptor {
    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn account_key(&self) -> &str {
        &self.account_key
    }

    fn account_label(&self) -> &str {
        &self.account_label
    }

    fn provenance(&self) -> &[String] {
        &self.provenance
    }

    fn identity(&self) -> CanonicalAccountIdentity {
        self.identity.clone()
    }
}

/// Validated, post-auth discovery generation.
#[derive(Clone)]
pub struct ValidatedUsageDiscovery {
    /// Content-derived config generation.
    pub config_generation: Option<String>,
    /// Canonical current accounts only; anonymous/failed sources never become rows.
    pub accounts: Vec<DiscoveredAccountDescriptor>,
    /// Sanitized config and source failures.
    pub diagnostics: Vec<UsageDiscoveryDiagnostic>,
    /// Deduplicated source inventory used for refresh routing.
    pub candidates: Vec<UsageSourceCandidateDescriptor>,
    pub bindings: Vec<ValidatedCredentialBinding>,
}

impl ValidatedUsageDiscovery {
    pub fn unresolved_capabilities(&self) -> impl Iterator<Item = &UsageSourceCandidateDescriptor> {
        self.candidates.iter().filter(|candidate| {
            self.bindings.iter().any(|binding| {
                binding.capability_id == candidate.capability_id && binding.identity.is_none()
            })
        })
    }

    pub fn canonical_aliases(&self) -> impl Iterator<Item = (&str, &CanonicalAccountIdentity)> {
        self.bindings.iter().filter_map(|binding| {
            binding
                .identity
                .as_ref()
                .map(|identity| (binding.capability_id.as_str(), identity))
        })
    }
}

impl std::fmt::Debug for ValidatedUsageDiscovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedUsageDiscovery")
            .field("config_generation", &self.config_generation)
            .field("accounts", &self.accounts)
            .field("diagnostics", &self.diagnostics)
            .field("candidates", &self.candidates)
            .field("binding_count", &self.bindings.len())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedCredentialBinding {
    pub surface: HostSurfaceId,
    pub identity: Option<CanonicalAccountIdentity>,
    pub source_id: String,
    pub capability_id: String,
    pub credential_revision: String,
    pub provenance: BTreeSet<String>,
    pub source: ValidatedCredentialSource,
}

#[derive(Clone, Debug)]
pub enum ValidatedCredentialSource {
    Profile(ProfileCredentialMaterial),
    Env {
        handle: OpaqueCredentialHandle,
        /// Canonical provider usage identity used for account/capability
        /// attribution and source-material lookup.
        key: String,
        /// Exact governed route key used for provider refresh dispatch.
        dispatch_key: String,
        launch_keys: BTreeSet<String>,
        material: Option<ProviderCredentialSourceMaterial>,
    },
    Capability,
    /// Locally identified profile with no pollable usage fetch by design
    /// (Muse identity, omp/hermes attribution adapters). Refresh yields an
    /// honest `Unsupported` snapshot, never a probe failure — unlike
    /// [`Self::Capability`], which is a forwarded trust-domain token whose
    /// refresh attempt is genuinely malformed here.
    Unpollable,
}

#[derive(Clone)]
pub enum ProfileCredentialMaterial {
    Claude(jackin_usage_provider_claude::ClaudeResolved),

    Codex {
        credentials: jackin_usage_provider_codex::CodexOAuthCredentials,
        root: PathBuf,
    },
    Amp {
        key: String,
    },
    Grok {
        auth_path: PathBuf,
    },
    Kimi {
        token: String,
    },
    OpenCode {
        auth_path: PathBuf,
    },
    Cursor {
        auth_path: PathBuf,
    },
    Gemini {
        creds_path: PathBuf,
    },
    /// Antigravity CLI grant (Keychain singleton): presence-only, no secret
    /// material — refresh shells out to `agy`, which owns the grant.
    Antigravity,
}

impl std::fmt::Debug for ProfileCredentialMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude(_) => formatter.write_str("Claude(REDACTED)"),
            Self::Codex { root, .. } => formatter
                .debug_struct("Codex")
                .field("credentials", &"REDACTED")
                .field("root", root)
                .finish(),
            Self::Amp { .. } => formatter.write_str("Amp(REDACTED)"),
            Self::Grok { auth_path } => formatter
                .debug_struct("Grok")
                .field("auth_path", auth_path)
                .finish(),
            Self::Kimi { .. } => formatter.write_str("Kimi(REDACTED)"),
            Self::OpenCode { auth_path } => formatter
                .debug_struct("OpenCode")
                .field("auth_path", auth_path)
                .finish(),
            Self::Cursor { auth_path } => formatter
                .debug_struct("Cursor")
                .field("auth_path", auth_path)
                .finish(),
            Self::Gemini { creds_path } => formatter
                .debug_struct("Gemini")
                .field("creds_path", creds_path)
                .finish(),
            Self::Antigravity => formatter.write_str("Antigravity"),
        }
    }
}

impl std::fmt::Debug for UsageDiscoveryCatalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageDiscoveryCatalog")
            .field("config_generation", &self.config_generation)
            .field("candidates", &self.candidates)
            .field("diagnostics", &self.diagnostics)
            .field("source_count", &self.sources.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CredentialSourceKey {
    Profile {
        agent: Agent,
        root: PathBuf,
    },
    Env {
        surface: HostSurfaceId,
        handle: OpaqueCredentialHandle,
        /// Canonical provider usage key, never a launch alias.
        key: String,
        /// Exact governed route key whose provider semantics must be kept.
        dispatch_key: String,
    },
    Capability {
        surface: HostSurfaceId,
        id: String,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum DiscoveredCredentialSource {
    Profile {
        surface: HostSurfaceId,
        agent: Agent,
        root: PathBuf,
        operator_home: PathBuf,
        account_label: Option<String>,
        source_id: String,
        capability_id: String,
        provenance: BTreeSet<String>,
    },
    Env {
        surface: HostSurfaceId,
        handle: OpaqueCredentialHandle,
        key: String,
        dispatch_key: String,
        launch_keys: BTreeSet<String>,
        kind: UsageCredentialKind,
        account_label: Option<String>,
        source_id: String,
        capability_id: String,
        provenance: BTreeSet<String>,
    },
    Capability {
        surface: HostSurfaceId,
        account_label: Option<String>,
        source_id: String,
        capability_id: String,
    },
}

pub(crate) struct CandidateAccumulator {
    pub(crate) surface: HostSurfaceId,
    pub(crate) kind: UsageCredentialKind,
    pub(crate) provenance: BTreeSet<String>,
    pub(crate) env_keys: BTreeSet<String>,
    pub(crate) account_label: Option<String>,
    pub(crate) operator_home: Option<PathBuf>,
}

pub(crate) fn merge_env_candidate(
    candidate: &mut CandidateAccumulator,
    provenance: &BTreeSet<String>,
    env_key: &str,
    account_label: Option<&str>,
) {
    candidate.provenance.extend(provenance.clone());
    candidate.env_keys.insert(env_key.to_owned());
    if candidate.account_label.is_none() {
        candidate.account_label = account_label.map(str::to_owned);
    }
}
