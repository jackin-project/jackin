// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Account-registry authority, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

mod profile_validation;

pub(super) use profile_validation::refresh_credential_binding;
pub use profile_validation::validate_usage_sources;
#[cfg(test)]
use profile_validation::{
    ProfileCredentialReader, ProfileReadOutcome, ProfileValidation, opencode_profile_identity,
    profile_identity, validate_source, validate_usage_sources_with_reader,
};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use jackin_config::{
    AccountCredential, AiProvider, AppConfig, ConfigSourceIssue, ReadOnlyConfigSnapshot,
};
use jackin_core::{
    Agent, AuthForwardMode, JackinPaths, UsageCredentialEnvName, UsageCredentialOwner,
    WorkspaceName,
};
use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::UsageCredentialSourceIdentity;

use super::{
    CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId, HostUsageRuntime,
    StagedUsageDiscovery, discovered_account_keys,
};

/// Discovery boundary: Desktop may scan host config; Capsule sees capabilities only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageDiscoveryScope {
    /// Host-wide Desktop inventory rooted at explicit operator paths.
    HostDesktop {
        /// Directory containing `config.toml` and `workspaces/`.
        config_root: PathBuf,
        /// Operator home used for default and tilde-relative credential roots.
        operator_home: PathBuf,
    },
    /// Container inventory restricted to explicitly forwarded accounts.
    Capsule {
        /// Broker/runtime-issued capabilities available inside this Capsule.
        forwarded_accounts: Vec<ForwardedUsageAccount>,
    },
}

/// Credential-root inventory for docs and debug (no secrets read).
#[must_use]
pub fn host_credential_root_matrix() -> Vec<HostCredentialRootRow> {
    use jackin_core::container_paths;
    vec![
        HostCredentialRootRow {
            surface: "claude",
            host_paths: "~/.claude/.credentials.json, ~/.claude.json, $CLAUDE_CONFIG_DIR",
            env_vars: "ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN",
            container_handoff: container_paths::CLAUDE_CREDENTIALS,
        },
        HostCredentialRootRow {
            surface: "codex",
            host_paths: "$CODEX_HOME/auth.json, ~/.codex/auth.json",
            env_vars: "",
            container_handoff: container_paths::CODEX_AUTH,
        },
        HostCredentialRootRow {
            surface: "amp",
            host_paths: "Amp home secrets loaders",
            env_vars: "",
            container_handoff: container_paths::AMP_SECRETS,
        },
        HostCredentialRootRow {
            surface: "grok",
            host_paths: "~/.grok (auth + bin)",
            env_vars: "",
            container_handoff: container_paths::GROK_AUTH,
        },
        HostCredentialRootRow {
            surface: "kimi",
            host_paths: "~/.kimi-code, ~/.kimi",
            env_vars: "KIMI_AUTH_TOKEN, KIMI_CODE_API_KEY, kimi_auth_token",
            container_handoff: container_paths::KIMI_CODE_DIR,
        },
        HostCredentialRootRow {
            surface: "opencode",
            host_paths: "$XDG_DATA_HOME/opencode/auth.json or ~/.local/share/opencode/auth.json",
            env_vars: "",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "zai",
            host_paths: "",
            env_vars: "ZAI_API_KEY, ZHIPU_API_KEY, Z_AI_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "minimax",
            host_paths: "",
            env_vars: "MINIMAX_CODING_API_KEY, MINIMAX_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "google",
            host_paths: "~/.gemini/antigravity-cli, ~/.gemini, $GEMINI_CLI_HOME",
            env_vars: "GEMINI_API_KEY, GOOGLE_API_KEY",
            container_handoff: container_paths::GEMINI_AUTH,
        },
        HostCredentialRootRow {
            surface: "cursor",
            host_paths: "~/.cursor, $CURSOR_CONFIG_DIR",
            env_vars: "CURSOR_API_KEY",
            container_handoff: container_paths::CURSOR_AUTH,
        },
        HostCredentialRootRow {
            surface: "meta",
            host_paths: "~/.config/muse",
            env_vars: "META_API_KEY",
            container_handoff: container_paths::MUSE_AUTH,
        },
        HostCredentialRootRow {
            surface: "openrouter",
            host_paths: "",
            env_vars: "OPENROUTER_API_KEY",
            container_handoff: "",
        },
    ]
}

/// One row of the host credential matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCredentialRootRow {
    /// Surface id.
    pub surface: &'static str,
    /// Host path roots.
    pub host_paths: &'static str,
    /// Environment variables.
    pub env_vars: &'static str,
    /// Container handoff fallback.
    pub container_handoff: &'static str,
}

/// One account capability explicitly forwarded into a Capsule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedUsageAccount {
    /// Stable provider surface id.
    pub surface_id: String,
    /// Opaque broker-issued capability id; never credential material.
    pub capability_id: String,
    /// Authenticated display label when already known.
    pub account_label: Option<String>,
}

/// Opaque process-local handle for a resolved environment credential.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueCredentialHandle(String);

impl OpaqueCredentialHandle {
    /// Construct from a non-secret adapter-owned identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Debug for OpaqueCredentialHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpaqueCredentialHandle(REDACTED)")
    }
}

/// Secret-free outcome for one governed provider environment key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialEnvOutcome {
    /// Protected value resolved and is retained behind this adapter handle.
    Resolved(OpaqueCredentialHandle),
    /// An explicitly required host value is missing.
    Missing,
    /// Protected source denied access or was unavailable.
    Denied,
    /// Configured source was structurally malformed.
    Malformed,
    /// Source requires an explicit operator action before retry.
    InteractionRequired,
}

/// One governed provider key result from a tier-4 env adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialEnvResolution {
    /// Exact requested key name.
    pub key: String,
    /// Secret-free outcome.
    pub outcome: ProviderCredentialEnvOutcome,
}

/// Secret-free source identity and material fingerprint retained by one
/// validated discovery binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialSourceMaterial {
    /// Exact declaration identity observed by the resolver.
    pub source: UsageCredentialSourceIdentity,
    /// Fingerprint of the resolved material held behind the opaque handle.
    pub material_fingerprint: String,
}

/// Port from usage discovery to tier-4 env/1Password composition.
pub trait ProviderCredentialEnvResolver: Send + Sync {
    /// Begin one explicit manual retry action.
    ///
    /// Adapters may evict only prior non-success outcomes here. Background
    /// refresh never calls this method.
    fn begin_manual_retry(&self) {}

    /// Resolve only `keys` for the exact effective config scope.
    ///
    /// Absent declarations are omitted. Implementations retain resolved secret
    /// values internally and return opaque handles only.
    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution>;

    /// Resolve authenticated identity for an already resolved opaque handle.
    ///
    /// The default is anonymous because most API-key providers reveal identity
    /// only in the quota response. Implementations must never expose the key.
    fn identify_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialIdentityOutcome {
        ProviderCredentialIdentityOutcome::Anonymous
    }

    /// Probe quota for one already-resolved credential without exposing it.
    ///
    /// The adapter owns secret access; provider snapshot construction remains
    /// in `jackin-usage`. Background refresh may call this only for successful
    /// handles retained by the completed discovery generation.
    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        ProviderCredentialRefreshOutcome::Malformed
    }

    /// Return the exact non-secret source material bound to one opaque handle.
    /// Implementations that cannot prove this must return `None`; the launch
    /// boundary then rejects scoped environment credentials.
    fn source_material(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> Option<ProviderCredentialSourceMaterial> {
        None
    }
}

/// Authenticated identity result for an opaque provider credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialIdentityOutcome {
    /// Provider authenticated the source. Stable id wins over label for dedup.
    Authenticated {
        /// Provider-issued stable account/organization id when available.
        provider_id: Option<String>,
        /// Authenticated user-facing account label when available.
        account_label: Option<String>,
    },
    /// Source is usable but identity is unavailable until a quota request.
    Anonymous,
    /// Credential disappeared after enumeration.
    Missing,
    /// Protected source access was denied.
    Denied,
    /// Credential payload is malformed.
    Malformed,
}

/// Secret-free quota result returned by a protected-source adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderCredentialRefreshOutcome {
    /// Provider snapshot, including authenticated identity when supplied.
    Snapshot {
        /// Provider view with secret-free account/quota data.
        view: Box<FocusedUsageView>,
        /// Typed provider rate-limit metadata, when the provider returned HTTP 429.
        rate_limit: Option<crate::usage::ProviderRateLimit>,
        /// Typed provider failure metadata, independent of display text.
        failure_metadata: Option<crate::usage::ProviderFailureMetadata>,
    },
    /// Credential disappeared after discovery.
    Missing,
    /// Protected credential access is no longer authorized.
    Denied,
    /// Credential or provider response was malformed.
    Malformed,
    /// Source requires an explicit operator action before retry.
    InteractionRequired,
}

/// Credential form discovered before authenticated identity resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageCredentialKind {
    /// Agent-owned credential/config profile root.
    Profile,
    /// Provider API key.
    ApiKey,
    /// Provider OAuth token supplied through env.
    OAuthToken,
    /// Broker-issued Capsule capability.
    ForwardedCapability,
}

/// Sanitized source-level failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDiscoveryIssue {
    /// Config source was unreadable.
    ConfigUnreadable,
    /// Config source was malformed or invalid.
    ConfigInvalid,
    /// Config schema is newer than supported.
    ConfigVersionUnsupported,
    /// Config changed repeatedly during discovery.
    ConfigTransientConflict,
    /// Required credential source is absent.
    CredentialMissing,
    /// Protected credential access was denied/unavailable.
    CredentialDenied,
    /// A Keychain item exists but the operator has not approved access.
    KeychainConsentRequired,
    /// Credential source is malformed.
    CredentialMalformed,
    /// Credential source requires explicit interaction.
    InteractionRequired,
}

impl UsageDiscoveryIssue {
    /// Stable machine-readable identifier exported through sanitized adapters.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "config_unreadable",
            Self::ConfigInvalid => "config_invalid",
            Self::ConfigVersionUnsupported => "config_version_unsupported",
            Self::ConfigTransientConflict => "config_transient_conflict",
            Self::CredentialMissing => "credential_missing",
            Self::CredentialDenied => "credential_denied",
            Self::KeychainConsentRequired => "keychain_consent_required",
            Self::CredentialMalformed => "credential_malformed",
            Self::InteractionRequired => "interaction_required",
        }
    }

    /// Rust-owned operator copy. It deliberately contains no source location.
    #[must_use]
    pub const fn display_message(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "Configuration could not be read",
            Self::ConfigInvalid => "Configuration is invalid",
            Self::ConfigVersionUnsupported => "Configuration version is not supported",
            Self::ConfigTransientConflict => "Configuration changed while it was being read",
            Self::CredentialMissing => "Credentials are missing",
            Self::CredentialDenied => "Credential access was denied",
            Self::KeychainConsentRequired => {
                "Keychain consent required; approve jackin in Keychain Access"
            }
            Self::CredentialMalformed => "Credentials are malformed",
            Self::InteractionRequired => "Credential access requires interaction",
        }
    }
}

/// Sanitized provider/scope diagnostic. No path, secret, or 1Password coordinate.
#[derive(Clone, PartialEq, Eq)]
pub struct UsageDiscoveryDiagnostic {
    /// Provider surface when the failure is provider-specific.
    pub surface_id: Option<String>,
    /// Rust-composed scope label (`account …`, `workspace …`).
    pub scope_label: String,
    /// Opaque identity of the configured source that failed validation.
    pub unresolved_source: Option<UsageDiscoveryUnresolvedSource>,
    /// Stable machine-readable category.
    pub issue: UsageDiscoveryIssue,
}

/// Secret-free identity and observation count for one unresolved source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDiscoveryUnresolvedSource {
    /// Stable opaque source identifier, never a path, alias, or secret.
    pub capability_id: String,
    /// Number of current config observations contributing this source.
    pub configuration_count: u32,
}

impl std::fmt::Debug for UsageDiscoveryDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageDiscoveryDiagnostic")
            .field("surface_id", &self.surface_id)
            .field("scope_label", &"[REDACTED]")
            .field("unresolved_source", &self.unresolved_source)
            .field("issue", &self.issue)
            .finish()
    }
}

/// Sanitized candidate source descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSourceCandidateDescriptor {
    /// Provider surface id.
    pub surface_id: String,
    /// Credential form.
    pub credential_kind: UsageCredentialKind,
    /// Opaque process-local source identifier.
    pub source_id: String,
    /// Stable opaque capability identity; never a source ordinal or credential hash.
    pub capability_id: String,
    /// Every config scope that resolved to this source.
    pub provenance: Vec<String>,
}

/// One complete source discovery generation.
#[derive(Clone, PartialEq, Eq)]
pub struct UsageDiscoveryCatalog {
    /// SHA-256 config generation; absent for capability-only Capsule discovery.
    pub config_generation: Option<String>,
    /// Deduplicated sanitized source descriptors.
    pub candidates: Vec<UsageSourceCandidateDescriptor>,
    /// Isolated config/credential diagnostics.
    pub diagnostics: Vec<UsageDiscoveryDiagnostic>,
    pub(super) sources: Vec<DiscoveredCredentialSource>,
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
    pub(super) identity: CanonicalAccountIdentity,
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
    pub(super) bindings: Vec<ValidatedCredentialBinding>,
}

impl ValidatedUsageDiscovery {
    pub(super) fn unresolved_capabilities(
        &self,
    ) -> impl Iterator<Item = &UsageSourceCandidateDescriptor> {
        self.candidates.iter().filter(|candidate| {
            self.bindings.iter().any(|binding| {
                binding.capability_id == candidate.capability_id && binding.identity.is_none()
            })
        })
    }

    pub(super) fn canonical_aliases(
        &self,
    ) -> impl Iterator<Item = (&str, &CanonicalAccountIdentity)> {
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

#[derive(Clone)]
pub(super) struct ValidatedCredentialBinding {
    pub surface: HostSurfaceId,
    pub identity: Option<CanonicalAccountIdentity>,
    pub source_id: String,
    pub capability_id: String,
    pub credential_revision: String,
    pub provenance: BTreeSet<String>,
    pub source: ValidatedCredentialSource,
}

#[derive(Clone)]
pub(super) enum ValidatedCredentialSource {
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
pub(super) enum ProfileCredentialMaterial {
    Claude(crate::usage::ClaudeResolved),
    Codex {
        credentials: crate::usage::CodexOAuthCredentials,
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
enum CredentialSourceKey {
    Profile {
        agent: Agent,
        root: PathBuf,
        /// Exact selected Keychain source used as the Claude local identity.
        claude_service: Option<String>,
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
pub(super) enum DiscoveredCredentialSource {
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

struct CandidateAccumulator {
    surface: HostSurfaceId,
    kind: UsageCredentialKind,
    provenance: BTreeSet<String>,
    env_keys: BTreeSet<String>,
    account_label: Option<String>,
    operator_home: Option<PathBuf>,
}

fn merge_env_candidate(
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

/// Discover and pre-deduplicate every source authorized by `scope`.
pub fn discover_usage_sources(
    scope: &UsageDiscoveryScope,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> Result<UsageDiscoveryCatalog, String> {
    match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => discover_host_sources(config_root, operator_home, env_resolver),
        UsageDiscoveryScope::Capsule { forwarded_accounts } => {
            Ok(discover_forwarded_sources(forwarded_accounts))
        }
    }
}

fn discover_host_sources(
    config_root: &Path,
    operator_home: &Path,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> Result<UsageDiscoveryCatalog, String> {
    let paths = JackinPaths::resolve_with_env(operator_home, None, Some(config_root.as_os_str()));
    let snapshot = jackin_config::load_read_only_config_snapshot(&paths)
        .map_err(|_| "config snapshot unavailable".to_owned())?;
    let mut diagnostics = config_diagnostics(&snapshot);
    let mut candidates = BTreeMap::<CredentialSourceKey, CandidateAccumulator>::new();
    enumerate_registered_accounts(
        &snapshot.config,
        operator_home,
        env_resolver,
        &mut candidates,
        &mut diagnostics,
    );

    Ok(materialize_catalog(
        Some(snapshot.generation.as_str().to_owned()),
        candidates,
        diagnostics,
    ))
}

fn discover_forwarded_sources(accounts: &[ForwardedUsageAccount]) -> UsageDiscoveryCatalog {
    let mut candidates = BTreeMap::<CredentialSourceKey, CandidateAccumulator>::new();
    for account in accounts {
        let Some(surface) = HostSurfaceId::from_id(&account.surface_id) else {
            continue;
        };
        // Every known surface reaches Capsules: `DESKTOP_PROVIDER_ORDER` is
        // the Swift glance contract only, not forwarded admission.
        if !HostSurfaceId::ALL.contains(&surface) {
            continue;
        }
        candidates
            .entry(CredentialSourceKey::Capability {
                surface,
                id: account.capability_id.clone(),
            })
            .or_insert_with(|| CandidateAccumulator {
                surface,
                kind: UsageCredentialKind::ForwardedCapability,
                provenance: BTreeSet::from(["forwarded to Capsule".to_owned()]),
                env_keys: BTreeSet::new(),
                account_label: account.account_label.clone(),
                operator_home: None,
            });
    }
    materialize_catalog(None, candidates, Vec::new())
}

/// Discovery-isolated alias for one governed registry entry.
///
/// Operator-env attribution retains out every account-governed name, so an
/// account credential presented to a CLI-side secret source under its
/// governed key resolves to `Missing` while broker-side sources (which read
/// `config.env` directly) resolve it. Presenting the one isolated
/// declaration under a non-governed alias keeps both resolvers on the same
/// declaration; the governed name is still recorded on the discovered source
/// for refresh routing and forwarding.
fn usage_account_alias_entry(
    entry: UsageCredentialEnvName,
    canonical_owner: UsageCredentialOwner,
) -> UsageCredentialEnvName {
    let name = match entry.name {
        jackin_core::ANTHROPIC_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ANTHROPIC_API_KEY",
        jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN",
        jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME => {
            "JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN"
        }
        jackin_core::OPENAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY",
        jackin_core::AMP_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_AMP_API_KEY",
        jackin_core::KIMI_CODE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_KIMI_CODE_API_KEY",
        jackin_core::KIMI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_KIMI_API_KEY",
        jackin_core::MOONSHOT_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_MOONSHOT_API_KEY",
        jackin_core::XAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_XAI_API_KEY",
        jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GROK_DEPLOYMENT_KEY",
        jackin_core::ZAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ZAI_API_KEY",
        jackin_core::ZHIPU_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY",
        "Z_AI_API_KEY" => "JACKIN_USAGE_ACCOUNT_Z_AI_API_KEY",
        jackin_core::MINIMAX_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_MINIMAX_API_KEY",
        jackin_core::OPENCODE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENCODE_API_KEY",
        jackin_core::GEMINI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GEMINI_API_KEY",
        jackin_core::GOOGLE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GOOGLE_API_KEY",
        jackin_core::CURSOR_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_CURSOR_API_KEY",
        jackin_core::META_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_META_API_KEY",
        jackin_core::OPENROUTER_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY",
        _ => return entry,
    };
    UsageCredentialEnvName {
        name,
        owner: canonical_owner,
    }
}

/// Recover the governed registry name for one discovery-isolated alias.
///
/// Unknown names pass through unchanged so direct governed-name callers keep
/// their existing cache identity.
pub(super) fn governed_name_for_account_alias(name: &str) -> &str {
    match name {
        "JACKIN_USAGE_ACCOUNT_ANTHROPIC_API_KEY" => jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN" => jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN" => {
            jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
        }
        "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY" => jackin_core::OPENAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_AMP_API_KEY" => jackin_core::AMP_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_KIMI_CODE_API_KEY" => jackin_core::KIMI_CODE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_KIMI_API_KEY" => jackin_core::KIMI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_MOONSHOT_API_KEY" => jackin_core::MOONSHOT_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_XAI_API_KEY" => jackin_core::XAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GROK_DEPLOYMENT_KEY" => jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ZAI_API_KEY" => jackin_core::ZAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY" => jackin_core::ZHIPU_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_Z_AI_API_KEY" => "Z_AI_API_KEY",
        "JACKIN_USAGE_ACCOUNT_MINIMAX_API_KEY" => jackin_core::MINIMAX_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_OPENCODE_API_KEY" => jackin_core::OPENCODE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GEMINI_API_KEY" => jackin_core::GEMINI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GOOGLE_API_KEY" => jackin_core::GOOGLE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_CURSOR_API_KEY" => jackin_core::CURSOR_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_META_API_KEY" => jackin_core::META_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY" => jackin_core::OPENROUTER_API_KEY_ENV_NAME,
        _ => name,
    }
}

/// Registry entries are the sole discovery authority. Workspace references add
/// provenance; they never cause an ambient profile or environment scan.
fn enumerate_registered_accounts(
    config: &AppConfig,
    operator_home: &Path,
    resolver: &dyn ProviderCredentialEnvResolver,
    candidates: &mut BTreeMap<CredentialSourceKey, CandidateAccumulator>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
) {
    for (id, account) in &config.accounts {
        if !account.enabled {
            continue;
        }
        let surface = provider_surface(account.provider);
        let canonical_owner = canonical_owner_for_account(surface, account.provider);
        let mut provenance = BTreeSet::from([format!("account {id}")]);
        for (workspace_name, workspace) in &config.workspaces {
            if workspace.accounts.contains(id) {
                provenance.insert(format!("workspace {workspace_name}"));
            }
        }
        let label = if !account.name.trim().is_empty() {
            Some(account.name.trim().to_owned())
        } else if !id.trim().is_empty() {
            Some(id.trim().to_owned())
        } else {
            None
        };

        // Profile discovery remains the baseline path. It does not need an
        // env route and must not invoke the protected env resolver.
        if let AccountCredential::Profile {
            agent, directory, ..
        } = &account.credential
        {
            let root = resolve_profile_root(operator_home, directory);
            let claude_service = if *agent == Agent::Claude {
                jackin_core::claude_keychain_scope(&root, operator_home, operator_home)
                    .map(|scope| scope.service)
            } else {
                None
            };
            candidates
                .entry(CredentialSourceKey::Profile {
                    agent: *agent,
                    root,
                    claude_service,
                })
                .and_modify(|candidate| {
                    candidate.provenance.extend(provenance.clone());
                    if candidate.account_label.is_none() {
                        candidate.account_label = label.clone();
                    }
                })
                .or_insert_with(|| CandidateAccumulator {
                    surface,
                    kind: UsageCredentialKind::Profile,
                    provenance,
                    env_keys: BTreeSet::new(),
                    account_label: label.clone(),
                    operator_home: Some(operator_home.to_path_buf()),
                });
            continue;
        }

        let (value, kind, expected_mode) = match &account.credential {
            AccountCredential::ApiKey { value, .. } => {
                (value, UsageCredentialKind::ApiKey, AuthForwardMode::ApiKey)
            }
            AccountCredential::OAuthToken { value, .. } => (
                value,
                UsageCredentialKind::OAuthToken,
                AuthForwardMode::OAuthToken,
            ),
            AccountCredential::Profile { .. } => continue,
        };
        let Ok(routes) = config.credential_descriptors_for_account(id) else {
            diagnostics.push(account_diagnostic(
                surface,
                id,
                None,
                1,
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        };
        for route in routes {
            if route.mode != expected_mode {
                diagnostics.push(account_diagnostic(
                    surface,
                    id,
                    Some(route.env_name),
                    1,
                    UsageDiscoveryIssue::CredentialMalformed,
                ));
                continue;
            }
            let Some(entry) = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
                .iter()
                .copied()
                .find(|entry| entry.name == route.env_name)
            else {
                diagnostics.push(account_diagnostic(
                    surface,
                    id,
                    Some(route.env_name),
                    1,
                    UsageDiscoveryIssue::CredentialMalformed,
                ));
                continue;
            };
            // Resolve the account's exact declaration through one
            // discovery-only alias. Operator-env attribution strips governed
            // names, so putting the governed key directly in this isolated
            // config would incorrectly report a valid account as missing.
            let mut isolated = AppConfig {
                env: config.env.clone(),
                ..AppConfig::default()
            };
            let alias = usage_account_alias_entry(entry, canonical_owner);
            isolated.env.insert(alias.name.to_owned(), value.clone());
            let resolutions =
                resolver.resolve_provider_credentials(&isolated, None, None, &[alias]);
            let outcome = resolutions
                .into_iter()
                .find(|result| result.key == alias.name)
                .map_or(ProviderCredentialEnvOutcome::Missing, |result| {
                    result.outcome
                });
            let issue = match outcome {
                ProviderCredentialEnvOutcome::Resolved(handle) => {
                    candidates
                        .entry(CredentialSourceKey::Env {
                            surface,
                            handle,
                            key: canonical_usage_env_name(surface).to_owned(),
                            dispatch_key: super::credential_resolver::dispatch_key_for_route(
                                canonical_owner,
                                governed_name_for_account_alias(entry.name),
                            )
                            .to_owned(),
                        })
                        .and_modify(|candidate| {
                            merge_env_candidate(
                                candidate,
                                &provenance,
                                entry.name,
                                label.as_deref(),
                            );
                        })
                        .or_insert_with(|| CandidateAccumulator {
                            surface,
                            kind,
                            provenance: provenance.clone(),
                            env_keys: BTreeSet::from([entry.name.to_owned()]),
                            account_label: label.clone(),
                            operator_home: None,
                        });
                    continue;
                }
                ProviderCredentialEnvOutcome::Missing => UsageDiscoveryIssue::CredentialMissing,
                ProviderCredentialEnvOutcome::Denied => UsageDiscoveryIssue::CredentialDenied,
                ProviderCredentialEnvOutcome::Malformed => UsageDiscoveryIssue::CredentialMalformed,
                ProviderCredentialEnvOutcome::InteractionRequired => {
                    UsageDiscoveryIssue::InteractionRequired
                }
            };
            diagnostics.push(account_diagnostic(surface, id, Some(entry.name), 1, issue));
        }
    }
}

fn provider_surface(provider: AiProvider) -> HostSurfaceId {
    match provider {
        AiProvider::Anthropic => HostSurfaceId::Claude,
        AiProvider::OpenAi => HostSurfaceId::Codex,
        AiProvider::Amp => HostSurfaceId::Amp,
        AiProvider::Xai => HostSurfaceId::Grok,
        AiProvider::Opencode => HostSurfaceId::OpenCode,
        AiProvider::Moonshot => HostSurfaceId::Kimi,
        AiProvider::Zai => HostSurfaceId::Zai,
        AiProvider::Minimax => HostSurfaceId::Minimax,
        AiProvider::Google => HostSurfaceId::Google,
        AiProvider::Cursor => HostSurfaceId::Cursor,
        AiProvider::Meta => HostSurfaceId::Meta,
        AiProvider::OpenRouter => HostSurfaceId::OpenRouter,
    }
}

fn canonical_owner_for_account(
    surface: HostSurfaceId,
    provider: AiProvider,
) -> UsageCredentialOwner {
    let owner = match provider {
        AiProvider::Anthropic => UsageCredentialOwner::Claude,
        AiProvider::OpenAi => UsageCredentialOwner::Codex,
        AiProvider::Amp => UsageCredentialOwner::Amp,
        AiProvider::Moonshot => UsageCredentialOwner::Kimi,
        AiProvider::Xai => UsageCredentialOwner::Grok,
        AiProvider::Zai => UsageCredentialOwner::Zai,
        AiProvider::Minimax => UsageCredentialOwner::Minimax,
        AiProvider::Opencode => UsageCredentialOwner::OpenCode,
        AiProvider::Google => UsageCredentialOwner::Google,
        AiProvider::Cursor => UsageCredentialOwner::Cursor,
        AiProvider::Meta => UsageCredentialOwner::Meta,
        AiProvider::OpenRouter => UsageCredentialOwner::OpenRouter,
    };
    debug_assert_eq!(provider_surface(provider), surface);
    owner
}

/// Canonical provider usage key. Launch routes may use provider-compatible
/// aliases, but cache/refresh/capability identity always uses this key.
fn canonical_usage_env_name(surface: HostSurfaceId) -> &'static str {
    match surface {
        HostSurfaceId::Claude => jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        HostSurfaceId::Codex => jackin_core::OPENAI_API_KEY_ENV_NAME,
        HostSurfaceId::Amp => jackin_core::AMP_API_KEY_ENV_NAME,
        HostSurfaceId::Kimi => jackin_core::KIMI_CODE_API_KEY_ENV_NAME,
        HostSurfaceId::Grok => jackin_core::XAI_API_KEY_ENV_NAME,
        HostSurfaceId::Zai => jackin_core::ZAI_API_KEY_ENV_NAME,
        HostSurfaceId::Minimax => jackin_core::MINIMAX_API_KEY_ENV_NAME,
        HostSurfaceId::OpenCode => jackin_core::OPENCODE_API_KEY_ENV_NAME,
        HostSurfaceId::Google => jackin_core::GEMINI_API_KEY_ENV_NAME,
        HostSurfaceId::Cursor => jackin_core::CURSOR_API_KEY_ENV_NAME,
        HostSurfaceId::Meta => jackin_core::META_API_KEY_ENV_NAME,
        HostSurfaceId::OpenRouter => jackin_core::OPENROUTER_API_KEY_ENV_NAME,
    }
}

fn resolve_profile_root(operator_home: &Path, configured: &Path) -> PathBuf {
    let mut components = configured.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => operator_home.join(components),
        Some(_) if configured.is_absolute() => configured.to_path_buf(),
        _ => operator_home.join(configured),
    }
}

fn account_diagnostic(
    surface: HostSurfaceId,
    account_id: &str,
    source_key: Option<&str>,
    configuration_count: u32,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    let material = format!(
        "surface:{}:account:{}:source:{}",
        surface.id(),
        account_id,
        source_key.unwrap_or("account-credential")
    );
    let capability_id =
        jackin_core::account_key_hash("usage-discovery-unresolved-source-v1", &material);
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: "account".to_owned(),
        unresolved_source: Some(UsageDiscoveryUnresolvedSource {
            capability_id: capability_id
                .strip_prefix("sha256:")
                .unwrap_or(&capability_id)
                .to_owned(),
            configuration_count,
        }),
        issue,
    }
}

fn config_diagnostics(snapshot: &ReadOnlyConfigSnapshot) -> Vec<UsageDiscoveryDiagnostic> {
    snapshot
        .diagnostics
        .iter()
        .map(|diagnostic| UsageDiscoveryDiagnostic {
            surface_id: None,
            scope_label: match &diagnostic.scope {
                jackin_config::ConfigSourceScope::Global => "global config".to_owned(),
                jackin_config::ConfigSourceScope::Workspaces => "workspace configs".to_owned(),
                jackin_config::ConfigSourceScope::Workspace(name) => {
                    format!("workspace {name}")
                }
            },
            unresolved_source: None,
            issue: match diagnostic.issue {
                ConfigSourceIssue::Unreadable => UsageDiscoveryIssue::ConfigUnreadable,
                ConfigSourceIssue::UnsupportedVersion => {
                    UsageDiscoveryIssue::ConfigVersionUnsupported
                }
                ConfigSourceIssue::TransientConflict => {
                    UsageDiscoveryIssue::ConfigTransientConflict
                }
                ConfigSourceIssue::Malformed
                | ConfigSourceIssue::Invalid
                | ConfigSourceIssue::InvalidWorkspaceName
                | ConfigSourceIssue::ConflictingWorkspaceDefinitions => {
                    UsageDiscoveryIssue::ConfigInvalid
                }
            },
        })
        .collect()
}

fn materialize_catalog(
    config_generation: Option<String>,
    candidates: BTreeMap<CredentialSourceKey, CandidateAccumulator>,
    diagnostics: Vec<UsageDiscoveryDiagnostic>,
) -> UsageDiscoveryCatalog {
    let mut descriptors = Vec::with_capacity(candidates.len());
    let mut sources = Vec::with_capacity(candidates.len());
    for (index, (key, candidate)) in candidates.into_iter().enumerate() {
        let source_id = format!("source-{:04}", index + 1);
        let capability_id = source_capability_id(candidate.surface, &key);
        let provenance = candidate.provenance.iter().cloned().collect::<Vec<_>>();
        descriptors.push(UsageSourceCandidateDescriptor {
            surface_id: candidate.surface.id().to_owned(),
            credential_kind: candidate.kind,
            source_id: source_id.clone(),
            capability_id: capability_id.clone(),
            provenance,
        });
        let source = match key {
            CredentialSourceKey::Profile { agent, root, .. } => {
                DiscoveredCredentialSource::Profile {
                    surface: candidate.surface,
                    agent,
                    root,
                    operator_home: candidate.operator_home.unwrap_or_default(),
                    account_label: candidate.account_label,
                    source_id,
                    capability_id,
                    provenance: candidate.provenance,
                }
            }
            CredentialSourceKey::Env {
                surface,
                handle,
                key,
                dispatch_key,
            } => DiscoveredCredentialSource::Env {
                surface,
                handle,
                key,
                dispatch_key,
                launch_keys: candidate.env_keys,
                kind: candidate.kind,
                account_label: candidate.account_label,
                source_id,
                capability_id,
                provenance: candidate.provenance,
            },
            CredentialSourceKey::Capability { surface, id } => {
                DiscoveredCredentialSource::Capability {
                    surface,
                    account_label: candidate.account_label,
                    source_id,
                    capability_id: id,
                }
            }
        };
        sources.push(source);
    }
    UsageDiscoveryCatalog {
        config_generation,
        candidates: descriptors,
        diagnostics,
        sources,
    }
}

fn source_capability_id(surface: HostSurfaceId, key: &CredentialSourceKey) -> String {
    if let CredentialSourceKey::Capability { id, .. } = key {
        return id.clone();
    }
    let evidence = match key {
        CredentialSourceKey::Profile {
            agent: Agent::Claude,
            claude_service: Some(service),
            ..
        } => return crate::usage::claude_source_capability_id_for_service(service),
        CredentialSourceKey::Profile { agent, root, .. } => {
            format!("profile-v1:{}:{}", agent.slug(), root.to_string_lossy())
        }
        CredentialSourceKey::Env {
            surface,
            handle,
            key,
            dispatch_key,
        } => {
            fn segment(value: &str) -> String {
                format!("{}:{value}", value.len())
            }
            format!(
                "env-v3:{}:{}:{}:{}",
                surface.id(),
                segment(key),
                segment(dispatch_key),
                segment(&handle.0)
            )
        }
        CredentialSourceKey::Capability { .. } => unreachable!("returned above"),
    };
    let hashed = jackin_core::account_key_hash(surface.id(), &evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

impl HostUsageRuntime {
    /// Run one fresh, read-only discovery scan. A failed scan returns `None`
    /// and never hands stale credentials back to broker rotation.
    pub fn stage_discovery(
        &mut self,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<Option<StagedUsageDiscovery>, String> {
        self.require_open()?;
        let Some(scope) = self.discovery_scope.clone() else {
            return Ok(None);
        };
        resolver.begin_manual_retry();
        let Ok(catalog) = discover_usage_sources(&scope, resolver) else {
            self.push_event(
                "discovery_failed",
                None,
                Some("current account discovery unavailable".to_owned()),
            );
            return Ok(None);
        };
        let discovered = validate_usage_sources(catalog, resolver);
        let changed = self.discovery.as_ref().is_none_or(|current| {
            super::broker::usage_catalog_entries(current)
                != super::broker::usage_catalog_entries(&discovered)
        });
        Ok(Some(StagedUsageDiscovery {
            base_generation: self.discovery_generation,
            changed,
            discovery: discovered,
        }))
    }

    /// Commit a successful discovery stage after broker activation. The local
    /// generation fence rejects an older scan even when its catalog revision
    /// string happens to match the newer scan.
    pub fn commit_staged_discovery(
        &mut self,
        staged: StagedUsageDiscovery,
    ) -> Result<bool, String> {
        self.require_open()?;
        if staged.base_generation != self.discovery_generation {
            return Err("stale usage discovery stage".to_owned());
        }
        if !staged.changed {
            self.push_event("discovery_reconciled", None, Some("unchanged".to_owned()));
            return Ok(false);
        }
        let current = discovered_account_keys(Some(&staged.discovery));
        self.discovery = Some(staged.discovery);
        self.discovery_generation = self.discovery_generation.saturating_add(1);
        self.discovered_views.retain(|key, _| current.contains(key));
        let active = self
            .broker_phases
            .iter()
            .filter(|(_, phase)| phase.is_active())
            .map(|(capability, _)| capability.clone())
            .collect::<Vec<_>>();
        self.broker_phases.clear();
        self.broker_generations.clear();
        for capability in active {
            self.push_event(
                "broker_phase_changed",
                Some(&capability.surface_id),
                Some("failed".to_owned()),
            );
        }
        self.push_event("discovery_reconciled", None, Some("changed".to_owned()));
        Ok(true)
    }

    /// Rescan the retained Rust discovery scope without dispatching provider probes.
    ///
    /// This is the manual-refresh reconciliation boundary used before broker
    /// capabilities are rebuilt. The prior validated generation remains usable
    /// when the read-only config scan is unavailable.
    pub fn reconcile_discovery(
        &mut self,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<bool, String> {
        let Some(staged) = self.stage_discovery(resolver)? else {
            return Ok(false);
        };
        self.commit_staged_discovery(staged)
    }

    pub(super) fn record_discovered_snapshot(
        &mut self,
        binding: &ValidatedCredentialBinding,
        mut view: FocusedUsageView,
    ) {
        let identity = binding.identity.clone().or_else(|| {
            CanonicalAccountIdentity::from_view(binding.surface, &view).map(|_| {
                CanonicalAccountIdentity::source_capability(binding.surface, &binding.capability_id)
            })
        });
        let Some(identity) = identity else {
            let error = view.last_error.clone();
            let kind = if error.is_some() {
                "probe_failed"
            } else {
                "snapshot_updated"
            };
            self.discovered_provider_views.insert(binding.surface, view);
            self.push_event(kind, Some(binding.surface.id()), error);
            return;
        };
        if view.account.account_label.trim().is_empty()
            && let Some(account) = self.discovery.as_ref().and_then(|discovery| {
                discovery
                    .accounts
                    .iter()
                    .find(|account| account.identity == identity)
            })
        {
            view.account.account_label = account.account_label.clone();
        }
        let account_key = identity.account_key();
        self.discovered_views
            .insert((binding.surface, account_key.clone()), view);
        self.discovered_provider_views.remove(&binding.surface);
        self.ensure_discovered_account(identity, account_key, binding);
        self.push_event("snapshot_updated", Some(binding.surface.id()), None);
    }

    fn ensure_discovered_account(
        &mut self,
        identity: CanonicalAccountIdentity,
        account_key: String,
        binding: &ValidatedCredentialBinding,
    ) {
        let Some(discovery) = &mut self.discovery else {
            return;
        };
        if let Some(account) = discovery
            .accounts
            .iter_mut()
            .find(|account| account.identity == identity)
        {
            account
                .provenance
                .extend(binding.provenance.iter().cloned());
            account.provenance.sort();
            account.provenance.dedup();
            if !account.source_ids.contains(&binding.source_id) {
                account.source_ids.push(binding.source_id.clone());
                account.source_ids.sort();
            }
            return;
        }
        let Some(view) = self
            .discovered_views
            .get(&(binding.surface, account_key.clone()))
        else {
            return;
        };
        discovery.accounts.push(DiscoveredAccountDescriptor {
            surface_id: binding.surface.id().to_owned(),
            account_key,
            account_label: view.account.account_label.clone(),
            provenance: binding.provenance.iter().cloned().collect(),
            source_ids: vec![binding.source_id.clone()],
            identity,
        });
    }
}

#[cfg(test)]
mod tests;
