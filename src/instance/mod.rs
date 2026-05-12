use crate::config::{AuthForwardMode, GithubAuthMode};
use crate::manifest::RoleManifest;
use crate::paths::JackinPaths;
use std::path::{Path, PathBuf};

mod auth;
pub mod manifest;
pub mod naming;
pub use manifest::{
    DockerResources, InstanceIndex, InstanceIndexEntry, InstanceManifest, InstanceQuery,
    InstanceStatus, NewInstanceManifest,
};
pub use naming::{
    class_family_matches, container_name_with_id, new_container_name, primary_container_name,
    runtime_slug,
};

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
// `TokenMode` so the value never lands in a `tracing::debug!`,
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

/// Runtime state for the selected agent (model override only).
///
/// Auth paths for all agents in `manifest.supported_agents()` are tracked
/// separately on [`ProvisionedAuth`] so `hardline --new` can switch to any
/// supported agent without re-authentication.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentRuntimeState {
    /// Selected agent is Claude.
    Claude {
        /// Optional model override from `[claude].model`. Passed to the
        /// interactive CLI with `--model` at launch time.
        model: Option<String>,
    },
    /// Selected agent is Codex.
    Codex {
        /// Optional model override from `[codex].model`. Passed to the
        /// interactive CLI with `-m` at launch time.
        model: Option<String>,
    },
    /// Selected agent is Amp.
    Amp,
}

/// Auth state provisioned for every agent listed in
/// `manifest.supported_agents()`.
///
/// Fields for a given agent are populated only when that agent appears in
/// `supported_agents()`; otherwise they are `false` / `None`. `agent_mounts`
/// consults this struct so every supported agent has its credentials available
/// inside the container regardless of which agent started the initial session —
/// enabling `hardline --new` to switch agents without re-authentication.
#[derive(Debug, Clone)]
pub struct ProvisionedAuth {
    /// Whether Claude's home directories were provisioned.
    pub claude: bool,
    /// Whether Claude's auth files should be bind-mounted under
    /// `/jackin/claude/`. `false` for env-driven modes
    /// (`ignore` / `api_key` / `oauth_token`).
    pub claude_forward_auth: bool,
    /// Host path to Claude's account-metadata file, or `None` when Claude
    /// is not in `supported_agents()`.
    pub claude_account_json: Option<PathBuf>,
    /// Host path to Claude's OAuth credentials file, or `None` when Claude
    /// is not in `supported_agents()`.
    pub claude_credentials_json: Option<PathBuf>,
    /// Whether Codex's home directories were provisioned.
    pub codex: bool,
    /// Host path to Codex's `auth.json` when available, or `None` when
    /// Codex is not in `supported_agents()` or the host had no auth file.
    pub codex_auth_json: Option<PathBuf>,
    /// Whether Amp's home directories were provisioned.
    pub amp: bool,
    /// Host path to Amp's `secrets.json` when available, or `None` when
    /// Amp is not in `supported_agents()` or no secrets file existed.
    pub amp_secrets_json: Option<PathBuf>,
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
    /// Runtime state for the selected agent (model override only). Auth
    /// paths for all supported agents live on `auth`.
    pub agent_runtime: AgentRuntimeState,
    /// Provisioned auth for every agent in `manifest.supported_agents()`.
    pub auth: ProvisionedAuth,
}

impl RoleState {
    /// Host path to Claude's account-metadata file. `None` when Claude is
    /// not in `supported_agents()`. The path is returned regardless of mount
    /// eligibility — consult [`Self::claude_forwards_auth`] when filtering
    /// for runtime reachability.
    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        self.auth.claude_account_json.as_deref()
    }

    /// Claude model override, if the role manifest declared one and the
    /// selected agent is Claude.
    #[must_use]
    pub fn claude_model(&self) -> Option<&str> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { model } => model.as_deref(),
            AgentRuntimeState::Codex { .. } | AgentRuntimeState::Amp => None,
        }
    }

    /// Host path to Claude's OAuth credentials file. `None` when Claude is
    /// not in `supported_agents()`. Pair with [`Self::claude_forwards_auth`]
    /// when filtering for runtime reachability.
    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        self.auth.claude_credentials_json.as_deref()
    }

    /// Whether Claude's auth files (`account.json`, `credentials.json`)
    /// should flow into the container under `/jackin/claude/`. `false` for
    /// env-driven modes (`ignore` / `api_key` / `oauth_token`) and when
    /// Claude is not in `supported_agents()`.
    #[must_use]
    pub fn claude_forwards_auth(&self) -> bool {
        self.auth.claude_forward_auth
    }

    /// Codex model override, if the role manifest declared one and the
    /// selected agent is Codex.
    #[must_use]
    pub fn codex_model(&self) -> Option<&str> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { model } => model.as_deref(),
            AgentRuntimeState::Claude { .. } | AgentRuntimeState::Amp => None,
        }
    }

    /// Host path to Codex's `auth.json`. `None` when Codex is not in
    /// `supported_agents()` or when no auth file is available.
    #[must_use]
    pub fn codex_auth_json(&self) -> Option<&Path> {
        self.auth.codex_auth_json.as_deref()
    }

    /// Host path to Amp's `secrets.json`. `None` when Amp is not in
    /// `supported_agents()` or when no file is available.
    #[must_use]
    pub fn amp_secrets_json(&self) -> Option<&Path> {
        self.auth.amp_secrets_json.as_deref()
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
/// cannot leak through `tracing::debug!`, panic messages, or
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

impl RoleState {
    /// Provision per-supported-agent auth state.
    ///
    /// `auth_modes` is invoked once per agent in `manifest.supported_agents()`
    /// — pass `crate::config::resolve_mode(config, a, ws, role)` so each
    /// agent gets its own configured forward mode. Reusing the *selected*
    /// agent's mode for sibling agents silently wipes their durable state
    /// when modes diverge (e.g. `claude.auth_forward = sync` next to
    /// `codex.auth_forward = api_key`).
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        auth_modes: &dyn Fn(crate::agent::Agent) -> AuthForwardMode,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: crate::agent::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let gh_config_dir = root.join(".config/gh");
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        std::fs::create_dir_all(&gh_config_dir)?;
        std::fs::create_dir_all(&home_dir)?;
        std::fs::create_dir_all(&jackin_state_dir)?;

        let hosts_yml = gh_config_dir.join("hosts.yml");
        let gh_provision_outcome = Self::provision_github_auth(&hosts_yml, github, host_home)?;

        // Provision auth for every agent in supported_agents() so
        // `hardline --new` can switch agents without re-authentication.
        let mut auth = ProvisionedAuth {
            claude: false,
            claude_forward_auth: false,
            claude_account_json: None,
            claude_credentials_json: None,
            codex: false,
            codex_auth_json: None,
            amp: false,
            amp_secrets_json: None,
        };
        let mut selected_outcome = AuthProvisionOutcome::Skipped;

        for supported in manifest.supported_agents() {
            match supported {
                crate::agent::Agent::Claude => {
                    let claude_dir = root.join("claude");
                    let claude_home_dir = home_dir.join(".claude");
                    std::fs::create_dir_all(&claude_dir)?;
                    std::fs::create_dir_all(&claude_home_dir)?;
                    let claude_account_home = home_dir.join(".claude.json");
                    if !claude_account_home.exists() {
                        std::fs::write(&claude_account_home, "{}")?;
                    }
                    let account_json = claude_dir.join("account.json");
                    let credentials_json = claude_dir.join("credentials.json");
                    let (outcome, forward_auth) = Self::provision_claude_auth(
                        &account_json,
                        &credentials_json,
                        auth_modes(supported),
                        host_home,
                    )?;
                    auth.claude = true;
                    auth.claude_forward_auth = forward_auth;
                    auth.claude_account_json = Some(account_json);
                    auth.claude_credentials_json = Some(credentials_json);
                    if supported == agent {
                        selected_outcome = outcome;
                    }
                }
                crate::agent::Agent::Codex => {
                    let codex_dir = root.join("codex");
                    let codex_home_dir = home_dir.join(".codex");
                    std::fs::create_dir_all(&codex_dir)?;
                    std::fs::create_dir_all(&codex_home_dir)?;
                    let auth_json_path = codex_dir.join("auth.json");
                    let (outcome, auth_json) = Self::provision_codex_auth(
                        &auth_json_path,
                        auth_modes(supported),
                        host_home,
                    )?;
                    auth.codex = true;
                    auth.codex_auth_json = auth_json;
                    if supported == agent {
                        selected_outcome = outcome;
                    }
                }
                crate::agent::Agent::Amp => {
                    let amp_dir = root.join("amp");
                    let amp_home_dir = home_dir.join(".local/share/amp");
                    std::fs::create_dir_all(&amp_dir)?;
                    std::fs::create_dir_all(&amp_home_dir)?;
                    let secrets_json_path = amp_dir.join("secrets.json");
                    let (outcome, secrets_json) = Self::provision_amp_auth(
                        &secrets_json_path,
                        auth_modes(supported),
                        host_home,
                    )?;
                    auth.amp = true;
                    auth.amp_secrets_json = secrets_json;
                    if supported == agent {
                        selected_outcome = outcome;
                    }
                }
            }
        }

        let agent_runtime = match agent {
            crate::agent::Agent::Claude => AgentRuntimeState::Claude {
                model: manifest.claude.as_ref().and_then(|cfg| cfg.model.clone()),
            },
            crate::agent::Agent::Codex => AgentRuntimeState::Codex {
                model: manifest.codex.as_ref().and_then(|cfg| cfg.model.clone()),
            },
            crate::agent::Agent::Amp => AgentRuntimeState::Amp,
        };

        Ok((
            Self {
                root,
                gh_config_dir,
                gh_provision_outcome,
                agent_runtime,
                auth,
            },
            selected_outcome,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    fn simple_manifest(temp: &tempfile::TempDir) -> crate::manifest::RoleManifest {
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        crate::manifest::RoleManifest::load(temp.path()).unwrap()
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Auth files exist as `{}` placeholders even under env-driven
        // modes; they just won't be bind-mounted (`forward_auth = false`).
        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(
            !state.claude_forwards_auth(),
            "Ignore mode must not forward auth into the container",
        );
        assert!(state.claude_model().is_none());
        assert!(state.codex_model().is_none());

        // Pin the host-side grouped layout: a regression to the legacy
        // flat shape (`.claude/state/.credentials.json` at the data-dir
        // root) would still satisfy the accessor checks
        // above, since they only look up paths through the enum. These
        // assertions verify the actual host paths under
        // `<container>/claude/`.
        let container_root = paths.data_dir.join("jackin-agent-smith");
        assert_eq!(
            state.claude_account_json().unwrap(),
            container_root.join("claude").join("account.json"),
        );
        assert_eq!(
            state.claude_credentials_json().unwrap(),
            container_root.join("claude").join("credentials.json"),
        );
        assert!(container_root.join("home/.claude").is_dir());
        assert_eq!(
            std::fs::read_to_string(container_root.join("home/.claude.json")).unwrap(),
            "{}"
        );
        assert!(container_root.join("state").is_dir());
    }

    #[test]
    fn prepares_codex_state_carries_model_without_config_toml() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
model = "gpt-5"
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Codex,
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert_eq!(state.codex_model(), Some("gpt-5"));
        assert!(
            !paths
                .data_dir
                .join("jackin-agent-smith")
                .join("codex")
                .join("config.toml")
                .exists()
        );
        assert!(
            paths
                .data_dir
                .join("jackin-agent-smith")
                .join("home/.codex")
                .is_dir()
        );
        // Codex state carries no Claude auth paths — the typed enum
        // makes the absence structural rather than a runtime nil.
        assert!(state.claude_account_json().is_none());
        assert!(state.claude_credentials_json().is_none());
        assert!(!state.claude_forwards_auth());
    }

    /// Regression: a multi-agent role must apply each supported
    /// agent's *own* configured `auth_forward` mode, not the selected
    /// agent's mode. Before the fix, selecting Codex with
    /// `codex.auth_forward = ApiKey` would call `provision_claude_auth`
    /// with `ApiKey` and silently `wipe_claude_state`, destroying the
    /// operator's durable Claude credentials and breaking the next
    /// `hardline --new --agent claude` switch.
    #[test]
    fn prepare_resolves_auth_mode_per_supported_agent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        // Claude → Sync (host missing → HostMissing, forward_auth = true)
        // Codex → ApiKey (would wipe Claude state if applied cross-agent)
        let auth_modes = |agent: crate::agent::Agent| match agent {
            crate::agent::Agent::Claude => AuthForwardMode::Sync,
            crate::agent::Agent::Codex => AuthForwardMode::ApiKey,
            crate::agent::Agent::Amp => AuthForwardMode::Ignore,
        };

        let (state, selected_outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &auth_modes,
            &GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Codex,
        )
        .unwrap();

        // Selected agent is Codex with ApiKey → TokenMode (env-driven).
        // The selected-outcome attribution must follow the *selected*
        // agent, not the last-iterated one.
        assert_eq!(selected_outcome, AuthProvisionOutcome::TokenMode);

        // Both agents provisioned.
        assert!(state.auth.claude, "claude home dirs should be provisioned");
        assert!(state.auth.codex, "codex home dirs should be provisioned");

        // Critical assertion: Claude's mode (Sync) is honored, not
        // Codex's (ApiKey). A regression to applying Codex's mode to
        // Claude would wipe state and set forward_auth = false.
        assert!(
            state.claude_forwards_auth(),
            "claude.auth_forward = Sync must produce forward_auth = true even when Codex is the selected agent",
        );
        assert!(
            state.claude_account_json().unwrap().exists(),
            "Sync mode must leave an account.json placeholder on disk",
        );
    }
}
