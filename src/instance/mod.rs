use crate::config::{AuthForwardMode, GithubAuthMode};
use crate::manifest::RoleManifest;
use crate::paths::JackinPaths;
use std::path::{Path, PathBuf};

mod auth;
pub mod naming;
pub use naming::{class_family_matches, next_container_name, primary_container_name, runtime_slug};

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

/// Agent-specific paths that belong to one variant.
///
/// Encoded as an enum so the agent variant and the actual paths can
/// never disagree.
///
/// All host paths land under `/jackin/<agent>/...` inside the
/// container. The agent's expected home-relative paths
/// (`~/.claude.json`, `~/.codex/auth.json`, `~/.local/share/amp/secrets.json`,
/// …) are NOT bind-mounted directly: jackin's entrypoint copies the
/// relevant files from `/jackin/` into the agent's home before launch.
/// This isolates the host→container handoff to a single tree (`/jackin/`)
/// the operator can audit at a glance, and frees the agent's home tree
/// (`/home/agent/.claude/`, `/home/agent/.codex/`,
/// `/home/agent/.local/share/amp/`) to carry image-baked config without
/// being masked by a runtime mount.
///
/// Every variant encodes mount eligibility in `Option<PathBuf>`:
/// `Some(p)` ⇔ the file should bind-mount at `/jackin/<agent>/<file>`,
/// `None` ⇔ env-driven auth (`CLAUDE_CODE_OAUTH_TOKEN` /
/// `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `AMP_API_KEY`) or the
/// host-missing fallback. `wipe_claude_state` may still leave a `{}`
/// shell on disk; this type tracks mount, not on-disk presence.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentRuntimeState {
    Claude {
        /// Host path mounted at `/jackin/claude/account.json`. `None`
        /// when env-driven modes wiped it / mounted nothing, matching
        /// the `Some` ⇔ mountable invariant Codex and Amp variants
        /// follow. The role-state file itself may still exist as `{}`
        /// after a non-sync mode; this field tracks mount eligibility,
        /// not on-disk presence.
        account_json: Option<PathBuf>,
        /// Host path mounted at `/jackin/claude/credentials.json`.
        /// Same `Some` ⇔ mountable invariant as `account_json`.
        credentials_json: Option<PathBuf>,
    },
    Codex {
        /// Host path mounted at `/jackin/codex/config.toml` (always —
        /// generated from the manifest, not auth state).
        config_toml: PathBuf,
        /// Host path mounted at `/jackin/codex/auth.json` when the
        /// file was synced from the host's `~/.codex/auth.json` on a
        /// previous launch. `None` when the host had no auth.json at
        /// the most recent launch — the bind mount is skipped and any
        /// in-container `codex login` writes to the container's
        /// writable layer (lost on `docker rm`).
        auth_json: Option<PathBuf>,
    },
    Amp {
        /// Host path mounted at `/jackin/amp/secrets.json`. `None`
        /// when env-driven modes wiped it or sync had no host file
        /// to copy and no carry-over to preserve.
        ///
        /// `XDG_DATA` path. `~/.config/amp/settings.json` (`XDG_CONFIG`)
        /// holds preferences only and is intentionally not forwarded.
        secrets_json: Option<PathBuf>,
    },
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
}

impl RoleState {
    /// Host path to Claude's account-metadata file. `None` when no
    /// mount is eligible (env-driven mode wiped it / non-Claude
    /// state).
    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { account_json, .. } => account_json.as_deref(),
            AgentRuntimeState::Codex { .. } | AgentRuntimeState::Amp { .. } => None,
        }
    }

    /// Host path to Claude's OAuth credentials file. `None` when no
    /// mount is eligible.
    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude {
                credentials_json, ..
            } => credentials_json.as_deref(),
            AgentRuntimeState::Codex { .. } | AgentRuntimeState::Amp { .. } => None,
        }
    }

    /// Host path to Codex's `config.toml` (mounted at
    /// `/jackin/codex/config.toml` in the container). `None`
    /// if this state was not prepared for `Agent::Codex`.
    #[must_use]
    pub fn codex_config_toml(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { config_toml, .. } => Some(config_toml),
            AgentRuntimeState::Claude { .. } | AgentRuntimeState::Amp { .. } => None,
        }
    }

    /// Host path to Codex's `auth.json` (mounted at
    /// `/jackin/codex/auth.json` in the container). `None` when
    /// no auth file is available (host had none and no in-container
    /// login has run yet) or when this state was not prepared for
    /// `Agent::Codex`.
    #[must_use]
    pub fn codex_auth_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { auth_json, .. } => auth_json.as_deref(),
            AgentRuntimeState::Claude { .. } | AgentRuntimeState::Amp { .. } => None,
        }
    }

    /// Host path to Amp's `secrets.json`. `None` when no file is
    /// available or when this state was not prepared for `Agent::Amp`.
    #[must_use]
    pub fn amp_secrets_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Amp { secrets_json, .. } => secrets_json.as_deref(),
            AgentRuntimeState::Claude { .. } | AgentRuntimeState::Codex { .. } => None,
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
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        auth_forward: AuthForwardMode,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: crate::agent::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let gh_config_dir = root.join(".config/gh");

        std::fs::create_dir_all(&gh_config_dir)?;

        let hosts_yml = gh_config_dir.join("hosts.yml");
        let gh_provision_outcome = Self::provision_github_auth(&hosts_yml, github, host_home)?;

        let (agent_runtime, outcome) = match agent {
            crate::agent::Agent::Claude => {
                let claude_dir = root.join("claude");
                std::fs::create_dir_all(&claude_dir)?;

                let account_path = claude_dir.join("account.json");
                let credentials_path = claude_dir.join("credentials.json");
                let (outcome, forward_auth) = Self::provision_claude_auth(
                    &account_path,
                    &credentials_path,
                    auth_forward,
                    host_home,
                )?;

                // Mount eligibility: forward_auth gates the whole pair
                // (env-driven modes never bind host state), then each
                // file's existence gates its own mount. `provision_claude_auth`
                // always writes account.json (as `{}` when wiping) so it
                // exists post-call under sync; credentials.json may have
                // been removed by the wipe path.
                let (account_json, credentials_json) = if forward_auth {
                    (
                        account_path.exists().then_some(account_path),
                        credentials_path.exists().then_some(credentials_path),
                    )
                } else {
                    (None, None)
                };

                (
                    AgentRuntimeState::Claude {
                        account_json,
                        credentials_json,
                    },
                    outcome,
                )
            }
            crate::agent::Agent::Codex => {
                let codex_dir = root.join("codex");
                std::fs::create_dir_all(&codex_dir)?;
                let config_toml = codex_dir.join("config.toml");
                let auth_json_path = codex_dir.join("auth.json");
                let (outcome, auth_json) = Self::provision_codex_auth(
                    &config_toml,
                    &auth_json_path,
                    manifest,
                    auth_forward,
                    host_home,
                )?;
                (
                    AgentRuntimeState::Codex {
                        config_toml,
                        auth_json,
                    },
                    outcome,
                )
            }
            crate::agent::Agent::Amp => {
                let amp_dir = root.join("amp");
                std::fs::create_dir_all(&amp_dir)?;
                let secrets_json_path = amp_dir.join("secrets.json");
                let (outcome, secrets_json) =
                    Self::provision_amp_auth(&secrets_json_path, auth_forward, host_home)?;
                (AgentRuntimeState::Amp { secrets_json }, outcome)
            }
        };

        Ok((
            Self {
                root,
                gh_config_dir,
                gh_provision_outcome,
                agent_runtime,
            },
            outcome,
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
            r#"dockerfile = "Dockerfile"

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
            AuthForwardMode::Ignore,
            &GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Mount accessors return `None` under Ignore (env-driven mode
        // never bind-mounts host state). The role-state file itself
        // still exists on disk as a `{}` placeholder.
        assert!(state.claude_account_json().is_none());
        assert!(state.claude_credentials_json().is_none());
        assert!(state.codex_config_toml().is_none());

        let container_root = paths.data_dir.join("jackin-agent-smith");
        assert_eq!(
            std::fs::read_to_string(container_root.join("claude").join("account.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn prepares_codex_state_writes_config_toml() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["codex"]

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

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            &GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Codex,
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert!(state.codex_config_toml().is_some());
        assert!(state.codex_config_toml().unwrap().is_file());
        // Codex state carries no Claude auth paths — the typed enum
        // makes the absence structural rather than a runtime nil.
        assert!(state.claude_account_json().is_none());
        assert!(state.claude_credentials_json().is_none());
    }
}
