// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Config precedence, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use jackin_config::{AppConfig, ConfigSourceIssue, ReadOnlyConfigSnapshot};
use jackin_core::{
    Agent, AuthForwardMode, JackinPaths, UsageCredentialEnvName, UsageCredentialOwner,
    WorkspaceName,
};
use jackin_protocol::control::FocusedUsageView;

use super::{
    CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId, HostUsageRuntime,
    discovered_account_keys,
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
            env_vars: "ZAI_API_KEY, Z_AI_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "minimax",
            host_paths: "",
            env_vars: "MINIMAX_CODING_API_KEY, MINIMAX_API_KEY",
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
    Snapshot(Box<FocusedUsageView>),
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
    /// Rust-composed scope label (`default host profile`, `workspace …`).
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
    pub provenance: BTreeSet<String>,
    pub source: ValidatedCredentialSource,
}

#[derive(Clone)]
pub(super) enum ValidatedCredentialSource {
    Profile(ProfileCredentialMaterial),
    Env {
        handle: OpaqueCredentialHandle,
        key: String,
    },
    Capability,
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
        root: PathBuf,
    },
    OpenCode {
        auth_path: PathBuf,
    },
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
        source_id: String,
        capability_id: String,
        provenance: BTreeSet<String>,
    },
    Env {
        surface: HostSurfaceId,
        handle: OpaqueCredentialHandle,
        key: String,
        kind: UsageCredentialKind,
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

struct EffectiveScope {
    workspace: Option<WorkspaceName>,
    role: Option<String>,
    label: String,
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
    let scopes = effective_scopes(&snapshot.config);
    let mut candidates = BTreeMap::<CredentialSourceKey, CandidateAccumulator>::new();

    for effective in scopes {
        enumerate_profile_candidates(&snapshot.config, operator_home, &effective, &mut candidates);
        enumerate_env_candidates(
            &snapshot.config,
            &effective,
            env_resolver,
            &mut candidates,
            &mut diagnostics,
        );
    }

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
        if !HostSurfaceId::DESKTOP_PROVIDER_ORDER.contains(&surface) {
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

fn effective_scopes(config: &AppConfig) -> Vec<EffectiveScope> {
    let mut scopes = vec![EffectiveScope {
        workspace: None,
        role: None,
        label: "default host profile".to_owned(),
    }];
    for role in config.roles.keys() {
        scopes.push(EffectiveScope {
            workspace: None,
            role: Some(role.clone()),
            label: format!("role {role}"),
        });
    }
    for (workspace_name, workspace) in &config.workspaces {
        let Ok(parsed) = WorkspaceName::parse(workspace_name) else {
            continue;
        };
        scopes.push(EffectiveScope {
            workspace: Some(parsed.clone()),
            role: None,
            label: format!("workspace {workspace_name}"),
        });
        let mut roles = workspace.roles.keys().cloned().collect::<BTreeSet<_>>();
        if workspace.allowed_roles.is_empty() {
            roles.extend(config.roles.keys().cloned());
        } else {
            roles.extend(workspace.allowed_roles.iter().cloned());
        }
        for role in roles {
            scopes.push(EffectiveScope {
                workspace: Some(parsed.clone()),
                role: Some(role.clone()),
                label: format!("workspace {workspace_name} role {role}"),
            });
        }
    }
    scopes
}

fn enumerate_profile_candidates(
    config: &AppConfig,
    operator_home: &Path,
    scope: &EffectiveScope,
    candidates: &mut BTreeMap<CredentialSourceKey, CandidateAccumulator>,
) {
    for agent in Agent::ALL.iter().copied() {
        let role = scope.role.as_deref().unwrap_or("");
        let mode = jackin_config::resolve_mode(config, agent, scope.workspace.as_ref(), role);
        if mode != AuthForwardMode::Sync {
            continue;
        }
        let configured =
            jackin_config::resolve_sync_source_dir(config, agent, scope.workspace.as_ref(), role);
        let root = configured.map_or_else(
            || {
                if agent == Agent::Opencode {
                    std::env::var_os("XDG_DATA_HOME")
                        .map_or_else(|| operator_home.join(".local/share"), PathBuf::from)
                        .join("opencode")
                } else {
                    operator_home.join(agent.runtime().state_paths().credential_dir)
                }
            },
            |path| resolve_profile_root(operator_home, &path),
        );
        if agent == Agent::Opencode && !root.join("auth.json").is_file() {
            continue;
        }
        let key = CredentialSourceKey::Profile { agent, root };
        candidates
            .entry(key)
            .and_modify(|candidate| {
                candidate.provenance.insert(scope.label.clone());
            })
            .or_insert_with(|| CandidateAccumulator {
                surface: HostSurfaceId::from_agent(agent),
                kind: UsageCredentialKind::Profile,
                provenance: BTreeSet::from([scope.label.clone()]),
                env_key: None,
                account_label: None,
                operator_home: Some(operator_home.to_path_buf()),
            });
    }
}

fn enumerate_env_candidates(
    config: &AppConfig,
    scope: &EffectiveScope,
    resolver: &dyn ProviderCredentialEnvResolver,
    candidates: &mut BTreeMap<CredentialSourceKey, CandidateAccumulator>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
) {
    let governed = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
        .iter()
        .copied()
        .filter(|entry| entry.owner != UsageCredentialOwner::OpenCode)
        .filter(|entry| owner_allowed_in_scope(config, scope, entry.owner))
        .collect::<Vec<_>>();
    let resolutions = resolver.resolve_provider_credentials(
        config,
        scope.workspace.as_ref(),
        scope.role.as_deref(),
        &governed,
    );
    for resolution in resolutions {
        let Some(entry) = governed.iter().find(|entry| entry.name == resolution.key) else {
            continue;
        };
        let Some(surface) = surface_for_owner(entry.owner) else {
            continue;
        };
        match resolution.outcome {
            ProviderCredentialEnvOutcome::Resolved(handle) => {
                let kind = if entry.name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME {
                    UsageCredentialKind::OAuthToken
                } else {
                    UsageCredentialKind::ApiKey
                };
                let key = CredentialSourceKey::Env {
                    surface,
                    handle: handle.clone(),
                    key: resolution.key.clone(),
                };
                candidates
                    .entry(key)
                    .and_modify(|candidate| {
                        candidate.provenance.insert(scope.label.clone());
                    })
                    .or_insert_with(|| CandidateAccumulator {
                        surface,
                        kind,
                        provenance: BTreeSet::from([scope.label.clone()]),
                        env_key: Some(resolution.key.clone()),
                        account_label: None,
                        operator_home: None,
                    });
            }
            ProviderCredentialEnvOutcome::Missing => diagnostics.push(env_diagnostic(
                surface,
                scope,
                UsageDiscoveryIssue::CredentialMissing,
            )),
            ProviderCredentialEnvOutcome::Denied => diagnostics.push(env_diagnostic(
                surface,
                scope,
                UsageDiscoveryIssue::CredentialDenied,
            )),
            ProviderCredentialEnvOutcome::Malformed => diagnostics.push(env_diagnostic(
                surface,
                scope,
                UsageDiscoveryIssue::CredentialMalformed,
            )),
            ProviderCredentialEnvOutcome::InteractionRequired => diagnostics.push(env_diagnostic(
                surface,
                scope,
                UsageDiscoveryIssue::InteractionRequired,
            )),
        }
    }
}

fn owner_allowed_in_scope(
    config: &AppConfig,
    scope: &EffectiveScope,
    owner: UsageCredentialOwner,
) -> bool {
    let agent = match owner {
        UsageCredentialOwner::Claude => Some(Agent::Claude),
        UsageCredentialOwner::Codex => Some(Agent::Codex),
        UsageCredentialOwner::Amp => Some(Agent::Amp),
        UsageCredentialOwner::Kimi => Some(Agent::Kimi),
        UsageCredentialOwner::Grok => Some(Agent::Grok),
        UsageCredentialOwner::OpenCode => Some(Agent::Opencode),
        UsageCredentialOwner::Zai | UsageCredentialOwner::Minimax => None,
    };
    agent.is_none_or(|agent| {
        jackin_config::resolve_mode(
            config,
            agent,
            scope.workspace.as_ref(),
            scope.role.as_deref().unwrap_or(""),
        ) != AuthForwardMode::Ignore
    })
}

fn surface_for_owner(owner: UsageCredentialOwner) -> Option<HostSurfaceId> {
    match owner {
        UsageCredentialOwner::Claude => Some(HostSurfaceId::Claude),
        UsageCredentialOwner::Codex => Some(HostSurfaceId::Codex),
        UsageCredentialOwner::Amp => Some(HostSurfaceId::Amp),
        UsageCredentialOwner::Kimi => Some(HostSurfaceId::Kimi),
        UsageCredentialOwner::Grok => Some(HostSurfaceId::Grok),
        UsageCredentialOwner::Zai => Some(HostSurfaceId::Zai),
        UsageCredentialOwner::Minimax => Some(HostSurfaceId::Minimax),
        UsageCredentialOwner::OpenCode => None,
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

fn env_diagnostic(
    surface: HostSurfaceId,
    scope: &EffectiveScope,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: scope.label.clone(),
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
        CredentialSourceKey::Env { surface, key, .. } => {
            format!("env-v1:{}:{key}", surface.id())
        }
        CredentialSourceKey::Capability { .. } => unreachable!("returned above"),
    };
    let hashed = jackin_core::account_key_hash(surface.id(), &evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

enum ProfileReadOutcome {
    Bytes(Vec<u8>),
    Missing,
    Denied,
}

trait ProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome;
    fn exists(&self, path: &Path) -> bool;
    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome;
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

    for source in catalog.sources {
        let (surface, source_id, capability_id, provenance, source, outcome) =
            validate_source(source, env_resolver, profile_reader);

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
                            .map(|label| {
                                CanonicalAccountSubject::ProviderStableHandle(
                                    label.trim().to_owned(),
                                )
                            })
                    });
                let Some(subject) = subject else {
                    bindings.push(ValidatedCredentialBinding {
                        surface,
                        identity: None,
                        source_id,
                        capability_id,
                        provenance,
                        source,
                    });
                    continue;
                };
                let identity = CanonicalAccountIdentity { surface, subject };
                let label = account_label
                    .as_deref()
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_owned)
                    .or_else(|| provider_id.clone())
                    .unwrap_or_default();
                let entry =
                    accounts
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
                    provenance,
                    source,
                });
            }
            ProfileValidation::Anonymous(_) => bindings.push(ValidatedCredentialBinding {
                surface,
                identity: None,
                source_id,
                capability_id,
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
            ProfileValidation::Malformed => diagnostics.push(source_diagnostic(
                surface,
                &provenance,
                UsageDiscoveryIssue::CredentialMalformed,
            )),
        }
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

type ValidatedSourceParts = (
    HostSurfaceId,
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
            source_id,
            capability_id,
            provenance,
        } => {
            let outcome = profile_identity(profile_reader, agent, &root, &operator_home);
            let source = match &outcome {
                ProfileValidation::Authenticated { material, .. }
                | ProfileValidation::Anonymous(material) => material
                    .clone()
                    .map_or(ValidatedCredentialSource::Capability, |material| {
                        ValidatedCredentialSource::Profile(*material)
                    }),
                _ => ValidatedCredentialSource::Capability,
            };
            (
                surface,
                source_id,
                capability_id,
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
            source_id,
            capability_id,
            provenance,
        } => {
            let outcome = match env_resolver.identify_provider_credential(surface, &handle) {
                ProviderCredentialIdentityOutcome::Authenticated {
                    provider_id,
                    account_label,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label,
                    material: None,
                },
                ProviderCredentialIdentityOutcome::Anonymous => ProfileValidation::Anonymous(None),
                ProviderCredentialIdentityOutcome::Missing => ProfileValidation::Missing,
                ProviderCredentialIdentityOutcome::Denied => ProfileValidation::Denied,
                ProviderCredentialIdentityOutcome::Malformed => ProfileValidation::Malformed,
            };
            (
                surface,
                source_id,
                capability_id,
                provenance,
                ValidatedCredentialSource::Env { handle, key },
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
                capability_id,
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

fn profile_identity(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    match agent {
        Agent::Claude => claude_profile_identity(reader, root, operator_home),
        Agent::Codex => codex_profile_identity(reader, &root.join("auth.json")),
        Agent::Amp => amp_profile_identity(reader, &root.join("secrets.json")),
        Agent::Kimi => {
            if reader.exists(root) {
                ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Kimi {
                    root: root.to_path_buf(),
                })))
            } else {
                ProfileValidation::Missing
            }
        }
        Agent::Grok => grok_profile_identity(reader, &root.join("auth.json")),
        Agent::Opencode => opencode_profile_identity(reader, &root.join("auth.json")),
    }
}

fn opencode_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    match reader.read(path) {
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
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
    let view = match &binding.source {
        ValidatedCredentialSource::Env { handle, key } => {
            return env_resolver.refresh_provider_credential(binding.surface, key, handle);
        }
        ValidatedCredentialSource::Capability => {
            return ProviderCredentialRefreshOutcome::Malformed;
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(resolved)) => {
            crate::usage::claude_view_from_wave(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
                crate::usage::ClaudeWaveResolution::Resolved(Box::new(resolved.clone())),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex {
            credentials,
            root,
        }) => crate::usage::codex_profile_snapshot(
            binding.surface.agent_slug(),
            credentials,
            root,
            chrono::Utc::now().timestamp(),
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) => {
            crate::usage::amp_api_key_snapshot(
                binding.surface.agent_slug(),
                key,
                chrono::Utc::now().timestamp(),
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Grok { auth_path }) => {
            let now = chrono::Utc::now().timestamp();
            let result = crate::usage::fetch_grok_rest_billing(auth_path, now)
                .map(|response| crate::usage::GrokBillingSnapshot::Rest(Box::new(response)));
            crate::usage::grok_snapshot_from_rpc_result(
                binding.surface.agent_slug(),
                now,
                auth_path,
                true,
                false,
                false,
                result,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Kimi { root }) => {
            let now = chrono::Utc::now().timestamp();
            let token = crate::usage::load_kimi_local_token_from_home(root, now);
            crate::usage::kimi_snapshot(binding.surface.agent_slug(), token.as_deref(), now)
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::OpenCode { auth_path }) => {
            crate::usage::opencode_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            )
        }
    };
    ProviderCredentialRefreshOutcome::Snapshot(Box::new(view))
}

impl HostUsageRuntime {
    /// Rescan the retained Rust discovery scope without dispatching provider probes.
    ///
    /// This is the manual-refresh reconciliation boundary used before broker
    /// capabilities are rebuilt. The prior validated generation remains usable
    /// when the read-only config scan is unavailable.
    pub fn reconcile_discovery(
        &mut self,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<bool, String> {
        self.require_open()?;
        let Some(scope) = self.discovery_scope.clone() else {
            return Ok(false);
        };
        resolver.begin_manual_retry();
        let Ok(catalog) = discover_usage_sources(&scope, resolver) else {
            self.push_event(
                "discovery_failed",
                None,
                Some("current account discovery unavailable".to_owned()),
            );
            return Ok(false);
        };
        let discovered = validate_usage_sources(catalog, resolver);
        let changed = self
            .discovery
            .as_ref()
            .map(|current| &current.config_generation)
            != Some(&discovered.config_generation);
        self.discovery = Some(discovered);
        if changed {
            let current = discovered_account_keys(self.discovery.as_ref());
            self.discovered_views.retain(|key, _| current.contains(key));
        }
        self.push_event(
            "discovery_reconciled",
            None,
            Some(if changed { "changed" } else { "unchanged" }.to_owned()),
        );
        Ok(changed)
    }

    pub(super) fn record_discovered_snapshot(
        &mut self,
        binding: &ValidatedCredentialBinding,
        mut view: FocusedUsageView,
    ) {
        let identity = binding
            .identity
            .clone()
            .or_else(|| CanonicalAccountIdentity::from_view(binding.surface, &view));
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
