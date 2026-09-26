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
    Agent, JackinPaths, UsageCredentialEnvName, UsageCredentialOwner, WorkspaceName,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDiscoveryDiagnostic {
    /// Provider surface when the failure is provider-specific.
    pub surface_id: Option<String>,
    /// Rust-composed scope label (`account …`, `workspace …`).
    pub scope_label: String,
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
        key: String,
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
    },
    Env {
        surface: HostSurfaceId,
        handle: OpaqueCredentialHandle,
        key: String,
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
    env_key: Option<String>,
    account_label: Option<String>,
    operator_home: Option<PathBuf>,
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
                env_key: None,
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
fn usage_account_alias_entry(entry: UsageCredentialEnvName) -> UsageCredentialEnvName {
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
        owner: entry.owner,
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
        let (surface, owner) = provider_surface(account.provider);
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
        if let AccountCredential::Profile {
            agent, directory, ..
        } = &account.credential
        {
            let root = resolve_profile_root(operator_home, directory);
            candidates
                .entry(CredentialSourceKey::Profile {
                    agent: *agent,
                    root,
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
                    env_key: None,
                    account_label: label.clone(),
                    operator_home: Some(operator_home.to_path_buf()),
                });
            continue;
        }
        let (value, kind) = match &account.credential {
            AccountCredential::ApiKey { value, .. } => (value, UsageCredentialKind::ApiKey),
            AccountCredential::OAuthToken { value, .. } => (value, UsageCredentialKind::OAuthToken),
            AccountCredential::Profile { .. } => continue,
        };
        let entry = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
            .iter()
            .copied()
            .find(|entry| {
                entry.owner == owner
                    && (entry.name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
                        == (kind == UsageCredentialKind::OAuthToken)
            });
        let Some(entry) = entry else {
            diagnostics.push(account_diagnostic(
                surface,
                id,
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        };
        // Reuse protected env/1Password resolution with one explicit declaration.
        // Global env is retained only for interpolation dependencies; no roles,
        // workspaces or unrelated provider declarations are enumerated.
        let mut isolated = AppConfig {
            env: config.env.clone(),
            ..AppConfig::default()
        };
        let alias = usage_account_alias_entry(entry);
        isolated.env.insert(alias.name.to_owned(), value.clone());
        let resolutions = resolver.resolve_provider_credentials(&isolated, None, None, &[alias]);
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
                        key: entry.name.to_owned(),
                    })
                    .and_modify(|candidate| {
                        candidate.provenance.extend(provenance.clone());
                        if candidate.account_label.is_none() {
                            candidate.account_label = label.clone();
                        }
                    })
                    .or_insert_with(|| CandidateAccumulator {
                        surface,
                        kind,
                        provenance,
                        env_key: Some(entry.name.to_owned()),
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

fn provider_surface(provider: AiProvider) -> (HostSurfaceId, UsageCredentialOwner) {
    match provider {
        AiProvider::Anthropic => (HostSurfaceId::Claude, UsageCredentialOwner::Claude),
        AiProvider::OpenAi => (HostSurfaceId::Codex, UsageCredentialOwner::Codex),
        AiProvider::Amp => (HostSurfaceId::Amp, UsageCredentialOwner::Amp),
        AiProvider::Xai => (HostSurfaceId::Grok, UsageCredentialOwner::Grok),
        AiProvider::Opencode => (HostSurfaceId::OpenCode, UsageCredentialOwner::OpenCode),
        AiProvider::Moonshot => (HostSurfaceId::Kimi, UsageCredentialOwner::Kimi),
        AiProvider::Zai => (HostSurfaceId::Zai, UsageCredentialOwner::Zai),
        AiProvider::Minimax => (HostSurfaceId::Minimax, UsageCredentialOwner::Minimax),
        AiProvider::Google => (HostSurfaceId::Google, UsageCredentialOwner::Google),
        AiProvider::Cursor => (HostSurfaceId::Cursor, UsageCredentialOwner::Cursor),
        AiProvider::Meta => (HostSurfaceId::Meta, UsageCredentialOwner::Meta),
        AiProvider::OpenRouter => (HostSurfaceId::OpenRouter, UsageCredentialOwner::OpenRouter),
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
            CredentialSourceKey::Profile { agent, root } => DiscoveredCredentialSource::Profile {
                surface: candidate.surface,
                agent,
                root,
                operator_home: candidate.operator_home.unwrap_or_default(),
                account_label: candidate.account_label,
                source_id,
                capability_id,
                provenance: candidate.provenance,
            },
            CredentialSourceKey::Env {
                surface, handle, ..
            } => DiscoveredCredentialSource::Env {
                surface,
                handle,
                key: candidate.env_key.unwrap_or_default(),
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
        CredentialSourceKey::Profile { agent, root } => {
            format!("profile-v1:{}:{}", agent.slug(), root.to_string_lossy())
        }
        CredentialSourceKey::Env {
            surface,
            handle,
            key,
        } => {
            fn segment(value: &str) -> String {
                format!("{}:{value}", value.len())
            }
            format!(
                "env-v2:{}:{}:{}",
                surface.id(),
                segment(key),
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
        match crate::usage::read_claude_keychain_item(&scope.service) {
            #[cfg(any(target_os = "macos", test))]
            crate::usage::ClaudeKeychainRead::Payload { json } => {
                ProfileReadOutcome::Bytes(json.into_bytes())
            }
            crate::usage::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
            crate::usage::ClaudeKeychainRead::Missing => ProfileReadOutcome::Missing,
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
    Authenticated {
        provider_id: Option<String>,
        account_label: Option<String>,
        material: Option<Box<ProfileCredentialMaterial>>,
    },
    Anonymous(Option<Box<ProfileCredentialMaterial>>),
    Missing,
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
    // Provider-issued identities per surface, from any source form. An
    // anonymous env/key credential carries no identity evidence of its own;
    // when exactly one same-surface provider identity exists, the key joins
    // that canonical account instead of minting a source-scoped row.
    let mut strong = BTreeMap::<HostSurfaceId, BTreeSet<CanonicalAccountIdentity>>::new();
    for (surface, _, _, _, _, _, outcome) in &validated {
        if let ProfileValidation::Authenticated {
            provider_id: Some(id),
            ..
        } = outcome
            && !id.trim().is_empty()
        {
            strong
                .entry(*surface)
                .or_default()
                .insert(CanonicalAccountIdentity {
                    surface: *surface,
                    subject: CanonicalAccountSubject::ProviderId(id.trim().to_owned()),
                });
        }
    }
    let (primary, attachable): (Vec<ValidatedSourceParts>, Vec<ValidatedSourceParts>) = validated
        .into_iter()
        .partition(|parts| !is_attachable_env_source(&parts.5, &parts.6));
    // Strong sources accumulate first so canonical labels come from
    // authenticated evidence, never from an attached anonymous key.
    for parts in primary {
        accumulate_validated_source(parts, None, &mut diagnostics, &mut bindings, &mut accounts);
    }
    for parts in attachable {
        let attach_to = match strong.get(&parts.0) {
            Some(ids) if ids.len() == 1 => ids.iter().next().cloned(),
            _ => None,
        };
        accumulate_validated_source(
            parts,
            attach_to,
            &mut diagnostics,
            &mut bindings,
            &mut accounts,
        );
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

/// Whether an env/key source proved no identity of its own.
///
/// Anonymous API-key/OAuth-token credentials are bearer material without
/// local identity evidence. Unlike profiles (distinct local logins) and
/// forwarded capabilities (a separate trust domain), they may join the one
/// same-surface provider-authenticated account when it exists.
fn is_attachable_env_source(
    source: &ValidatedCredentialSource,
    outcome: &ProfileValidation,
) -> bool {
    if !matches!(source, ValidatedCredentialSource::Env { .. }) {
        return false;
    }
    match outcome {
        ProfileValidation::Authenticated { provider_id, .. } => {
            provider_id.as_deref().is_none_or(|id| id.trim().is_empty())
        }
        ProfileValidation::Anonymous(_) => true,
        ProfileValidation::Missing
        | ProfileValidation::Denied
        | ProfileValidation::ConsentRequired
        | ProfileValidation::Malformed => false,
    }
}

fn accumulate_validated_source(
    parts: ValidatedSourceParts,
    attach_to: Option<CanonicalAccountIdentity>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
    bindings: &mut Vec<ValidatedCredentialBinding>,
    accounts: &mut BTreeMap<CanonicalAccountIdentity, AccountAccumulator>,
) {
    let (surface, source_id, capability_id, credential_revision, provenance, source, outcome) =
        parts;
    if let Some(identity) = attach_to {
        let label = match &outcome {
            ProfileValidation::Authenticated {
                provider_id,
                account_label,
                ..
            } => account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let entry = accounts.entry(identity.clone()).or_insert_with(|| {
            // Unreachable: the strong target accumulates first and always
            // mints its account. The fallback keeps the merge total.
            AccountAccumulator {
                label,
                provenance: BTreeSet::new(),
                source_ids: BTreeSet::new(),
            }
        });
        entry.provenance.extend(provenance.iter().cloned());
        entry.source_ids.insert(source_id.clone());
        bindings.push(ValidatedCredentialBinding {
            surface,
            identity: Some(identity),
            source_id,
            capability_id,
            credential_revision,
            provenance,
            source,
        });
        return;
    }

    match outcome {
        ProfileValidation::Authenticated {
            provider_id,
            account_label,
            material: _,
        } => {
            let subject = provider_id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .map(|id| CanonicalAccountSubject::ProviderId(id.trim().to_owned()))
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
                    provenance,
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
                provenance,
                source,
            });
        }
        ProfileValidation::Anonymous(_) => bindings.push(ValidatedCredentialBinding {
            surface,
            identity: None,
            source_id,
            capability_id,
            credential_revision,
            provenance,
            source,
        }),
        ProfileValidation::Missing => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::CredentialMissing,
        )),
        ProfileValidation::Denied => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::CredentialDenied,
        )),
        ProfileValidation::ConsentRequired => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::KeychainConsentRequired,
        )),
        ProfileValidation::Malformed => diagnostics.push(source_diagnostic(
            surface,
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
    ValidatedCredentialSource,
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
            root,
            operator_home,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let outcome = profile_identity(profile_reader, agent, &root, &operator_home);
            let credential_revision =
                profile_credential_revision(profile_reader, agent, &root, &operator_home);
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
            let outcome = match outcome {
                ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label,
                    material,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material,
                },
                ProfileValidation::Anonymous(_) => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        outcome
                    }
                }
                other => other,
            };
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                source,
                outcome,
            )
        }
        DiscoveredCredentialSource::Env {
            surface,
            handle,
            key,
            kind: _,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let material = env_resolver.source_material(surface, &key, &handle);
            let outcome = match env_resolver.identify_provider_credential(surface, &handle) {
                ProviderCredentialIdentityOutcome::Authenticated {
                    provider_id,
                    account_label: auth_label,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material: None,
                },
                ProviderCredentialIdentityOutcome::Anonymous => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        ProfileValidation::Anonymous(None)
                    }
                }
                ProviderCredentialIdentityOutcome::Missing => ProfileValidation::Missing,
                ProviderCredentialIdentityOutcome::Denied => ProfileValidation::Denied,
                ProviderCredentialIdentityOutcome::Malformed => ProfileValidation::Malformed,
            };
            let credential_revision =
                opaque_credential_revision(&format!("env:{}:{}:{}", surface.id(), key, handle.0));
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                ValidatedCredentialSource::Env {
                    handle,
                    key,
                    material,
                },
                outcome,
            )
        }
        DiscoveredCredentialSource::Capability {
            surface,
            account_label,
            source_id,
            capability_id,
        } => {
            let provenance = BTreeSet::from(["forwarded to Capsule".to_owned()]);
            let outcome = account_label.map_or(ProfileValidation::Anonymous(None), |label| {
                ProfileValidation::Authenticated {
                    provider_id: None,
                    account_label: Some(label),
                    material: None,
                }
            });
            (
                surface,
                source_id,
                capability_id.clone(),
                opaque_credential_revision(&format!("capability:{capability_id}")),
                provenance,
                ValidatedCredentialSource::Capability,
                outcome,
            )
        }
    }
}

fn source_diagnostic(
    surface: HostSurfaceId,
    provenance: &BTreeSet<String>,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: provenance.iter().cloned().collect::<Vec<_>>().join(", "),
        issue,
    }
}

/// Return an opaque revision for the complete credential material read for a
/// profile source. The path-derived source id is intentionally not enough:
/// providers frequently rotate tokens in place without changing the profile
/// path or account identity.
fn profile_credential_revision(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
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
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            file("amp.secrets", path);
        }
        Agent::Kimi => file("kimi.credentials", root.join("credentials/kimi-code.json")),
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
        ProfileReadOutcome::ConsentRequired => {
            evidence.push(format!("{label}:consent-required"));
        }
    }
}

fn opaque_credential_revision(evidence: &str) -> String {
    let hashed = jackin_core::account_key_hash("usage-credential-material-v2", evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

fn profile_identity(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    match agent {
        Agent::Claude => claude_profile_identity(reader, root, operator_home),
        Agent::Codex => codex_profile_identity(reader, &root.join("auth.json")),
        Agent::Amp => {
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            amp_profile_identity(reader, &path)
        }
        Agent::Kimi => {
            let value = match read_json(reader, &root.join("credentials/kimi-code.json")) {
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
/// discovery and the path is kept as refresh material, so refresh re-reads
/// the registered root instead of a stale discovery-time copy. A
/// present-but-tokenless `auth.json` is malformed, never an anonymous
/// binding refresh cannot serve.
fn cursor_profile_identity(reader: &dyn ProfileCredentialReader, root: &Path) -> ProfileValidation {
    let auth_path = root.join("auth.json");
    let value = match read_json(reader, &auth_path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    if crate::usage::cursor_auth_from_value(&value).is_none() {
        return ProfileValidation::Malformed;
    }
    let material = Some(Box::new(ProfileCredentialMaterial::Cursor { auth_path }));
    let label = read_json(reader, &root.join("cli-config.json"))
        .ok()
        .flatten()
        .and_then(|config| crate::usage::cursor_cli_identity_from_value(&config));
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
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::OpenCode {
                auth_path: path.to_path_buf(),
            })))
        }
    }
}

fn claude_profile_identity(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    let mut paths = vec![root.join(".credentials.json"), root.join(".claude.json")];
    if root == operator_home.join(".claude") {
        paths.push(operator_home.join(".claude.json"));
    }
    let mut credential = None;
    let mut account_label = None;
    let mut organization_type = None;
    for path in paths {
        match read_json(reader, &path) {
            Ok(Some(value)) => {
                if credential.is_none() {
                    credential = crate::usage::claude_oauth_from_value(&value);
                }
                if account_label.is_none() {
                    account_label = crate::usage::claude_email_from_value(&value);
                }
                if organization_type.is_none() {
                    organization_type = crate::usage::claude_organization_type_from_value(&value);
                }
            }
            Ok(None) => {}
            Err(ProfileValidation::Denied) => return ProfileValidation::Denied,
            Err(ProfileValidation::ConsentRequired) => return ProfileValidation::ConsentRequired,
            Err(_) => return ProfileValidation::Malformed,
        }
    }
    if let Some(credential) = credential {
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
        return account_label.map_or(ProfileValidation::Anonymous(material.clone()), |label| {
            ProfileValidation::Authenticated {
                provider_id: None,
                account_label: Some(label),
                material,
            }
        });
    }
    let current_dir = operator_home;
    let Some(scope) = jackin_core::claude_keychain_scope(root, operator_home, current_dir) else {
        return ProfileValidation::Malformed;
    };
    match reader.read_claude_keychain(&scope) {
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
            let Some(credential) = crate::usage::claude_oauth_from_value(&value) else {
                return ProfileValidation::Malformed;
            };
            let account_label = crate::usage::claude_email_from_value(&value);
            let is_anonymous = account_label.is_none() && credential.refresh_token.is_none();
            let material = Some(Box::new(ProfileCredentialMaterial::Claude(
                crate::usage::ClaudeResolved {
                    access_token: credential.access_token,
                    subscription_type: credential.subscription_type,
                    account_email: account_label.clone(),
                    organization_type: crate::usage::claude_organization_type_from_value(&value),
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
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
    }
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
        root: path.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
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
    let Some(object) = value.as_object() else {
        return ProfileValidation::Malformed;
    };
    let labeled = object.iter().find_map(|(key, value)| {
        let label = key.strip_prefix("apiKey@")?.trim();
        let secret = value.as_str()?.trim();
        (!label.is_empty() && !secret.is_empty()).then(|| (label.to_owned(), secret.to_owned()))
    });
    let fallback_key = object.values().find_map(|value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
            .map(str::to_owned)
    });
    let Some(key) = labeled
        .as_ref()
        .map(|(_, key)| key.clone())
        .or(fallback_key)
    else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Amp { key }));
    labeled.map_or(
        ProfileValidation::Anonymous(material.clone()),
        |(label, _)| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

fn grok_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Grok {
        auth_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_id", "team_id"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
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
        ValidatedCredentialSource::Env { handle, key, .. } => {
            return env_resolver.refresh_provider_credential(binding.surface, key, handle);
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
            crate::usage::claude_view_from_wave_with_rate_limit(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
                crate::usage::ClaudeWaveResolution::Resolved(Box::new(resolved.clone())),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex {
            credentials,
            root,
        }) => crate::usage::codex_profile_snapshot_with_rate_limit(
            binding.surface.agent_slug(),
            credentials,
            root,
            chrono::Utc::now().timestamp(),
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) => (
            crate::usage::amp_api_key_snapshot(
                binding.surface.agent_slug(),
                key,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Grok { auth_path }) => {
            let now = chrono::Utc::now().timestamp();
            let result = crate::usage::fetch_grok_rest_billing(auth_path, now)
                .map(|response| crate::usage::GrokBillingSnapshot::Rest(Box::new(response)));
            (
                crate::usage::grok_snapshot_from_rpc_result(
                    binding.surface.agent_slug(),
                    now,
                    auth_path,
                    true,
                    false,
                    false,
                    result,
                ),
                None,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Kimi { token }) => {
            let now = chrono::Utc::now().timestamp();
            (
                crate::usage::kimi_snapshot(
                    binding.surface.agent_slug(),
                    Some(token.as_str()),
                    now,
                ),
                None,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::OpenCode { auth_path }) => (
            crate::usage::opencode_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor { auth_path }) => (
            crate::usage::cursor_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
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
