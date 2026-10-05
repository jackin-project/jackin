// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Account-registry authority, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
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
    /// Logical authentication evidence supplied by the accepted host catalog.
    pub canonical_identity: Option<jackin_protocol::control::UsageCanonicalAccountIdentity>,
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
    /// Credential lookup could not complete; retry may succeed later.
    CredentialUnavailable,
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
            Self::CredentialUnavailable => "credential_unavailable",
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
            Self::CredentialUnavailable => "Credential access is temporarily unavailable",
            Self::KeychainConsentRequired => {
                "Keychain consent required; approve jackin in Keychain Access"
            }
            Self::CredentialMalformed => "Credentials are malformed",
            Self::InteractionRequired => "Credential access requires interaction",
        }
    }
}

/// Sanitized provider/scope diagnostic. No path, secret, or 1Password coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDiscoveryDiagnostic {
    /// Provider surface when the failure is provider-specific.
    pub surface_id: Option<String>,
    /// Rust-composed scope label (`account …`, `workspace …`).
    pub scope_label: String,
    /// Exact configured account registry IDs contributing this diagnostic.
    pub configured_account_ids: BTreeSet<String>,
    /// Stable machine-readable category.
    pub issue: UsageDiscoveryIssue,
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
    /// Exact configured account registry IDs resolving to this source.
    pub configured_account_ids: BTreeSet<String>,
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

/// Enumerate configured usage sources without accessing credential material.
///
/// Disabled runtimes retain declaration inventory; authentication and broker
/// admission remain deferred until a live discovery scan.
pub fn discover_usage_inventory(
    scope: &UsageDiscoveryScope,
) -> Result<ValidatedUsageDiscovery, String> {
    let (config_generation, candidates, diagnostics) = match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => {
            let paths =
                JackinPaths::resolve_with_env(operator_home, None, Some(config_root.as_os_str()));
            let snapshot = jackin_config::load_read_only_config_snapshot(&paths)
                .map_err(|_| "config snapshot unavailable".to_owned())?;
            let generation = snapshot.generation.as_str().to_owned();
            let diagnostics = config_diagnostics(&snapshot);
            let candidates = snapshot
                .config
                .accounts
                .iter()
                .filter(|(_, account)| account.enabled)
                .map(|(id, account)| {
                    let surface = provider_surface(account.provider);
                    let credential_kind = match &account.credential {
                        AccountCredential::Profile { .. } => UsageCredentialKind::Profile,
                        AccountCredential::ApiKey { .. } => UsageCredentialKind::ApiKey,
                        AccountCredential::OAuthToken { .. } => UsageCredentialKind::OAuthToken,
                    };
                    // Inventory identifiers cannot authorize a provider call.
                    // Bind them to the entire declaration generation without
                    // exporting paths, credential values, or source coordinates.
                    let source_id = jackin_core::account_key_hash(
                        surface.id(),
                        &format!("inventory-v1:{}:{generation}:{id}", id.len()),
                    );
                    let source_id = source_id
                        .strip_prefix("sha256:")
                        .unwrap_or(&source_id)
                        .to_owned();
                    let mut provenance = BTreeSet::from([format!("account {id}")]);
                    for (name, workspace) in &snapshot.config.workspaces {
                        if workspace.accounts.contains(id) {
                            provenance.insert(format!("workspace {name}"));
                        }
                    }
                    UsageSourceCandidateDescriptor {
                        surface_id: surface.id().to_owned(),
                        credential_kind,
                        capability_id: source_id.clone(),
                        source_id,
                        provenance: provenance.into_iter().collect(),
                        configured_account_ids: BTreeSet::from([id.clone()]),
                    }
                })
                .collect();
            (Some(generation), candidates, diagnostics)
        }
        UsageDiscoveryScope::Capsule { forwarded_accounts } => {
            let catalog = discover_forwarded_sources(forwarded_accounts);
            (
                catalog.config_generation,
                catalog.candidates,
                catalog.diagnostics,
            )
        }
    };
    Ok(ValidatedUsageDiscovery {
        config_generation,
        accounts: Vec::new(),
        candidates,
        diagnostics,
        bindings: Vec::new(),
    })
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
            }) || self.candidate_is_deferred(candidate)
        })
    }

    pub(super) fn candidate_is_deferred(&self, candidate: &UsageSourceCandidateDescriptor) -> bool {
        !self
            .bindings
            .iter()
            .any(|binding| binding.capability_id == candidate.capability_id)
            && !self.diagnostics.iter().any(|diagnostic| {
                diagnostic.surface_id.as_deref() == Some(candidate.surface_id.as_str())
                    && !diagnostic
                        .configured_account_ids
                        .is_disjoint(&candidate.configured_account_ids)
            })
    }

    /// Configured sources whose protected lookup has not been attempted.
    #[must_use]
    pub fn has_deferred_sources(&self, surface_id: &str) -> bool {
        self.candidates.iter().any(|candidate| {
            candidate.surface_id == surface_id && self.candidate_is_deferred(candidate)
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

pub(super) use jackin_core::ProfileCredentialSourceMaterial;

#[derive(Clone)]
pub(super) struct ValidatedCredentialBinding {
    pub surface: HostSurfaceId,
    pub identity: Option<CanonicalAccountIdentity>,
    pub source_id: String,
    pub capability_id: String,
    pub credential_revision: String,
    pub profile_material: Option<ProfileCredentialSourceMaterial>,
    pub provenance: BTreeSet<String>,
    pub configured_account_ids: BTreeSet<String>,
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
    },
    Amp {
        key: String,
    },
    Grok {
        auth: serde_json::Value,
        identity: Option<String>,
    },
    Kimi {
        token: String,
    },
    OpenCode {
        token: String,
    },
    Cursor {
        auth: crate::usage::CursorAuth,
        identity: Option<String>,
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
        provider: String,
        selector: Option<(String, Option<String>)>,
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
pub(super) enum DiscoveredCredentialSource {
    Profile {
        surface: HostSurfaceId,
        agent: Agent,
        provider: String,
        selector: Option<jackin_config::ProfileSelector>,
        root: PathBuf,
        operator_home: PathBuf,
        account_label: Option<String>,
        source_id: String,
        capability_id: String,
        provenance: BTreeSet<String>,
        configured_account_ids: BTreeSet<String>,
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
        configured_account_ids: BTreeSet<String>,
    },
    Capability {
        surface: HostSurfaceId,
        canonical_identity: Option<jackin_protocol::control::UsageCanonicalAccountIdentity>,
        account_label: Option<String>,
        source_id: String,
        capability_id: String,
        configured_account_ids: BTreeSet<String>,
    },
}

struct CandidateAccumulator {
    canonical_identity: Option<jackin_protocol::control::UsageCanonicalAccountIdentity>,
    surface: HostSurfaceId,
    kind: UsageCredentialKind,
    provenance: BTreeSet<String>,
    configured_account_ids: BTreeSet<String>,
    env_keys: BTreeSet<String>,
    account_label: Option<String>,
    operator_home: Option<PathBuf>,
}

fn merge_env_candidate(
    candidate: &mut CandidateAccumulator,
    provenance: &BTreeSet<String>,
    configured_account_ids: &BTreeSet<String>,
    env_key: &str,
    account_label: Option<&str>,
) {
    candidate.provenance.extend(provenance.clone());
    candidate
        .configured_account_ids
        .extend(configured_account_ids.iter().cloned());
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
    let mut rejected = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for account in accounts {
        let Some(surface) = HostSurfaceId::from_id(&account.surface_id) else {
            continue;
        };
        // Every known surface reaches Capsules: `DESKTOP_PROVIDER_ORDER` is
        // the Swift glance contract only, not forwarded admission.
        if !HostSurfaceId::ALL.contains(&surface) {
            continue;
        }
        if account.capability_id.is_empty()
            || account.capability_id.len() > 128
            || !account.capability_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            diagnostics.push(source_diagnostic(
                surface,
                &BTreeSet::new(),
                &BTreeSet::from(["forwarded to Capsule".to_owned()]),
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        }
        let key = CredentialSourceKey::Capability {
            surface,
            id: account.capability_id.clone(),
        };
        if rejected.contains(&key) {
            continue;
        }
        if candidates
            .get(&key)
            .is_some_and(|existing| existing.canonical_identity != account.canonical_identity)
        {
            candidates.remove(&key);
            rejected.insert(key);
            diagnostics.push(source_diagnostic(
                surface,
                &BTreeSet::new(),
                &BTreeSet::from(["forwarded to Capsule".to_owned()]),
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        }
        candidates
            .entry(key)
            .or_insert_with(|| CandidateAccumulator {
                canonical_identity: account.canonical_identity.clone(),
                surface,
                kind: UsageCredentialKind::ForwardedCapability,
                provenance: BTreeSet::from(["forwarded to Capsule".to_owned()]),
                configured_account_ids: BTreeSet::new(),
                env_keys: BTreeSet::new(),
                account_label: account.account_label.clone(),
                operator_home: None,
            });
    }
    materialize_catalog(None, candidates, diagnostics)
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
        let configured_account_ids = BTreeSet::from([id.clone()]);
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
            agent,
            directory,
            xdg_roots,
            source_selector,
        } = &account.credential
        {
            let root = xdg_roots
                .as_ref()
                .filter(|_| matches!(agent, Agent::Amp | Agent::Opencode))
                .map_or_else(
                    || resolve_profile_root(operator_home, directory),
                    |roots| roots.data.join(agent.slug()),
                );
            let selector = source_selector
                .as_ref()
                .map(|selector| (selector.entry.clone(), selector.profile.clone()));
            candidates
                .entry(CredentialSourceKey::Profile {
                    agent: *agent,
                    provider: account.provider.slug().to_owned(),
                    selector,
                    root,
                })
                .and_modify(|candidate| {
                    candidate.provenance.extend(provenance.clone());
                    candidate
                        .configured_account_ids
                        .extend(configured_account_ids.iter().cloned());
                    if candidate.account_label.is_none() {
                        candidate.account_label = label.clone();
                    }
                })
                .or_insert_with(|| CandidateAccumulator {
                    canonical_identity: None,
                    surface,
                    kind: UsageCredentialKind::Profile,
                    provenance,
                    configured_account_ids,
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
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        };
        for route in routes {
            if route.mode != expected_mode {
                diagnostics.push(account_diagnostic(
                    surface,
                    id,
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
                                &configured_account_ids,
                                entry.name,
                                label.as_deref(),
                            );
                        })
                        .or_insert_with(|| CandidateAccumulator {
                            canonical_identity: None,
                            surface,
                            kind,
                            provenance: provenance.clone(),
                            configured_account_ids: configured_account_ids.clone(),
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
            diagnostics.push(account_diagnostic(surface, id, issue));
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
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: format!("account {account_id}"),
        configured_account_ids: BTreeSet::from([account_id.to_owned()]),
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
            configured_account_ids: BTreeSet::new(),
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
            configured_account_ids: candidate.configured_account_ids.clone(),
        });
        let source = match key {
            CredentialSourceKey::Profile {
                agent,
                provider,
                selector,
                root,
            } => DiscoveredCredentialSource::Profile {
                surface: candidate.surface,
                agent,
                provider,
                selector: selector
                    .map(|(entry, profile)| jackin_config::ProfileSelector { entry, profile }),
                root,
                operator_home: candidate.operator_home.unwrap_or_default(),
                account_label: candidate.account_label,
                source_id,
                capability_id,
                provenance: candidate.provenance,
                configured_account_ids: candidate.configured_account_ids,
            },
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
                configured_account_ids: candidate.configured_account_ids,
            },
            CredentialSourceKey::Capability { surface, id } => {
                DiscoveredCredentialSource::Capability {
                    surface,
                    canonical_identity: candidate.canonical_identity,
                    account_label: candidate.account_label,
                    source_id,
                    capability_id: id,
                    configured_account_ids: candidate.configured_account_ids,
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
            agent,
            provider,
            selector,
            root,
        } => {
            let selector = profile_selector_value(selector.as_ref());
            let source = jackin_core::profile_credential_source_identity(
                *agent,
                provider,
                root,
                selector.as_ref(),
            );
            format!(
                "profile-v2:{}:{}",
                agent.slug(),
                source.descriptor_fingerprint
            )
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

#[derive(Clone)]
enum ProfileReadOutcome {
    Bytes(Vec<u8>),
    Missing,
    Unavailable,
    Denied,
    ConsentRequired,
}

trait ProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome;
    fn exists(&self, path: &Path) -> bool;
    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome;
    /// Presence-only probe for the Antigravity Keychain grant singleton.
    /// `Bytes` is always empty and never carries the grant: the CLI owns the
    /// secret, discovery only learns whether it exists.
    fn read_antigravity_keychain(&self) -> ProfileReadOutcome;
}

struct CachingProfileCredentialReader<'a> {
    inner: &'a dyn ProfileCredentialReader,
    exists: std::cell::RefCell<BTreeMap<PathBuf, bool>>,
    files: std::cell::RefCell<BTreeMap<PathBuf, ProfileReadOutcome>>,
    keychain: std::cell::RefCell<BTreeMap<String, ProfileReadOutcome>>,
    antigravity_grant: std::cell::RefCell<Option<ProfileReadOutcome>>,
}

impl<'a> CachingProfileCredentialReader<'a> {
    fn new(inner: &'a dyn ProfileCredentialReader) -> Self {
        Self {
            inner,
            exists: std::cell::RefCell::new(BTreeMap::new()),
            files: std::cell::RefCell::new(BTreeMap::new()),
            keychain: std::cell::RefCell::new(BTreeMap::new()),
            antigravity_grant: std::cell::RefCell::new(None),
        }
    }
}

impl ProfileCredentialReader for CachingProfileCredentialReader<'_> {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        if let Some(outcome) = self.files.borrow().get(path).cloned() {
            return outcome;
        }
        let outcome = self.inner.read(path);
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), outcome.clone());
        outcome
    }

    fn exists(&self, path: &Path) -> bool {
        if let Some(exists) = self.exists.borrow().get(path).copied() {
            return exists;
        }
        let exists = self.inner.exists(path);
        self.exists.borrow_mut().insert(path.to_path_buf(), exists);
        exists
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        if let Some(outcome) = self.keychain.borrow().get(&scope.service).cloned() {
            return outcome;
        }
        let outcome = self.inner.read_claude_keychain(scope);
        self.keychain
            .borrow_mut()
            .insert(scope.service.clone(), outcome.clone());
        outcome
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        if let Some(outcome) = self.antigravity_grant.borrow().clone() {
            return outcome;
        }
        let outcome = self.inner.read_antigravity_keychain();
        *self.antigravity_grant.borrow_mut() = Some(outcome.clone());
        outcome
    }
}

struct SystemProfileCredentialReader;

impl ProfileCredentialReader for SystemProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        match std::fs::read(path) {
            Ok(bytes) => ProfileReadOutcome::Bytes(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ProfileReadOutcome::Missing
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                ProfileReadOutcome::Denied
            }
            Err(_) => ProfileReadOutcome::Missing,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        match crate::usage::claude_keychain_state()
            .read_with(&scope.service, crate::usage::read_claude_keychain_item)
        {
            #[cfg(any(target_os = "macos", test))]
            crate::usage::ClaudeKeychainRead::Payload { json } => {
                ProfileReadOutcome::Bytes(json.into_bytes())
            }
            crate::usage::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
            crate::usage::ClaudeKeychainRead::Missing => ProfileReadOutcome::Missing,
            crate::usage::ClaudeKeychainRead::Unavailable => ProfileReadOutcome::Unavailable,
            crate::usage::ClaudeKeychainRead::ConsentRequired => {
                ProfileReadOutcome::ConsentRequired
            }
        }
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        #[cfg(target_os = "macos")]
        {
            use security_framework::item::{ItemClass, ItemSearchOptions};

            // Reference-only search: no `load_data`, so the grant payload is
            // never read into this process — presence is the whole answer.
            let mut options = ItemSearchOptions::new();
            options
                .class(ItemClass::generic_password())
                .service(crate::usage::ANTIGRAVITY_KEYCHAIN_SERVICE)
                .limit(1);
            match options.search() {
                Ok(results) if !results.is_empty() => ProfileReadOutcome::Bytes(Vec::new()),
                Ok(_) => ProfileReadOutcome::Missing,
                Err(error) => match crate::usage::classify_claude_keychain_status(error.code()) {
                    // Unreachable: the classifier only emits Denied/Missing.
                    // Fail closed to absence either way.
                    crate::usage::ClaudeKeychainRead::Payload { .. } => ProfileReadOutcome::Missing,
                    crate::usage::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
                    crate::usage::ClaudeKeychainRead::Missing => ProfileReadOutcome::Missing,
                    crate::usage::ClaudeKeychainRead::Unavailable => {
                        ProfileReadOutcome::Unavailable
                    }
                    crate::usage::ClaudeKeychainRead::ConsentRequired => {
                        ProfileReadOutcome::ConsentRequired
                    }
                },
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            ProfileReadOutcome::Missing
        }
    }
}

enum ProfileValidation {
    ProvenIdentity {
        identity: CanonicalAccountIdentity,
        account_label: Option<String>,
    },
    Authenticated {
        provider_id: Option<String>,
        account_label: Option<String>,
        material: Option<Box<ProfileCredentialMaterial>>,
    },
    Anonymous(Option<Box<ProfileCredentialMaterial>>),
    Missing,
    Unavailable,
    Denied,
    ConsentRequired,
    Malformed,
}

struct AccountAccumulator {
    label: String,
    provenance: BTreeSet<String>,
    source_ids: BTreeSet<String>,
}

/// Validate every pre-deduplicated source and merge authenticated identities.
///
/// Missing/malformed/denied sources produce diagnostics and never account rows.
pub fn validate_usage_sources(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ValidatedUsageDiscovery {
    validate_usage_sources_with_reader(catalog, env_resolver, &SystemProfileCredentialReader)
}

fn validate_usage_sources_with_reader(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedUsageDiscovery {
    let mut diagnostics = catalog.diagnostics;
    let mut bindings = Vec::new();
    let mut accounts = BTreeMap::<CanonicalAccountIdentity, AccountAccumulator>::new();

    let profile_reader = CachingProfileCredentialReader::new(profile_reader);
    let validated: Vec<ValidatedSourceParts> = catalog
        .sources
        .into_iter()
        .map(|source| validate_source(source, env_resolver, &profile_reader))
        .collect();
    // Each source supplies its own authentication evidence. Another source
    // sharing its provider cannot prove which account an anonymous key owns.
    for parts in validated {
        accumulate_validated_source(parts, &mut diagnostics, &mut bindings, &mut accounts);
    }

    let accounts = accounts
        .into_iter()
        .map(|(identity, account)| DiscoveredAccountDescriptor {
            surface_id: identity.surface.id().to_owned(),
            account_key: identity.account_key(),
            account_label: account.label,
            provenance: account.provenance.into_iter().collect(),
            source_ids: account.source_ids.into_iter().collect(),
            identity,
        })
        .collect();

    ValidatedUsageDiscovery {
        config_generation: catalog.config_generation,
        accounts,
        diagnostics,
        candidates: catalog.candidates,
        bindings,
    }
}

fn accumulate_validated_source(
    parts: ValidatedSourceParts,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
    bindings: &mut Vec<ValidatedCredentialBinding>,
    accounts: &mut BTreeMap<CanonicalAccountIdentity, AccountAccumulator>,
) {
    let (
        surface,
        source_id,
        capability_id,
        credential_revision,
        provenance,
        configured_account_ids,
        source,
        profile_material,
        outcome,
    ) = parts;
    match outcome {
        ProfileValidation::ProvenIdentity {
            identity,
            account_label,
        } => {
            let entry = accounts
                .entry(identity.clone())
                .or_insert_with(|| AccountAccumulator {
                    label: account_label
                        .filter(|label| !label.trim().is_empty())
                        .unwrap_or_else(|| "Authenticated account".to_owned()),
                    provenance: BTreeSet::new(),
                    source_ids: BTreeSet::new(),
                });
            entry.provenance.extend(provenance.iter().cloned());
            entry.source_ids.insert(source_id.clone());
            bindings.push(ValidatedCredentialBinding {
                surface,
                identity: Some(identity),
                source_id,
                capability_id,
                credential_revision,
                profile_material,
                provenance,
                configured_account_ids,
                source,
            });
        }
        ProfileValidation::Authenticated {
            provider_id,
            account_label,
            material: _,
        } => {
            let subject = provider_id
                .as_ref()
                .filter(|id| !id.is_empty())
                .map(|id| CanonicalAccountSubject::ProviderId(id.clone()))
                .or_else(|| {
                    account_label
                        .as_ref()
                        .filter(|label| !label.trim().is_empty())
                        .map(|_| {
                            // A label is presentation evidence only. Keep
                            // source identity when the provider did not
                            // return a stronger canonical subject.
                            CanonicalAccountSubject::SourceCapability(capability_id.clone())
                        })
                });
            let Some(subject) = subject else {
                bindings.push(ValidatedCredentialBinding {
                    surface,
                    identity: None,
                    source_id,
                    capability_id,
                    credential_revision,
                    profile_material,
                    provenance,
                    configured_account_ids,
                    source,
                });
                return;
            };
            let identity = CanonicalAccountIdentity { surface, subject };
            let label = account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default();
            let entry = accounts
                .entry(identity.clone())
                .or_insert_with(|| AccountAccumulator {
                    label,
                    provenance: BTreeSet::new(),
                    source_ids: BTreeSet::new(),
                });
            entry.provenance.extend(provenance.iter().cloned());
            entry.source_ids.insert(source_id.clone());
            bindings.push(ValidatedCredentialBinding {
                surface,
                identity: Some(identity),
                source_id,
                capability_id,
                credential_revision,
                profile_material,
                provenance,
                configured_account_ids,
                source,
            });
        }
        ProfileValidation::Anonymous(_) => bindings.push(ValidatedCredentialBinding {
            surface,
            identity: None,
            source_id,
            capability_id,
            credential_revision,
            profile_material,
            provenance,
            configured_account_ids,
            source,
        }),
        ProfileValidation::Missing => diagnostics.push(source_diagnostic(
            surface,
            &configured_account_ids,
            &provenance,
            UsageDiscoveryIssue::CredentialMissing,
        )),
        ProfileValidation::Unavailable => diagnostics.push(source_diagnostic(
            surface,
            &configured_account_ids,
            &provenance,
            UsageDiscoveryIssue::CredentialUnavailable,
        )),
        ProfileValidation::Denied => diagnostics.push(source_diagnostic(
            surface,
            &configured_account_ids,
            &provenance,
            UsageDiscoveryIssue::CredentialDenied,
        )),
        ProfileValidation::ConsentRequired => diagnostics.push(source_diagnostic(
            surface,
            &configured_account_ids,
            &provenance,
            UsageDiscoveryIssue::KeychainConsentRequired,
        )),
        ProfileValidation::Malformed => diagnostics.push(source_diagnostic(
            surface,
            &configured_account_ids,
            &provenance,
            UsageDiscoveryIssue::CredentialMalformed,
        )),
    }
}

type ValidatedSourceParts = (
    HostSurfaceId,
    String,
    String,
    String,
    BTreeSet<String>,
    BTreeSet<String>,
    ValidatedCredentialSource,
    Option<ProfileCredentialSourceMaterial>,
    ProfileValidation,
);

fn validate_source(
    source: DiscoveredCredentialSource,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedSourceParts {
    match source {
        DiscoveredCredentialSource::Profile {
            surface,
            agent,
            provider,
            selector,
            root,
            operator_home,
            account_label: _,
            source_id,
            capability_id,
            provenance,
            configured_account_ids,
        } => {
            // The implemented OpenCode collector owns native OpenCode billing.
            // A compatible multi-provider store cannot authorize that collector
            // under a different issuer's surface.
            if agent == Agent::Opencode && provider != AiProvider::Opencode.slug() {
                return (
                    surface,
                    source_id,
                    capability_id,
                    opaque_credential_revision("unsupported-opencode-profile-provider"),
                    provenance,
                    configured_account_ids,
                    ValidatedCredentialSource::Capability,
                    None,
                    ProfileValidation::Malformed,
                );
            }
            let root = effective_profile_root(profile_reader, agent, &root);
            let selected_kimi_slot = if agent == Agent::Kimi {
                selected_kimi_auth_slot(profile_reader, &root)
            } else {
                None
            };
            let selected_kimi_path = selected_kimi_slot
                .as_ref()
                .map(|slot| root.join(&slot.credential_relative_path));
            let mut outcome = profile_identity_for_selected_kimi_slot(
                profile_reader,
                agent,
                &root,
                &operator_home,
                selected_kimi_slot.as_ref(),
            );
            let profile_material = captured_profile_material_for_selected_kimi_slot(
                profile_reader,
                agent,
                &provider,
                selector.as_ref(),
                &root,
                &operator_home,
                selected_kimi_slot.as_ref(),
            );
            if profile_material.is_none()
                && agent != Agent::Antigravity
                && matches!(
                    &outcome,
                    ProfileValidation::Authenticated { .. } | ProfileValidation::Anonymous(_)
                )
            {
                outcome = ProfileValidation::Malformed;
            }
            let credential_revision = profile_credential_revision(
                profile_reader,
                agent,
                &root,
                &operator_home,
                selected_kimi_path.as_deref(),
            );
            let source = match &outcome {
                ProfileValidation::Authenticated { material, .. }
                | ProfileValidation::Anonymous(material) => material.clone().map_or(
                    // A material-less local profile (Muse identity, omp/hermes
                    // attribution) is unpollable by design — never a forwarded
                    // trust-domain token, so never `Capability`.
                    ValidatedCredentialSource::Unpollable,
                    |material| ValidatedCredentialSource::Profile(*material),
                ),
                _ => ValidatedCredentialSource::Capability,
            };
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                configured_account_ids,
                source,
                profile_material,
                outcome,
            )
        }
        DiscoveredCredentialSource::Env {
            surface,
            handle,
            key,
            dispatch_key,
            launch_keys,
            kind: _,
            account_label: _,
            source_id,
            capability_id,
            provenance,
            configured_account_ids,
        } => {
            let material = env_resolver.source_material(surface, &key, &handle);
            let outcome = match env_resolver.identify_provider_credential(surface, &handle) {
                ProviderCredentialIdentityOutcome::Authenticated {
                    provider_id,
                    account_label: auth_label,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label,
                    material: None,
                },
                ProviderCredentialIdentityOutcome::Anonymous => ProfileValidation::Anonymous(None),
                ProviderCredentialIdentityOutcome::Missing => ProfileValidation::Missing,
                ProviderCredentialIdentityOutcome::Denied => ProfileValidation::Denied,
                ProviderCredentialIdentityOutcome::Malformed => ProfileValidation::Malformed,
            };
            let revision_evidence = serde_json::json!({
                "kind": "env-v4",
                "surface": surface.id(),
                "key": key,
                "dispatch_key": dispatch_key,
                "handle": handle.0,
                "material": material.as_ref().map(|material| serde_json::json!({
                    "source": material.source,
                    "material_fingerprint": material.material_fingerprint,
                })),
            });
            let credential_revision = opaque_credential_revision(&revision_evidence.to_string());
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                configured_account_ids,
                ValidatedCredentialSource::Env {
                    handle,
                    key,
                    dispatch_key,
                    launch_keys,
                    material,
                },
                None,
                outcome,
            )
        }
        DiscoveredCredentialSource::Capability {
            surface,
            canonical_identity,
            account_label,
            source_id,
            capability_id,
            configured_account_ids,
        } => {
            let provenance = BTreeSet::from(["forwarded to Capsule".to_owned()]);
            let outcome = match canonical_identity {
                Some(evidence) => CanonicalAccountIdentity::from_protocol(surface, &evidence)
                    .map_or(ProfileValidation::Malformed, |identity| {
                        ProfileValidation::ProvenIdentity {
                            identity,
                            account_label,
                        }
                    }),
                None => ProfileValidation::Anonymous(None),
            };
            (
                surface,
                source_id,
                capability_id.clone(),
                opaque_credential_revision(&format!("capability:{capability_id}")),
                provenance,
                configured_account_ids,
                ValidatedCredentialSource::Capability,
                None,
                outcome,
            )
        }
    }
}

fn source_diagnostic(
    surface: HostSurfaceId,
    configured_account_ids: &BTreeSet<String>,
    provenance: &BTreeSet<String>,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: provenance.iter().cloned().collect::<Vec<_>>().join(", "),
        configured_account_ids: configured_account_ids.clone(),
        issue,
    }
}

/// Return an opaque revision for the complete credential material read for a
/// profile source. The path-derived source id is intentionally not enough:
/// providers frequently rotate tokens in place without changing the profile
/// path or account identity.
fn profile_selector_value(
    selector: Option<&(String, Option<String>)>,
) -> Option<serde_json::Value> {
    selector.map(|(entry, profile)| {
        let mut value = serde_json::json!({"entry": entry});
        if let Some(profile) = profile {
            value["profile"] = serde_json::Value::String(profile.clone());
        }
        value
    })
}

fn effective_profile_root(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
) -> PathBuf {
    if agent == Agent::Amp {
        let source = jackin_config::amp_credentials_path_from_presence(
            root,
            reader.exists(&root.join("secrets.json")),
            reader.exists(&root.join("data/amp/secrets.json")),
        );
        source.parent().unwrap_or(root).to_path_buf()
    } else {
        root.to_path_buf()
    }
}

fn selected_kimi_auth_slot(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
) -> Option<jackin_config::KimiRuntimeAuthSlot> {
    let ProfileReadOutcome::Bytes(config) = reader.read(&root.join("config.toml")) else {
        return None;
    };
    // Kimi route overrides are account-owned and stripped from profile-backed
    // agent sessions, so the effective profile environment has no route vars.
    let effective_environment = BTreeMap::new();
    Some(
        jackin_config::kimi_runtime_auth_slot(
            &config,
            jackin_config::KIMI_CODE_AUTH_SLOT_CONTRACT_VERSION,
            &effective_environment,
        )
        .ok()?,
    )
}

/// Hash the same primary payload consumed by identity/material validation.
/// Protected material never implements Debug or escapes through descriptors.
#[cfg(test)]
fn captured_profile_material(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    provider: &str,
    selector: Option<&jackin_config::ProfileSelector>,
    root: &Path,
    operator_home: &Path,
) -> Option<ProfileCredentialSourceMaterial> {
    let selected_kimi_slot = if agent == Agent::Kimi {
        selected_kimi_auth_slot(reader, root)
    } else {
        None
    };
    captured_profile_material_for_selected_kimi_slot(
        reader,
        agent,
        provider,
        selector,
        root,
        operator_home,
        selected_kimi_slot.as_ref(),
    )
}

fn captured_profile_material_for_selected_kimi_slot(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    provider: &str,
    selector: Option<&jackin_config::ProfileSelector>,
    root: &Path,
    operator_home: &Path,
    selected_kimi_slot: Option<&jackin_config::KimiRuntimeAuthSlot>,
) -> Option<ProfileCredentialSourceMaterial> {
    let raw = match agent {
        Agent::Claude => claude_primary_read(reader, root, operator_home),
        Agent::Codex
        | Agent::Grok
        | Agent::Opencode
        | Agent::Cursor
        | Agent::Muse
        | Agent::Hermes => reader.read(&root.join("auth.json")),
        Agent::Amp => reader.read(&root.join("secrets.json")),
        Agent::Kimi => reader.read(&root.join(&selected_kimi_slot?.credential_relative_path)),
        Agent::Gemini => reader.read(&root.join("oauth_creds.json")),
        Agent::Omp => reader.read(&root.join("agent/agent.db")),
        // Presence-only grant cannot attest to material forwarded to a Capsule.
        Agent::Antigravity => return None,
    };
    let ProfileReadOutcome::Bytes(bytes) = raw else {
        return None;
    };
    let material_revision =
        jackin_core::profile_credential_material_revision(agent, &bytes).ok()?;
    let configured_selector = selector.map(serde_json::to_value).transpose().ok()?;
    let selector = if let Some(slot) = selected_kimi_slot {
        let mut descriptor = serde_json::Map::new();
        if let Some(configured) = configured_selector {
            descriptor.insert("profile".to_owned(), configured);
        }
        descriptor.insert(
            "kimi_runtime_auth_slot".to_owned(),
            serde_json::to_value(slot).ok()?,
        );
        Some(serde_json::Value::Object(descriptor))
    } else {
        configured_selector
    };
    Some(ProfileCredentialSourceMaterial {
        source: jackin_core::profile_credential_source_identity(
            agent,
            provider,
            root,
            selector.as_ref(),
        ),
        material_revision,
    })
}

fn profile_credential_revision(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
    selected_kimi_path: Option<&Path>,
) -> String {
    let mut evidence = Vec::new();
    let mut file = |label: &str, path: PathBuf| {
        append_profile_read(&mut evidence, label, reader.read(&path));
    };
    match agent {
        Agent::Claude => {
            file("claude.credentials", root.join(".credentials.json"));
            file("claude.config", root.join(".claude.json"));
            if root == operator_home.join(".claude") {
                file("claude.home-config", operator_home.join(".claude.json"));
            }
            if let Some(scope) =
                jackin_core::claude_keychain_scope(root, operator_home, operator_home)
            {
                append_profile_read(
                    &mut evidence,
                    "claude.keychain",
                    reader.read_claude_keychain(&scope),
                );
            }
        }
        Agent::Codex => file("codex.auth", root.join("auth.json")),
        Agent::Amp => {
            let path = root.join("secrets.json");
            file("amp.secrets", path);
        }
        Agent::Kimi => {
            let Some(path) = selected_kimi_path else {
                return opaque_credential_revision("kimi.credentials:missing-selected-path");
            };
            let path = path.to_path_buf();
            let relative = path.strip_prefix(root).unwrap_or(path.as_path());
            let label = format!("kimi.credentials:{}", relative.display());
            file(&label, path);
        }
        Agent::Grok => file("grok.auth", root.join("auth.json")),
        Agent::Opencode => file("opencode.auth", root.join("auth.json")),
        Agent::Antigravity => append_profile_read(
            &mut evidence,
            "antigravity.keychain",
            reader.read_antigravity_keychain(),
        ),
        Agent::Gemini => file("gemini.oauth", root.join("oauth_creds.json")),
        Agent::Cursor => {
            file("cursor.auth", root.join("auth.json"));
            file("cursor.config", root.join("cli-config.json"));
        }
        Agent::Muse => file("muse.auth", root.join("auth.json")),
        Agent::Omp => file("omp.database", root.join("agent/agent.db")),
        Agent::Hermes => file("hermes.auth", root.join("auth.json")),
    }
    opaque_credential_revision(&evidence.join("|"))
}

fn append_profile_read(evidence: &mut Vec<String>, label: &str, outcome: ProfileReadOutcome) {
    match outcome {
        ProfileReadOutcome::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
            for byte in &bytes {
                let _ignored = write!(hex, "{byte:02x}");
            }
            evidence.push(format!("{label}:bytes:{}:{hex}", bytes.len()));
        }
        ProfileReadOutcome::Missing => evidence.push(format!("{label}:missing")),
        ProfileReadOutcome::Denied => evidence.push(format!("{label}:denied")),
        ProfileReadOutcome::Unavailable => evidence.push(format!("{label}:unavailable")),
        ProfileReadOutcome::ConsentRequired => {
            evidence.push(format!("{label}:consent-required"));
        }
    }
}

fn opaque_credential_revision(evidence: &str) -> String {
    let hashed = jackin_core::account_key_hash("usage-credential-material-v2", evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

#[cfg(test)]
fn profile_identity(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    let selected_kimi_slot = if agent == Agent::Kimi {
        selected_kimi_auth_slot(reader, root)
    } else {
        None
    };
    profile_identity_for_selected_kimi_slot(
        reader,
        agent,
        root,
        operator_home,
        selected_kimi_slot.as_ref(),
    )
}

fn profile_identity_for_selected_kimi_slot(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
    selected_kimi_slot: Option<&jackin_config::KimiRuntimeAuthSlot>,
) -> ProfileValidation {
    match agent {
        Agent::Claude => claude_profile_identity(reader, root, operator_home),
        Agent::Codex => codex_profile_identity(reader, &root.join("auth.json")),
        Agent::Amp => {
            let path = root.join("secrets.json");
            amp_profile_identity(reader, &path)
        }
        Agent::Kimi => {
            let Some(slot) = selected_kimi_slot else {
                return ProfileValidation::Missing;
            };
            let value = match read_json(reader, &root.join(&slot.credential_relative_path)) {
                Ok(Some(value)) => value,
                Ok(None) => return ProfileValidation::Missing,
                Err(outcome) => return outcome,
            };
            crate::usage::kimi_local_token_from_value(&value, chrono::Utc::now().timestamp())
                .map_or(ProfileValidation::Malformed, |token| {
                    ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Kimi {
                        token,
                    })))
                })
        }
        Agent::Grok => grok_profile_identity(reader, &root.join("auth.json")),
        Agent::Opencode => opencode_profile_identity(reader, &root.join("auth.json")),
        // Antigravity wires through the host Keychain grant singleton: the
        // CLI owns the secret, so grant presence alone mints refresh
        // material and refresh shells out to `agy`.
        Agent::Antigravity => antigravity_profile_identity(reader),
        Agent::Gemini => gemini_profile_identity(reader, &root.join("oauth_creds.json")),
        Agent::Cursor => cursor_profile_identity(reader, root),
        // Muse stays explicitly unwired: identity is verified locally but no
        // material is minted — the secret lives in the platform credential
        // store and no pollable usage fetch exists by design
        // (`MuseKeyExchangePolicy::polling_enabled` is false), so refresh
        // cannot dispatch.
        Agent::Muse => muse_profile_identity(reader, &root.join("auth.json")),
        // omp stays explicitly unwired: it is an attribution-only aggregator
        // with no native identity or usage endpoint. SQLite store presence
        // (not content) is verified; table parsing belongs to a later lane.
        Agent::Omp => {
            if reader.exists(&root.join("agent/agent.db")) {
                ProfileValidation::Anonymous(None)
            } else {
                ProfileValidation::Missing
            }
        }
        // Hermes stays explicitly unwired: attribution-only adapter with no
        // Hermes-native quota API; usage needs caller-supplied underlying
        // buckets the refresh lane cannot produce.
        Agent::Hermes => anonymous_when_present(reader, &root.join("auth.json")),
    }
}

/// File present (any JSON shape) → anonymous binding; missing/denied/
/// malformed propagate truthfully. Used for agents whose identity
/// extraction is deferred to the usage lane.
fn anonymous_when_present(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    match read_json(reader, path) {
        Ok(Some(_)) => ProfileValidation::Anonymous(None),
        Ok(None) => ProfileValidation::Missing,
        Err(outcome) => outcome,
    }
}

/// Cursor identity comes from the sibling `cli-config.json` (`authInfo`
/// email), verified locally; token presence in `auth.json` is proven at
/// discovery; captured token and identity stay together through refresh. A
/// present-but-tokenless `auth.json` is malformed, never an anonymous
/// binding refresh cannot serve.
fn cursor_profile_identity(reader: &dyn ProfileCredentialReader, root: &Path) -> ProfileValidation {
    let auth_path = root.join("auth.json");
    let value = match read_json(reader, &auth_path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(auth) = crate::usage::cursor_auth_from_value(&value) else {
        return ProfileValidation::Malformed;
    };
    let label = read_json(reader, &root.join("cli-config.json"))
        .ok()
        .flatten()
        .and_then(|config| crate::usage::cursor_cli_identity_from_value(&config));
    let material = Some(Box::new(ProfileCredentialMaterial::Cursor {
        auth,
        identity: label.clone(),
    }));
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
        None => ProfileValidation::Anonymous(material),
    }
}

/// Gemini identity comes from `oauth_creds.json` when it names the login;
/// any valid credential file mints material (the Grok shape), since refresh
/// only needs discovery-proven OAuth presence until an entitlement endpoint
/// lands.
fn gemini_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Gemini {
        creds_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_email", "user_id", "account"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

/// Antigravity identity is the host Keychain grant singleton, probed for
/// presence only: the CLI owns the secret, so any payload is ignored and no
/// identity label is extracted. Grant present → anonymous binding with
/// refresh material; absent/denied propagates truthfully.
fn antigravity_profile_identity(reader: &dyn ProfileCredentialReader) -> ProfileValidation {
    match reader.read_antigravity_keychain() {
        ProfileReadOutcome::Bytes(_) => {
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Antigravity)))
        }
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Unavailable => ProfileValidation::Unavailable,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
    }
}

/// Muse identity comes from `auth.json` (`providers.meta.user_email`),
/// verified locally; the secret itself stays in the host Keychain.
fn muse_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let label = value
        .pointer("/providers/meta/user_email")
        .or_else(|| value.pointer("/providers/meta/user_full_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned);
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material: None,
        },
        None => ProfileValidation::Anonymous(None),
    }
}

fn opencode_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    match reader.read(path) {
        ProfileReadOutcome::Missing => {
            if path
                .parent()
                .map(|parent| parent.join("opencode.db"))
                .is_some_and(|database| reader.exists(&database))
            {
                // Database-only OpenCode stores have no materializable auth
                // source. Do not advertise a usage profile until the database
                // credential identity can be carried through launch binding.
                ProfileValidation::Malformed
            } else {
                ProfileValidation::Missing
            }
        }
        ProfileReadOutcome::Unavailable => ProfileValidation::Unavailable,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
            let Some(entries) = value.as_object() else {
                return ProfileValidation::Malformed;
            };
            if entries.len() != 1 {
                return ProfileValidation::Malformed;
            }
            let entry = value.get("opencode-go");
            let Some(entry) = entry else {
                return ProfileValidation::Missing;
            };
            let kind = entry.get("type").and_then(serde_json::Value::as_str);
            let key = entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty());
            if kind != Some("api") || key.is_none() {
                return ProfileValidation::Malformed;
            }
            let Ok(token) = crate::usage::opencode_api_key_from_value(&value) else {
                return ProfileValidation::Malformed;
            };
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::OpenCode {
                token,
            })))
        }
    }
}

fn claude_primary_read(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
    operator_home: &Path,
) -> ProfileReadOutcome {
    match reader.read(&root.join(".credentials.json")) {
        ProfileReadOutcome::Bytes(bytes)
            if !std::str::from_utf8(&bytes).is_ok_and(|text| text.trim().is_empty()) =>
        {
            return ProfileReadOutcome::Bytes(bytes);
        }
        ProfileReadOutcome::Bytes(_) | ProfileReadOutcome::Missing => {}
        outcome => return outcome,
    }
    let Some(scope) = jackin_core::claude_keychain_scope(root, operator_home, operator_home) else {
        return ProfileReadOutcome::Missing;
    };
    reader.read_claude_keychain(&scope)
}

fn claude_profile_identity(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    let value = match claude_primary_read(reader, root, operator_home) {
        ProfileReadOutcome::Bytes(bytes) => {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) => value,
                Err(_) => return ProfileValidation::Malformed,
            }
        }
        ProfileReadOutcome::Missing => return ProfileValidation::Missing,
        ProfileReadOutcome::Unavailable => return ProfileValidation::Unavailable,
        ProfileReadOutcome::Denied => return ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => return ProfileValidation::ConsentRequired,
    };
    let Some(credential) = crate::usage::claude_oauth_from_value(&value) else {
        return ProfileValidation::Malformed;
    };
    let mut account_label = crate::usage::claude_email_from_value(&value);
    let mut organization_type = crate::usage::claude_organization_type_from_value(&value);
    let mut paths = vec![root.join(".claude.json")];
    if root == operator_home.join(".claude") {
        paths.push(operator_home.join(".claude.json"));
    }
    for path in paths {
        match read_json(reader, &path) {
            Ok(Some(value)) => {
                if account_label.is_none() {
                    account_label = crate::usage::claude_email_from_value(&value);
                }
                if organization_type.is_none() {
                    organization_type = crate::usage::claude_organization_type_from_value(&value);
                }
            }
            Ok(None) => {}
            Err(outcome) => return outcome,
        }
    }
    let is_anonymous = account_label.is_none() && credential.refresh_token.is_none();
    let material = Some(Box::new(ProfileCredentialMaterial::Claude(
        crate::usage::ClaudeResolved {
            access_token: credential.access_token,
            subscription_type: credential.subscription_type,
            account_email: account_label.clone(),
            organization_type,
            credential_origin: "OAuth · configured profile".to_owned(),
            is_anonymous,
        },
    )));
    account_label.map_or(ProfileValidation::Anonymous(material.clone()), |label| {
        ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        }
    })
}

fn codex_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(credentials) = crate::usage::codex_oauth_from_value(&value) else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Codex {
        credentials: credentials.clone(),
    }));
    if credentials.account_id.is_none() && credentials.account_label.is_none() {
        ProfileValidation::Anonymous(material)
    } else {
        ProfileValidation::Authenticated {
            provider_id: credentials.account_id,
            account_label: credentials.account_label,
            material,
        }
    }
}

fn amp_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Ok(payload) = jackin_core::amp_profile_credential_payload(&value) else {
        return ProfileValidation::Malformed;
    };
    let Some(key) = payload
        .get("apiKey@https://ampcode.com/")
        .and_then(serde_json::Value::as_str)
    else {
        return ProfileValidation::Malformed;
    };
    ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Amp {
        key: key.to_owned(),
    })))
}

fn grok_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    if crate::usage::grok_bearer_token_from_value(&value, chrono::Utc::now().timestamp()).is_err() {
        return ProfileValidation::Malformed;
    }
    let identity = first_recursive_string(&value, &["email", "user_id", "team_id"]);
    let material = Some(Box::new(ProfileCredentialMaterial::Grok {
        auth: value,
        identity: identity.clone(),
    }));
    identity.map_or(ProfileValidation::Anonymous(material.clone()), |label| {
        ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        }
    })
}

fn read_json(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> Result<Option<serde_json::Value>, ProfileValidation> {
    match reader.read(path) {
        ProfileReadOutcome::Bytes(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| ProfileValidation::Malformed),
        ProfileReadOutcome::Missing => Ok(None),
        ProfileReadOutcome::Unavailable => Err(ProfileValidation::Unavailable),
        ProfileReadOutcome::Denied => Err(ProfileValidation::Denied),
        ProfileReadOutcome::ConsentRequired => Err(ProfileValidation::ConsentRequired),
    }
}

fn first_recursive_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(found) = map
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|found| !found.is_empty())
                {
                    return Some(found.to_owned());
                }
            }
            map.values()
                .find_map(|nested| first_recursive_string(nested, keys))
        }
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|nested| first_recursive_string(nested, keys)),
        _ => None,
    }
}

pub(super) fn refresh_credential_binding(
    binding: &ValidatedCredentialBinding,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderCredentialRefreshOutcome {
    let (view, rate_limit) = match &binding.source {
        ValidatedCredentialSource::Env {
            handle,
            dispatch_key,
            ..
        } => {
            return env_resolver.refresh_provider_credential(binding.surface, dispatch_key, handle);
        }
        ValidatedCredentialSource::Capability => {
            return ProviderCredentialRefreshOutcome::Malformed;
        }
        // Deliberate no-poll, never a provider outage: the honest
        // `Unsupported` view flows through the success path, outside
        // retry/backoff.
        ValidatedCredentialSource::Unpollable => (
            crate::usage::unpollable_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(resolved)) => {
            crate::usage::claude_profile_view_with_rate_limit(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
                resolved.clone(),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex { credentials }) => {
            crate::usage::codex_profile_snapshot_with_rate_limit(
                binding.surface.agent_slug(),
                credentials,
                chrono::Utc::now().timestamp(),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) => (
            crate::usage::amp_api_key_snapshot(
                binding.surface.agent_slug(),
                key,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Grok { auth, identity }) => {
            crate::usage::grok_profile_snapshot(
                binding.surface.agent_slug(),
                auth,
                identity.as_deref(),
                chrono::Utc::now().timestamp(),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Kimi { token }) => {
            let now = chrono::Utc::now().timestamp();
            (
                crate::usage::kimi_profile_snapshot(
                    binding.surface.agent_slug(),
                    token.as_str(),
                    now,
                ),
                None,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::OpenCode { token }) => (
            crate::usage::opencode_profile_snapshot(
                binding.surface.agent_slug(),
                token,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor {
            auth,
            identity,
        }) => (
            crate::usage::cursor_profile_snapshot(
                binding.surface.agent_slug(),
                auth,
                identity.as_deref(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Gemini { creds_path }) => {
            // Re-prove OAuth presence at refresh: a file deleted after
            // discovery is NeedsSecret, never a stale Unsupported.
            let has_oauth = creds_path.is_file();
            (
                crate::usage::gemini_snapshot_with_presence(
                    binding.surface.agent_slug(),
                    binding.surface.provider_label(),
                    has_oauth,
                    false,
                    "OAuth · configured profile",
                    chrono::Utc::now().timestamp(),
                ),
                None,
            )
        }
        // The Keychain grant needs no secret material here: `agy` owns the
        // grant and the collector shells out to it.
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity) => (
            crate::usage::antigravity_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
    };
    ProviderCredentialRefreshOutcome::Snapshot {
        view: Box::new(view),
        rate_limit,
    }
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
        // Provider quota/presentation cannot manufacture authentication
        // evidence. Only the validated source binding authorizes membership.
        let identity = binding.identity.clone();
        view.canonical_identity = identity
            .as_ref()
            .map(CanonicalAccountIdentity::protocol_identity);
        let capability = super::broker::capability_for_binding(
            binding,
            self.discovery
                .as_ref()
                .and_then(|discovery| discovery.config_generation.as_deref()),
        );
        view.account_identity = Some((&capability).into());
        if let Some(route_identity) = view.account_identity.as_mut() {
            route_identity.source_revision = self.discovery.as_ref().and_then(|discovery| {
                super::broker::usage_catalog_entries(discovery)
                    .into_iter()
                    .find(|entry| entry.capability == capability)
                    .map(|entry| entry.revision)
            });
        }
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
        // Unsupported/error snapshots carry diagnostic labels, not new
        // identity evidence. Retain the independently authenticated label.
        if (view.account.account_label.trim().is_empty()
            || matches!(
                view.confidence,
                jackin_protocol::control::UsageConfidence::None
                    | jackin_protocol::control::UsageConfidence::PresenceOnly
            ))
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
