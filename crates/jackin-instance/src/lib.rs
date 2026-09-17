//! jackin-instance: instance naming, manifests, and lifecycle records.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`InstanceManifest`] — on-disk instance record.

use anyhow::Context;
use jackin_config::{AuthForwardMode, GithubAuthMode};
use jackin_core::JackinPaths;
use jackin_manifest::RoleManifest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod auth;
pub use auth::validate_sync_source_dir;
mod error;
pub use error::{InstanceError, SyncSourceValidationError};
pub mod manifest;
pub mod naming;
mod process_telemetry;
pub use manifest::{
    AdmittedInstance, AppleContainerResources, BackendResources, DockerResources, InstanceIndex,
    InstanceIndexEntry, InstanceManifest, InstanceQuery, InstanceStatus, NewInstanceManifest,
    SessionRecord, SessionStatus,
};
pub use naming::{class_family_matches, container_name_with_id, new_container_name, runtime_slug};

/// Outcome of the `.claude.json` provisioning step, so callers can surface
/// a one-time notice when host credentials are forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvisionOutcome {
    /// No host auth was forwarded (ignore mode).
    Skipped,
    /// Host auth was synced (overwritten) into the container state.
    Synced,
    /// Mode would have forwarded, but host file was missing — wrote `{}`.
    HostMissing,
    /// Token mode: empty `.claude.json`, no `.credentials.json` —
    /// Claude Code inside the container uses `CLAUDE_CODE_OAUTH_TOKEN`
    /// from the resolved env.
    TokenMode,
}

impl AuthProvisionOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Synced => "synced",
            Self::HostMissing => "host_missing",
            Self::TokenMode => "token_mode",
        }
    }
}

/// Outcome of the `[github]` provisioning step.
///
/// Mirrors [`AuthProvisionOutcome`] but carries the resolved token
/// inline on the variants that produce one, so callers cannot construct
/// inconsistent `(outcome, token)` pairs (e.g. `(Skipped, Some(t))`).
/// Variant-level data also keeps the launch-summary renderer self-
/// describing without consulting a parallel `Option<String>` field.
#[derive(Clone, PartialEq, Eq)]
pub enum GithubProvisionOutcome {
    /// `auth_forward = sync` and the host token resolved — `hosts.yml`
    /// was materialized in the role-state directory. `token` is the
    /// resolved value, also exported as `GH_TOKEN` / `GITHUB_TOKEN`.
    /// `source` distinguishes which host path produced the token so the
    /// launch-summary line can attribute it accurately and operators can
    /// spot drift between the live `gh` CLI and a stale on-disk file.
    Synced {
        token: String,
        source: GithubTokenSource,
    },
    /// `auth_forward = sync` but neither `gh auth token` nor the host's
    /// `~/.config/gh/hosts.yml` produced a usable token. Any pre-existing
    /// in-container login is preserved. `reason` carries the typed
    /// signal so the notice can render the actual cause instead of
    /// guessing "host logged out".
    HostMissing { reason: HostMissingReason },
    /// `auth_forward = token`. No file mount; the launcher exports
    /// `token` as `GH_TOKEN` (and `GITHUB_TOKEN`) into the container env.
    TokenMode { token: String },
    /// `auth_forward = ignore`. Any prior `hosts.yml` was wiped; no env
    /// is exported.
    Skipped,
}

/// Which host path produced the synced token, surfaced in the launch
/// summary so the operator can audit the source on every container
/// start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubTokenSource {
    /// `GH_TOKEN` resolved from operator-configured `[github.env]` declarations
    /// before consulting the host `gh` CLI or `hosts.yml`.
    ConfiguredEnv,
    /// `gh auth token --hostname github.com` (live, Keychain-aware).
    GhCli,
    /// Direct parse of `~/.config/gh/hosts.yml` (file fallback used when
    /// `gh` isn't on PATH or when the CLI did not return a token).
    HostsFile,
}

/// Why `Sync` mode fell through to `HostMissing` — surfaced in the
/// launch-summary notice so the operator sees the real cause instead of
/// the "host logged out" catch-all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMissingReason {
    /// `gh` is not installed or the `host_home` is a hermetic-test path
    /// AND `~/.config/gh/hosts.yml` does not exist on the host. The
    /// closest match for "host logged out".
    NoGhAndNoHostsFile,
    /// `gh auth token` exited non-zero. The token (if any) is unusable
    /// — the operator's `gh` thinks it's broken (expired, revoked, or
    /// `gh auth login` never ran).
    GhCliFailed { stderr: String },
    /// `gh auth token` exited zero but printed no token. Same broken
    /// signal as `GhCliFailed`, different surface.
    GhCliEmpty,
    /// `~/.config/gh/hosts.yml` exists but `parse_gh_hosts_yml` did not
    /// extract a non-empty token from a `github.com` block.
    HostsFileMalformed,
}

// `Debug` is implemented manually to redact the token in `Synced` /
// `TokenMode` so the value never lands in diagnostic telemetry,
// `eprintln!("{state:?}")`, or panic backtrace.
impl std::fmt::Debug for GithubProvisionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synced { source, .. } => f
                .debug_struct("Synced")
                .field("token", &"<redacted>")
                .field("source", source)
                .finish(),
            Self::HostMissing { reason } => f
                .debug_struct("HostMissing")
                .field("reason", reason)
                .finish(),
            Self::TokenMode { .. } => f
                .debug_struct("TokenMode")
                .field("token", &"<redacted>")
                .finish(),
            Self::Skipped => f.debug_struct("Skipped").finish(),
        }
    }
}

impl GithubProvisionOutcome {
    /// Resolved token to export as `GH_TOKEN` / `GITHUB_TOKEN`, derived
    /// from the variant. `Some` for `Synced` and `TokenMode`; `None` for
    /// `HostMissing` and `Skipped`. Callers no longer have to consult a
    /// parallel `Option<String>` field on `RoleState`.
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Synced { token, .. } | Self::TokenMode { token } => Some(token),
            Self::HostMissing { .. } | Self::Skipped => None,
        }
    }

    /// Short-form discriminator used by tests and structured logs that
    /// need a `Copy` tag without the credentials.
    #[must_use]
    pub const fn kind(&self) -> GithubProvisionKind {
        match self {
            Self::Synced { .. } => GithubProvisionKind::Synced,
            Self::HostMissing { .. } => GithubProvisionKind::HostMissing,
            Self::TokenMode { .. } => GithubProvisionKind::TokenMode,
            Self::Skipped => GithubProvisionKind::Skipped,
        }
    }
}

/// Token-free discriminator for `GithubProvisionOutcome`. Useful in
/// assertions and pattern-matches where the token value is irrelevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubProvisionKind {
    Synced,
    HostMissing,
    TokenMode,
    Skipped,
}

/// Runtime state for the selected agent (identity + model override).
///
/// Collapsed from a 5-variant enum to a single struct.
/// Auth paths for provisioned agents are tracked separately on
/// [`ProvisionedAuth`]. The launch path provisions auth state for every agent
/// in `manifest.supported_agents()` so each agent's home directory is
/// bind-mounted at `docker run` and sibling tabs can authenticate without
/// re-launching.
#[derive(Debug, Clone)]
pub struct AgentRuntimeState {
    /// The selected agent for this session.
    pub agent: jackin_core::Agent,
    /// Optional model override from the role manifest (`None` = agent default).
    pub model: Option<String>,
}

/// Auth state provisioned for a single launch instance.
///
/// One entry per instance binding (agent + account + mode): two
/// instances of the same agent provision independently and merge as
/// separate entries in [`ProvisionedAuth::slots`].
///
/// `credential_paths` carries the host paths the launcher may
/// bind-mount into the container. Its shape is fixed per agent:
/// - Claude: `[account.json, credentials.json]`, always present —
///   pair with `forward_auth` plus an existence check at mount time.
///   `forward_auth` is `true` only for modes that mount real
///   credential files (`Sync` / `OAuthToken`); `ApiKey` and `Ignore`
///   wipe the role-state credential files and do not mount them.
/// - Kimi / Hermes: `[<role-state dir>]`, always present — pair with
///   `forward_auth`, which is `true` only when the directory holds
///   synced credentials worth mounting.
/// - Every other agent: the single credential file iff provisioning
///   decided it should mount (empty after a wipe, or after a
///   host-missing run with no prior file). For these agents
///   `forward_auth` is exactly `!credential_paths.is_empty()`.
///
/// `home_dir` is the agent home provisioned under the instance root
/// (`<container>/home/...`); it is `None` when the lazy ignore path
/// skipped all filesystem work.
#[derive(Debug, Clone)]
pub struct ProvisionedInstanceAuth {
    pub agent: jackin_core::Agent,
    pub account_id: String,
    pub mode: AuthForwardMode,
    pub home_dir: Option<PathBuf>,
    pub credential_paths: Vec<PathBuf>,
    pub forward_auth: bool,
}

impl ProvisionedInstanceAuth {
    fn new(
        binding: &InstanceAuthBinding,
        home_dir: Option<PathBuf>,
        credential_paths: Vec<PathBuf>,
        forward_auth: bool,
    ) -> Self {
        Self {
            agent: binding.agent,
            account_id: binding.account_id.clone(),
            mode: binding.mode,
            home_dir,
            credential_paths,
            forward_auth,
        }
    }
}

/// Auth state provisioned for a launch, keyed by instance key.
///
/// An entry exists iff its instance binding was included in the
/// caller's provision list and the corresponding preparation step ran.
#[derive(Debug, Clone, Default)]
pub struct ProvisionedAuth {
    pub slots: BTreeMap<String /*instance key*/, ProvisionedInstanceAuth>,
}

impl ProvisionedAuth {
    /// Synthesize the [`ProvisionedAuth::slots`] key for one
    /// account/agent pair: `{account-id}@{agent-slug}`. Shares the
    /// convention with `ResolvedInstance::config_id` in
    /// `jackin-config`; explicit config ids replace the synthesized
    /// key once threaded through the pipeline.
    #[must_use]
    pub fn instance_key(account_id: &str, agent: jackin_core::Agent) -> String {
        format!("{account_id}@{}", agent.slug())
    }

    /// First slot provisioned for `agent` in key order, if any.
    /// Single-instance launches hold at most one slot per agent, so
    /// this is the whole story there; multi-instance callers iterate
    /// [`ProvisionedAuth::slots`] and filter by [`ProvisionedInstanceAuth::agent`]
    /// instead.
    #[must_use]
    pub fn for_agent(&self, agent: jackin_core::Agent) -> Option<&ProvisionedInstanceAuth> {
        self.slots.values().find(|slot| slot.agent == agent)
    }
}

/// One instance's auth-provisioning request: which agent, which
/// account, which forward mode, and where sync-mode credentials come
/// from.
#[derive(Debug, Clone)]
pub struct InstanceAuthBinding {
    /// [`ProvisionedAuth::slots`] key: an explicit config id, or the
    /// `{account-id}@{agent-slug}` synthesis from
    /// [`ProvisionedAuth::instance_key`].
    pub key: String,
    pub agent: jackin_core::Agent,
    pub account_id: String,
    pub mode: AuthForwardMode,
    pub sync_source_dir: Option<PathBuf>,
}

impl InstanceAuthBinding {
    /// Bind one account/agent pair with a synthesized instance key.
    /// Overwrite `.key` afterwards when an explicit config id exists.
    #[must_use]
    pub fn new(
        account_id: impl Into<String>,
        agent: jackin_core::Agent,
        mode: AuthForwardMode,
        sync_source_dir: Option<PathBuf>,
    ) -> Self {
        let account_id = account_id.into();
        let key = ProvisionedAuth::instance_key(&account_id, agent);
        Self {
            key,
            agent,
            account_id,
            mode,
            sync_source_dir,
        }
    }
}

/// Placeholder account id used until config ids are threaded through
/// the provisioning pipeline (a later pass wires `ResolvedInstance`
/// through [`RoleState::prepare_for_bindings`]).
const DEFAULT_ACCOUNT_ID: &str = "default";

struct AgentAuthProvision {
    key: String,
    auth: ProvisionedInstanceAuth,
    outcome: AuthProvisionOutcome,
}

fn emit_agent_auth_provision(
    agent: jackin_core::Agent,
    mode: AuthForwardMode,
    result: Result<AuthProvisionOutcome, jackin_telemetry::schema::enums::ErrorType>,
) {
    use jackin_telemetry::{Attr, FieldSet, Value, event, schema};

    let auth_mode = match mode {
        AuthForwardMode::Sync => schema::enums::AuthMode::Sync,
        AuthForwardMode::ApiKey => schema::enums::AuthMode::ApiKey,
        AuthForwardMode::OAuthToken => schema::enums::AuthMode::OauthToken,
        AuthForwardMode::Ignore => schema::enums::AuthMode::Ignore,
    };
    let configured_source = match mode {
        AuthForwardMode::Sync => schema::enums::CredentialSourceType::AgentHome,
        AuthForwardMode::ApiKey | AuthForwardMode::OAuthToken => {
            schema::enums::CredentialSourceType::Environment
        }
        AuthForwardMode::Ignore => schema::enums::CredentialSourceType::None,
    };
    let (source, outcome, error_type) = match result {
        Ok(AuthProvisionOutcome::Synced | AuthProvisionOutcome::TokenMode) => (
            configured_source,
            schema::enums::OutcomeValue::Success,
            None,
        ),
        Ok(AuthProvisionOutcome::Skipped) => (
            schema::enums::CredentialSourceType::None,
            schema::enums::OutcomeValue::Skip,
            None,
        ),
        Ok(AuthProvisionOutcome::HostMissing) => (
            schema::enums::CredentialSourceType::None,
            schema::enums::OutcomeValue::Failure,
            Some(schema::enums::ErrorType::CredentialUnavailable),
        ),
        Err(error_type) => (
            configured_source,
            schema::enums::OutcomeValue::Error,
            Some(error_type),
        ),
    };
    let mut attrs = vec![
        Attr {
            key: schema::attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent.slug()),
        },
        Attr {
            key: schema::attrs::AUTH_MODE,
            value: Value::Str(auth_mode.as_str()),
        },
        Attr {
            key: schema::attrs::CREDENTIAL_SOURCE_TYPE,
            value: Value::Str(source.as_str()),
        },
        Attr {
            key: schema::attrs::OUTCOME,
            value: Value::Str(outcome.as_str()),
        },
    ];
    if let Some(error_type) = error_type {
        attrs.push(Attr {
            key: schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type.as_str()),
        });
    }
    let _telemetry_result =
        jackin_telemetry::emit_event(&event::AUTH_PROVISION, FieldSet::new(&attrs, None));
}

fn validate_selected_account_sources(
    bindings: &[InstanceAuthBinding],
    host_home: &Path,
) -> anyhow::Result<()> {
    for binding in bindings {
        if binding.mode == AuthForwardMode::Sync
            && let Some(source) = &binding.sync_source_dir
        {
            validate_sync_source_dir(binding.agent, source, host_home)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RoleState {
    pub root: PathBuf,
    pub gh_config_dir: PathBuf,
    /// Resolved GitHub provisioning outcome from
    /// [`Self::provision_github_auth`]. The variant carries the resolved
    /// token (when applicable) so callers can derive `GH_TOKEN` /
    /// `GITHUB_TOKEN` via [`GithubProvisionOutcome::token`] without a
    /// parallel `Option<String>` field.
    pub gh_provision_outcome: GithubProvisionOutcome,
    pub agent_runtime: AgentRuntimeState,
    pub auth: ProvisionedAuth,
    pub auth_outcomes: BTreeMap<jackin_core::Agent, AuthProvisionOutcome>,
}

impl RoleState {
    /// Host path to Claude's account-metadata file. `None` when Claude is
    /// not in `supported_agents()`. Pair with [`Self::claude_forwards_auth`]
    /// when filtering for runtime reachability.
    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.first().map(PathBuf::as_path))
    }

    /// Manifest model override for Claude, or `None` when the selected agent
    /// is not Claude or when no override is configured.
    #[must_use]
    pub fn claude_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Claude {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Host path to Claude's OAuth credentials file. `None` when Claude is
    /// not in `supported_agents()`. Pair with [`Self::claude_forwards_auth`].
    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.get(1).map(PathBuf::as_path))
    }

    /// Whether Claude's auth files flow into the container under
    /// `/jackin/claude/`. `false` for env-driven modes (`ignore` /
    /// `api_key` / `oauth_token`) and when Claude is not in
    /// `supported_agents()`.
    #[must_use]
    pub fn claude_forwards_auth(&self) -> bool {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .is_some_and(|slot| slot.forward_auth)
    }

    /// Manifest model override for Codex, or `None` if not Codex or no override.
    #[must_use]
    pub fn codex_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Codex {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for Kimi, or `None` if not Kimi or no override.
    #[must_use]
    pub fn kimi_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Kimi {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for `OpenCode`, or `None` if not `OpenCode` or no override.
    #[must_use]
    pub fn opencode_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Opencode {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for Grok, or `None` if not Grok or no override.
    #[must_use]
    pub fn grok_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Grok {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }
}

/// Inputs to `RoleState::prepare` for the GitHub-auth axis.
///
/// Carries the resolved `[github]` mode and the operator-resolved
/// `GH_TOKEN` value (only meaningful under `Token` mode — under `Sync`
/// the token comes from the host's `gh` keychain or `hosts.yml`, not
/// from this struct). The launcher derives this struct from
/// `config::resolve_github_mode` and the merged `[github.env]` map.
///
/// `Debug` is implemented manually to redact `token` so the value
/// cannot leak through diagnostic telemetry, panic messages, or
/// `eprintln!("{ctx:?}")`.
#[derive(Clone, Default)]
pub struct GithubAuthContext {
    pub mode: GithubAuthMode,
    pub token: Option<String>,
}

impl std::fmt::Debug for GithubAuthContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubAuthContext")
            .field("mode", &self.mode)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Resolver closures for [`RoleState::prepare`].
#[expect(
    missing_debug_implementations,
    reason = "PrepareResolvers carries borrowed closures; callers log resolved values instead."
)]
pub struct PrepareResolvers<'a> {
    pub auth_modes: &'a dyn Fn(jackin_core::Agent) -> AuthForwardMode,
    pub sync_source_dirs: &'a dyn Fn(jackin_core::Agent) -> Option<PathBuf>,
}

impl RoleState {
    /// Provision auth state for every agent in `manifest.supported_agents()` by
    /// delegating to [`Self::prepare_for_agents`] with the full supported set.
    ///
    /// `resolvers.auth_modes` is invoked once per agent — pass
    /// `jackin_config::resolve_mode(config, a, ws, role)` so each agent gets
    /// its own configured forward mode. Reusing the *selected* agent's mode for
    /// sibling agents silently wipes their durable state when modes diverge
    /// (e.g. `claude.auth_forward = sync` next to `codex.auth_forward =
    /// api_key`).
    ///
    /// `resolvers.sync_source_dirs` returns an optional override source
    /// directory for each agent's auth sync, overriding `host_home`.
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        Self::prepare_for_agents(
            paths,
            container_name,
            manifest,
            resolvers,
            github,
            host_home,
            agent,
            &manifest.supported_agents(),
        )
    }

    /// Provision auth state only for the provided agents.
    ///
    /// Accepts an explicit `provision_agents` list so callers that intentionally
    /// need only a subset (such as tests) can pass a narrower slice. The
    /// foreground launch path passes the full `manifest.supported_agents()` set;
    /// [`Self::prepare`] is a convenience wrapper that does the same.
    ///
    /// Each agent resolves to one [`InstanceAuthBinding`] with a
    /// placeholder account id; multi-instance callers use
    /// [`Self::prepare_for_bindings`] directly so two instances of the
    /// same agent provision independently.
    #[expect(
        clippy::too_many_arguments,
        reason = "Per-agent prepare carries every per-agent + per-container input \
                  the role-materialize path needs: paths, container identity, \
                  role selectors, validated repo, agent list, env resolver, \
                  workspace. Bundling is a parallel-pass refactor."
    )]
    pub fn prepare_for_agents(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
        provision_agents: &[jackin_core::Agent],
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let supported = manifest.supported_agents();
        let bindings: Vec<InstanceAuthBinding> = provision_agents
            .iter()
            .copied()
            .filter(|provision_agent| supported.contains(provision_agent))
            .map(|provisioned| {
                InstanceAuthBinding::new(
                    DEFAULT_ACCOUNT_ID,
                    provisioned,
                    (resolvers.auth_modes)(provisioned),
                    (resolvers.sync_source_dirs)(provisioned),
                )
            })
            .collect();
        Self::prepare_for_bindings(
            paths,
            container_name,
            manifest,
            &bindings,
            github,
            host_home,
            agent,
        )
    }

    /// Provision auth state for one explicit instance binding each.
    ///
    /// Unlike [`Self::prepare_for_agents`], bindings carry their own
    /// account/mode/key, so two bindings for the same agent provision
    /// independently and merge as separate [`ProvisionedAuth::slots`]
    /// entries. Binding keys must be unique within `bindings`.
    pub fn prepare_for_bindings(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        bindings: &[InstanceAuthBinding],
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let gh_config_dir = root.join(".config/gh");
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        std::fs::create_dir_all(&home_dir)?;
        // Owned by the host operator; the container runs as that same UID
        // (`--user` on docker run), so `agent` can write state files here
        // with no special directory mode.
        std::fs::create_dir_all(&jackin_state_dir)?;

        let hosts_yml = gh_config_dir.join("hosts.yml");
        let github_context = github.clone();
        validate_selected_account_sources(bindings, host_home)?;

        let host_home_path = host_home.to_path_buf();
        let root_path = root.clone();
        let home_path = home_dir.clone();

        let (gh_provision_outcome, auth_provisions) = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let root = root_path.clone();
                let home_dir = home_path.clone();
                let host_home = host_home_path.clone();
                let provisioned = binding.agent;
                let mode = binding.mode;
                let binding = binding.clone();
                let handle = jackin_telemetry::spawn::thread_scoped_joined(scope, move || {
                    Self::provision_agent_auth_slot(&root, &home_dir, &host_home, &binding)
                });
                handles.push((provisioned, mode, handle));
            }

            let gh_provision_outcome =
                if github_ignore_can_skip_state_prepare(&github_context, &hosts_yml)? {
                    jackin_diagnostics::active_timing_started(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some(&github_context.mode.to_string()),
                    );
                    jackin_diagnostics::active_timing_done(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some("skipped_no_state"),
                    );
                    GithubProvisionOutcome::Skipped
                } else {
                    let gh_handle = jackin_telemetry::spawn::thread_scoped_joined(scope, {
                        let hosts_yml = hosts_yml.clone();
                        let host_home = host_home_path.clone();
                        move || Self::provision_github_slot(&hosts_yml, &github_context, &host_home)
                    });
                    gh_handle
                        .join()
                        .map_err(|_| InstanceError::GithubAuthTaskPanicked)??
                };

            let mut auth_provisions = Vec::with_capacity(handles.len());
            for (agent, mode, handle) in handles {
                match handle.join() {
                    Ok(Ok(provision)) => {
                        emit_agent_auth_provision(agent, mode, Ok(provision.outcome));
                        auth_provisions.push(provision);
                    }
                    Ok(Err(error)) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::IoError),
                        );
                        return Err(error);
                    }
                    Err(_) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::Panic),
                        );
                        return Err(InstanceError::AuthProvisionTaskPanicked {
                            agent: agent.slug().to_owned(),
                        }
                        .into());
                    }
                }
            }

            anyhow::Ok((gh_provision_outcome, auth_provisions))
        })?;

        let mut auth = ProvisionedAuth::default();
        let mut auth_outcomes = BTreeMap::new();
        let mut selected_outcome = AuthProvisionOutcome::Skipped;

        for provision in auth_provisions {
            if provision.auth.agent == agent {
                selected_outcome = provision.outcome;
            }
            auth_outcomes.insert(provision.auth.agent, provision.outcome);
            auth.slots.insert(provision.key, provision.auth);
        }

        // Single struct construction — no per-variant dispatch needed.
        let agent_runtime = AgentRuntimeState {
            agent,
            model: manifest.agent_model(agent).map(str::to_owned),
        };

        Ok((
            Self {
                root,
                gh_config_dir,
                gh_provision_outcome,
                agent_runtime,
                auth,
                auth_outcomes,
            },
            selected_outcome,
        ))
    }

    fn provision_github_slot(
        hosts_yml: &Path,
        github: &GithubAuthContext,
        host_home: &Path,
    ) -> anyhow::Result<GithubProvisionOutcome> {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "role_state_prepare:github_auth",
            Some(&github.mode.to_string()),
        );
        if let Some(parent) = hosts_yml.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create GitHub role-state directory at {}",
                    parent.display()
                )
            })?;
        }
        let result = Self::provision_github_auth(hosts_yml, github, host_home);
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "role_state_prepare:github_auth",
            Some(if result.is_ok() { "prepared" } else { "error" }),
        );
        result
    }

    /// Background-prewarm auth state for non-selected agents only.
    ///
    /// This intentionally skips the GitHub-auth axis and returns no launch
    /// `RoleState`: foreground launch already prepared the selected agent and
    /// GitHub context needed for the current `docker run`. Background sibling
    /// prep may create/update only jackin-owned per-agent state under the
    /// instance data dir so opening a later sibling runtime has less work.
    pub fn prewarm_auth_for_agents(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        host_home: &Path,
        agents: &[jackin_core::Agent],
    ) -> anyhow::Result<usize> {
        let supported = manifest.supported_agents();
        let bindings: Vec<InstanceAuthBinding> = agents
            .iter()
            .copied()
            .filter(|agent| supported.contains(agent))
            .map(|provisioned| {
                InstanceAuthBinding::new(
                    DEFAULT_ACCOUNT_ID,
                    provisioned,
                    (resolvers.auth_modes)(provisioned),
                    (resolvers.sync_source_dirs)(provisioned),
                )
            })
            .collect();
        Self::prewarm_auth_for_bindings(paths, container_name, &bindings, host_home)
    }

    /// Background-prewarm auth state for one explicit instance binding
    /// each. Binding-driven counterpart of
    /// [`Self::prewarm_auth_for_agents`]; see its docs for the
    /// skip-GitHub contract. Bindings are already resolved, so no
    /// manifest filter applies here.
    pub fn prewarm_auth_for_bindings(
        paths: &JackinPaths,
        container_name: &str,
        bindings: &[InstanceAuthBinding],
        host_home: &Path,
    ) -> anyhow::Result<usize> {
        let root = paths.data_dir.join(container_name);
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        std::fs::create_dir_all(&home_dir)?;
        std::fs::create_dir_all(&jackin_state_dir)?;

        validate_selected_account_sources(bindings, host_home)?;

        let host_home_path = host_home.to_path_buf();
        let root_path = root.clone();
        let home_path = home_dir.clone();

        let prepared_auth = std::thread::scope(|scope| {
            let handles = bindings
                .iter()
                .map(|binding| {
                    let root = root_path.clone();
                    let home_dir = home_path.clone();
                    let host_home = host_home_path.clone();
                    let provisioned = binding.agent;
                    let mode = binding.mode;
                    let binding = binding.clone();
                    let handle = jackin_telemetry::spawn::thread_scoped_joined(scope, move || {
                        Self::provision_agent_auth_slot(&root, &home_dir, &host_home, &binding)
                    });
                    (provisioned, mode, handle)
                })
                .collect::<Vec<_>>();

            let mut prepared = Vec::with_capacity(handles.len());
            for (agent, mode, handle) in handles {
                match handle.join() {
                    Ok(Ok(provision)) => {
                        emit_agent_auth_provision(agent, mode, Ok(provision.outcome));
                        prepared.push(provision);
                    }
                    Ok(Err(error)) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::IoError),
                        );
                        return Err(error);
                    }
                    Err(_) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::Panic),
                        );
                        return Err(InstanceError::BackgroundAuthTaskPanicked.into());
                    }
                }
            }
            anyhow::Ok(prepared)
        })?;

        Ok(prepared_auth.len())
    }

    fn provision_agent_auth_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<AgentAuthProvision> {
        let agent = binding.agent;
        let mode = binding.mode;
        let timing_name = format!("role_state_prepare:{}_auth", agent.slug());
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            &timing_name,
            Some(&mode.to_string()),
        );
        let ignore_can_skip = if mode == AuthForwardMode::Ignore {
            agent_ignore_can_skip_state_prepare(root, agent)?
        } else {
            false
        };
        if ignore_can_skip {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                &timing_name,
                Some("skipped_no_state"),
            );
            let provision = AgentAuthProvision {
                key: binding.key.clone(),
                auth: skipped_ignore_instance_auth(root, binding),
                outcome: AuthProvisionOutcome::Skipped,
            };
            return Ok(provision);
        }
        let provision_result: anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> =
            match agent {
                jackin_core::Agent::Claude => {
                    Self::provision_claude_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Codex => {
                    Self::provision_codex_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Amp => {
                    Self::provision_amp_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Kimi => {
                    Self::provision_kimi_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Opencode => {
                    Self::provision_opencode_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Grok => {
                    Self::provision_grok_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Antigravity => {
                    Self::provision_antigravity_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Gemini => {
                    Self::provision_gemini_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Cursor => {
                    Self::provision_cursor_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Muse => {
                    Self::provision_muse_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Omp => {
                    Self::provision_omp_slot(root, home_dir, host_home, binding)
                }
                jackin_core::Agent::Hermes => {
                    Self::provision_hermes_slot(root, home_dir, host_home, binding)
                }
            };
        let timing_detail = provision_result
            .as_ref()
            .map_or("error".to_owned(), |(_, outcome)| format!("{outcome:?}"));
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Credentials,
            &timing_name,
            Some(&timing_detail),
        );
        let (slot, outcome) = provision_result?;
        anyhow::ensure!(
            !(mode == AuthForwardMode::Sync
                && binding.sync_source_dir.is_some()
                && outcome == AuthProvisionOutcome::HostMissing),
            "selected {agent} account credentials disappeared during provisioning"
        );
        Ok(AgentAuthProvision {
            key: binding.key.clone(),
            auth: slot,
            outcome,
        })
    }

    fn provision_claude_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let claude_dir = root.join("claude");
        let claude_home_dir = home_dir.join(".claude");
        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&claude_home_dir)?;
        // 0o600 because the Claude CLI may later persist OAuth state
        // into this file once the container runs.
        let claude_account_home = claude_home_dir.join(".claude.json");
        auth::create_private_file_if_absent(&claude_account_home, b"{}")?;
        let account_json = claude_dir.join("account.json");
        let credentials_json = claude_dir.join("credentials.json");
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_claude_auth_from_config_dir(
                &account_json,
                &credentials_json,
                mode,
                host_home,
                source_dir,
            )?
        } else {
            Self::provision_claude_auth(&account_json, &credentials_json, mode, host_home)?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(claude_home_dir),
            vec![account_json, credentials_json],
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_codex_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let codex_dir = root.join("codex");
        let codex_home_dir = home_dir.join(".codex");
        std::fs::create_dir_all(&codex_dir)?;
        std::fs::create_dir_all(&codex_home_dir)?;
        let auth_json_path = codex_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_codex_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_codex_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(codex_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_amp_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let amp_dir = root.join("amp");
        let amp_home_dir = home_dir.join(".local/share/amp");
        std::fs::create_dir_all(&amp_dir)?;
        std::fs::create_dir_all(&amp_home_dir)?;
        std::fs::create_dir_all(home_dir.join(".config/amp"))?;
        if mode == AuthForwardMode::Sync
            && let Some(source) = sync_source_dir
        {
            let settings = source.join("config/amp/settings.json");
            if settings.is_file() {
                std::fs::copy(settings, home_dir.join(".config/amp/settings.json"))?;
            }
        }
        let secrets_json_path = amp_dir.join("secrets.json");
        let (outcome, secrets_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_amp_auth_from_source_dir(&secrets_json_path, mode, source_dir)?
        } else {
            Self::provision_amp_auth(&secrets_json_path, mode, host_home)?
        };
        let credential_paths = secrets_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(amp_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_kimi_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let kimi_dir = root.join("kimi-code");
        let kimi_home_dir = home_dir.join(".kimi-code");
        std::fs::create_dir_all(&kimi_dir)?;
        std::fs::create_dir_all(&kimi_home_dir)?;
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_kimi_auth_from_source_dir(&kimi_dir, mode, source_dir)?
        } else {
            Self::provision_kimi_auth(&kimi_dir, mode, host_home)?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(kimi_home_dir),
            vec![kimi_dir],
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_opencode_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let opencode_dir = root.join("opencode");
        let opencode_home_dir = home_dir.join(".local/share/opencode");
        std::fs::create_dir_all(&opencode_dir)?;
        std::fs::create_dir_all(&opencode_home_dir)?;
        std::fs::create_dir_all(home_dir.join(".config/opencode"))?;
        let auth_json_path = opencode_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_opencode_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_opencode_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(opencode_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_grok_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let grok_dir = root.join("grok");
        let grok_home_dir = home_dir.join(".grok");
        std::fs::create_dir_all(&grok_dir)?;
        std::fs::create_dir_all(&grok_home_dir)?;
        let auth_json_path = grok_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_grok_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_grok_auth(&auth_json_path, mode, host_home)?
        };

        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(grok_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_antigravity_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let antigravity_dir = root.join("antigravity");
        let antigravity_home_dir = home_dir.join(".gemini/antigravity-cli");
        std::fs::create_dir_all(&antigravity_dir)?;
        std::fs::create_dir_all(&antigravity_home_dir)?;
        let settings_json_path = antigravity_dir.join("settings.json");
        let (outcome, settings_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_antigravity_auth_from_source_dir(&settings_json_path, mode, source_dir)?
        } else {
            Self::provision_antigravity_auth(&settings_json_path, mode, host_home)?
        };
        let credential_paths = settings_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(antigravity_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_gemini_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let gemini_dir = root.join("gemini");
        let gemini_home_dir = home_dir.join(".gemini");
        std::fs::create_dir_all(&gemini_dir)?;
        std::fs::create_dir_all(&gemini_home_dir)?;
        let oauth_creds_path = gemini_dir.join("oauth_creds.json");
        let (outcome, oauth_creds) = if let Some(source_dir) = sync_source_dir {
            Self::provision_gemini_auth_from_source_dir(&oauth_creds_path, mode, source_dir)?
        } else {
            Self::provision_gemini_auth(&oauth_creds_path, mode, host_home)?
        };
        let credential_paths = oauth_creds.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(gemini_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_cursor_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let cursor_dir = root.join("cursor");
        let cursor_home_dir = home_dir.join(".cursor");
        std::fs::create_dir_all(&cursor_dir)?;
        std::fs::create_dir_all(&cursor_home_dir)?;
        let auth_json_path = cursor_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_cursor_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_cursor_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(cursor_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_muse_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let muse_dir = root.join("muse");
        let muse_home_dir = home_dir.join(".config/muse");
        std::fs::create_dir_all(&muse_dir)?;
        std::fs::create_dir_all(&muse_home_dir)?;
        let auth_json_path = muse_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_muse_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_muse_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(muse_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_omp_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let omp_dir = root.join("omp");
        let omp_home_dir = home_dir.join(".omp");
        std::fs::create_dir_all(&omp_dir)?;
        std::fs::create_dir_all(&omp_home_dir)?;
        let agent_db_path = omp_dir.join("agent.db");
        let (outcome, agent_db) = if let Some(source_dir) = sync_source_dir {
            Self::provision_omp_auth_from_source_dir(&agent_db_path, mode, source_dir)?
        } else {
            Self::provision_omp_auth(&agent_db_path, mode, host_home)?
        };
        let credential_paths = agent_db.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(omp_home_dir),
            credential_paths,
            forward_auth,
        );
        Ok((slot, outcome))
    }

    fn provision_hermes_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.sync_source_dir.as_deref();
        let hermes_dir = root.join("hermes");
        let hermes_home_dir = home_dir.join(".hermes");
        std::fs::create_dir_all(&hermes_dir)?;
        std::fs::create_dir_all(&hermes_home_dir)?;
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_hermes_auth_from_source_dir(&hermes_dir, mode, source_dir)?
        } else {
            Self::provision_hermes_auth(&hermes_dir, mode, host_home)?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(hermes_home_dir),
            vec![hermes_dir],
            forward_auth,
        );
        Ok((slot, outcome))
    }
}

fn skipped_ignore_instance_auth(
    root: &Path,
    binding: &InstanceAuthBinding,
) -> ProvisionedInstanceAuth {
    // No filesystem work ran, so `home_dir` stays `None`; the
    // deterministic credential paths match the real-path shape so
    // path accessors keep working for lazy launches.
    let credential_paths = match binding.agent {
        jackin_core::Agent::Claude => {
            let claude_dir = root.join("claude");
            vec![
                claude_dir.join("account.json"),
                claude_dir.join("credentials.json"),
            ]
        }
        jackin_core::Agent::Kimi => vec![root.join("kimi-code")],
        jackin_core::Agent::Hermes => vec![root.join("hermes")],
        _ => Vec::new(),
    };
    ProvisionedInstanceAuth::new(binding, None, credential_paths, false)
}

fn agent_ignore_can_skip_state_prepare(
    root: &Path,
    agent: jackin_core::Agent,
) -> anyhow::Result<bool> {
    let stale_paths: Vec<PathBuf> = match agent {
        jackin_core::Agent::Claude => {
            let claude_dir = root.join("claude");
            vec![
                claude_dir.join("account.json"),
                claude_dir.join("credentials.json"),
            ]
        }
        jackin_core::Agent::Codex => vec![root.join("codex/auth.json")],
        jackin_core::Agent::Amp => vec![root.join("amp/secrets.json")],
        jackin_core::Agent::Kimi => vec![root.join("kimi-code")],
        jackin_core::Agent::Opencode => vec![root.join("opencode/auth.json")],
        jackin_core::Agent::Grok => vec![root.join("grok/auth.json")],
        jackin_core::Agent::Antigravity => vec![root.join("antigravity/settings.json")],
        jackin_core::Agent::Gemini => vec![root.join("gemini/oauth_creds.json")],
        jackin_core::Agent::Cursor => vec![root.join("cursor/auth.json")],
        jackin_core::Agent::Muse => vec![root.join("muse/auth.json")],
        jackin_core::Agent::Omp => vec![root.join("omp/agent.db")],
        jackin_core::Agent::Hermes => vec![root.join("hermes")],
    };

    for path in stale_paths {
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect {agent} role-state auth path at {}",
                        path.display()
                    )
                });
            }
        }
    }

    Ok(true)
}

fn github_ignore_can_skip_state_prepare(
    github: &GithubAuthContext,
    hosts_yml: &Path,
) -> anyhow::Result<bool> {
    if github.mode != GithubAuthMode::Ignore {
        return Ok(false);
    }
    match std::fs::symlink_metadata(hosts_yml) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect GitHub role-state file at {}",
                hosts_yml.display()
            )
        }),
    }
}

#[cfg(test)]
mod tests;
