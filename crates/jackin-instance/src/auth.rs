//! Agent credential provisioning: copies or wipes per-agent auth files in the
//! role-state directory before container launch.
//!
//! Implements `RoleState` methods for each supported agent (`Claude`, `Codex`,
//! `Amp`, `Kimi`, `OpenCode`). Each provisioner applies the `AuthForwardMode`
//! policy (`Sync`, `ApiKey`, `OAuthToken`, `Ignore`) to decide whether to
//! copy the host credential file, leave it, or wipe it.
//!
//! Invariant: any symlink at an auth-file path is rejected before branching
//! on mode — a compromised role cannot redirect a provisioning write through
//! a symlink placed between launches.

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use super::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason, RoleState,
};
use crate::{InstanceError, SyncSourceValidationError};
use jackin_config::{AuthForwardMode, GithubAuthMode};
use jackin_core::agent::Agent;
use std::path::Path;

/// Validate that `source_dir` carries the credential structure `agent`
/// expects for sync-mode auth forwarding.
///
/// Returns `Ok(())` when the folder holds usable credentials for that
/// agent, or `Err` describing what is missing. The message is
/// shown verbatim in the Source Folder picker so an operator cannot
/// silently select a folder that yields no credentials (and, for Claude,
/// would otherwise leak the default account into the capsule).
///
/// `host_home` is the operator's real home directory; it gates the macOS
/// Keychain probe used to validate a file-less Claude config dir and is
/// otherwise unused.
pub fn validate_sync_source_dir(
    agent: Agent,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    if !source_dir.is_dir() {
        return Err(SyncSourceValidationError::new(format!(
            "{} is not a directory.",
            source_dir.display()
        )));
    }
    match agent {
        // Claude has no single credential file on macOS — the login lives
        // in the Keychain — so accept either the file or a matching
        // Keychain entry for this exact config dir.
        Agent::Claude => {
            if read_host_credentials_from_claude_config_dir(source_dir, host_home).is_some() {
                Ok(())
            } else {
                Err(SyncSourceValidationError::new(format!(
                    "Not a Claude config folder: {} has no .credentials.json and no matching \
                     macOS Keychain login. Select the folder you set as CLAUDE_CONFIG_DIR when \
                     you logged in to Claude.",
                    source_dir.display()
                )))
            }
        }
        Agent::Codex => require_credential_file(source_dir, "auth.json", "Codex"),
        Agent::Grok => require_credential_file(source_dir, "auth.json", "Grok"),
        Agent::Opencode => require_credential_file(source_dir, "auth.json", "OpenCode"),
        Agent::Amp => require_credential_file(source_dir, "secrets.json", "Amp"),
        // Kimi syncs a directory tree rather than a single file.
        Agent::Kimi => {
            if source_dir.join("config.toml").is_file() && source_dir.join("credentials").is_dir() {
                Ok(())
            } else {
                Err(SyncSourceValidationError::new(format!(
                    "Not a Kimi config folder: {} must contain config.toml and a credentials/ \
                     directory.",
                    source_dir.display()
                )))
            }
        }
    }
}

/// Require a non-empty credential file named `name` directly inside `dir`.
fn require_credential_file(
    dir: &Path,
    name: &str,
    agent: &str,
) -> Result<(), SyncSourceValidationError> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(content) if !content.trim().is_empty() => Ok(()),
        Ok(_) => Err(SyncSourceValidationError::new(format!(
            "{agent} credential {name} in {} is empty.",
            dir.display()
        ))),
        Err(_) => Err(SyncSourceValidationError::new(format!(
            "Not a {agent} config folder: expected {name} directly inside {}.",
            dir.display()
        ))),
    }
}

impl RoleState {
    /// Provision Codex auth state. Runtime policy is passed as CLI
    /// flags by the entrypoint rather than generated into
    /// `~/.codex/config.toml`.
    ///
    /// `auth.json` semantics mirror Claude's `.credentials.json` for
    /// the file-mount surface; in-container `codex login` writes are
    /// only persisted across container removal when a sync mount
    /// already exists at launch (a host file at `~/.codex/auth.json`).
    ///   * **Sync** + host file present → copy with `0600` perms,
    ///     return `Synced`.
    ///   * **Sync** + host file absent → leave any existing role-state
    ///     `auth.json` untouched (it may survive from a prior synced
    ///     run), return `HostMissing`.
    ///   * **`ApiKey`** → wipe the role-state `auth.json` (the agent
    ///     authenticates via `OPENAI_API_KEY`; a forwarded auth.json
    ///     would let it silently fall back to OAuth credentials the
    ///     operator chose to bypass), return `TokenMode`.
    ///   * **`OAuthToken`** → unreachable in production: parser-rejected
    ///     for Codex. Defensive arm returns `TokenMode` without
    ///     touching role-state files.
    ///   * **Ignore** → delete the role-state `auth.json` if present,
    ///     return `Skipped`.
    ///
    /// Returns `(outcome, mounted_auth_json)` where `mounted_auth_json` is
    /// the role-state `auth.json` path when it should be bind-mounted into
    /// the container (file exists post-call), or `None` when the mount must
    /// be skipped (Ignore wiped it / Sync host-missing with no prior file /
    /// Token mode with no prior file). Centralising the decision here means
    /// `RoleState::prepare` does not need to re-stat the file or reason
    /// about which outcome implies which mount state.
    pub(super) fn provision_codex_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &host_home.join(".codex/auth.json"))
    }

    pub(super) fn provision_codex_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_codex_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        // OAuthToken is parser-rejected for Codex (unreachable in production),
        // so no warning is needed. Codex has no empty/whitespace content guard.
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Codex auth.json",
            "Codex",
            false,
            false,
            false,
        )
    }
}

impl RoleState {
    /// Provision GitHub CLI auth state for the role-state directory.
    ///
    /// `hosts_yml` is the role-state location of `.config/gh/hosts.yml`
    /// (the directory itself is bind-mounted RW into the container under
    /// `/home/agent/.config/gh`, so writing the file directly into that
    /// directory is enough — no separate file mount).
    ///
    ///   * **Sync** + host token resolved → write `hosts.yml` 0o600,
    ///     return `Synced { token, source }` with `source` naming
    ///     which host path produced the token (`gh` CLI vs file
    ///     fallback).
    ///   * **Sync** + host token absent → leave any existing
    ///     `hosts.yml` untouched (preserves in-container login from a
    ///     prior run), return `HostMissing { reason }` with the typed
    ///     reason (`NoGhAndNoHostsFile` / `GhCliFailed { stderr }` /
    ///     `GhCliEmpty` / `HostsFileMalformed`).
    ///   * **Token** → wipe any prior `hosts.yml` (so a stale
    ///     file-based login can't shadow the env token), return
    ///     `TokenMode { token }` with the operator-resolved value.
    ///   * **Ignore** → wipe any prior `hosts.yml`, return `Skipped`.
    ///
    /// On `Sync`-host-missing the existing in-container login is
    /// preserved deliberately — otherwise an operator who logged out
    /// on the host would lose the container's login on the next
    /// launch.
    pub(super) fn provision_github_auth(
        hosts_yml: &Path,
        github: &GithubAuthContext,
        host_home: &Path,
    ) -> anyhow::Result<GithubProvisionOutcome> {
        // Reject pre-existing symlinks before branching on mode. The
        // role-state dir is bind-mounted RW, so a compromised role could
        // plant a symlink between launches; calling reject_symlink
        // unconditionally is fine — it lstat's and no-ops on ENOENT.
        reject_symlink(hosts_yml)?;

        match github.mode {
            GithubAuthMode::Ignore => {
                wipe_file_if_present(hosts_yml)?;
                Ok(GithubProvisionOutcome::Skipped)
            }
            GithubAuthMode::Token => {
                wipe_file_if_present(hosts_yml)?;
                let token = github.token.clone().unwrap_or_default();
                Ok(GithubProvisionOutcome::TokenMode { token })
            }
            GithubAuthMode::Sync => {
                let resolved = if let Some(token) = github
                    .token
                    .as_ref()
                    .filter(|token| !token.trim().is_empty())
                {
                    HostGhResolution::Resolved(HostGhAuth {
                        token: token.clone(),
                        user: None,
                        source: GithubTokenSource::ConfiguredEnv,
                    })
                } else {
                    read_host_gh_token(host_home)?
                };
                match resolved {
                    HostGhResolution::Resolved(resolved) => {
                        let content = render_hosts_yml(&resolved.token, resolved.user.as_deref());
                        // Skip the write when content matches what's already
                        // on disk — avoids touching mtime + atomic-rename on
                        // every launch when nothing changed. Mirrors the
                        // codex provisioner's no-churn guard.
                        let needs_write = !std::fs::read_to_string(hosts_yml)
                            .is_ok_and(|existing| existing == content);
                        if needs_write {
                            write_private_file(hosts_yml, &content)?;
                        } else {
                            repair_permissions(hosts_yml);
                        }
                        Ok(GithubProvisionOutcome::Synced {
                            token: resolved.token,
                            source: resolved.source,
                        })
                    }
                    HostGhResolution::Missing(reason) => {
                        repair_permissions(hosts_yml);
                        Ok(GithubProvisionOutcome::HostMissing { reason })
                    }
                }
            }
        }
    }
}

/// Render a minimal `hosts.yml` body for the `github.com` host. `user`
/// is optional and falls back to a placeholder — gh accepts hosts.yml
/// without it, but writing a value keeps the file shape uniform.
fn render_hosts_yml(token: &str, user: Option<&str>) -> String {
    let user_field = user.filter(|s| !s.trim().is_empty()).unwrap_or("git");
    format!(
        "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: {user_field}\n",
    )
}

/// Resolved host-side `gh` auth + which source produced it, so the
/// caller can attribute the value in the launch summary.
struct HostGhAuth {
    token: String,
    user: Option<String>,
    source: GithubTokenSource,
}

/// Result of the host-side resolver. `Missing` carries the typed
/// reason so the launch-summary line can render the actual cause
/// instead of guessing "host logged out".
enum HostGhResolution {
    Resolved(HostGhAuth),
    Missing(HostMissingReason),
}

/// Wipe a file if it exists, ignoring `NotFound` so the call is
/// idempotent without a pre-stat that races with the unlink.
fn wipe_file_if_present(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// True when `host_home` is the operator's real home directory. Gates
/// the host-binary shellouts so hermetic tests with a temp-dir
/// `host_home` cannot leak to the real `gh` binary.
fn host_home_is_real(host_home: &Path) -> bool {
    let real_home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    real_home.as_deref() == Some(host_home)
}

/// Read the host's `gh` token, returning a typed reason when neither
/// source resolves so the launch-summary line can render an accurate
/// cause. Priority order:
///
/// 1. `gh auth token --hostname github.com` — Keychain-aware, only
///    consulted when `host_home` is the real home directory.
/// 2. `~/.config/gh/hosts.yml` parse — works without `gh` on PATH.
fn read_host_gh_token(host_home: &Path) -> anyhow::Result<HostGhResolution> {
    // Read hosts.yml once up front so both the CLI-success path (which
    // reads it for the `user` field) and the file-fallback path share
    // one IO.
    let hosts_path = host_home.join(".config/gh/hosts.yml");
    let hosts_yml = match std::fs::read_to_string(&hosts_path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(InstanceError::HostConfigRead {
                path: hosts_path,
                source: e,
            }
            .into());
        }
    };

    let mut cli_failure: Option<HostMissingReason> = None;

    if host_home_is_real(host_home) {
        #[expect(
            clippy::disallowed_methods,
            reason = "GitHub auth provisioning is called from spawn_blocking during launch"
        )]
        match std::process::Command::new("gh")
            .args(["auth", "token", "--hostname", "github.com"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if !token.is_empty() {
                    let user = hosts_yml
                        .as_deref()
                        .and_then(parse_gh_hosts_yml)
                        .and_then(|parsed| parsed.user);
                    return Ok(HostGhResolution::Resolved(HostGhAuth {
                        token,
                        user,
                        source: GithubTokenSource::GhCli,
                    }));
                }
                cli_failure = Some(HostMissingReason::GhCliEmpty);
                jackin_diagnostics::debug_log!(
                    "github_auth",
                    "gh auth token returned empty stdout; falling back to hosts.yml parse"
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                jackin_diagnostics::debug_log!(
                    "github_auth",
                    "gh auth token exited non-zero ({}); stderr={stderr}",
                    output.status,
                );
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: stderr.trim().to_owned(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                jackin_diagnostics::debug_log!("github_auth", "gh not on PATH: {e}");
            }
            Err(e) => {
                jackin_diagnostics::debug_log!(
                    "github_auth",
                    "gh auth token spawn failed ({:?}): {e}",
                    e.kind()
                );
                // Treat any non-NotFound spawn error as a CLI failure
                // signal too — the operator's gh is in a broken state
                // and the launch notice should say so.
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: e.to_string(),
                });
            }
        }
    }

    let Some(text) = hosts_yml else {
        return Ok(HostGhResolution::Missing(
            cli_failure.unwrap_or(HostMissingReason::NoGhAndNoHostsFile),
        ));
    };
    if let Some(mut parsed) = parse_gh_hosts_yml(&text) {
        parsed.source = GithubTokenSource::HostsFile;
        return Ok(HostGhResolution::Resolved(parsed));
    }
    jackin_diagnostics::debug_log!(
        "github_auth",
        "hosts.yml at {} did not yield a github.com oauth_token",
        hosts_path.display()
    );
    // CLI failure (when known) is the more actionable signal than
    // "file malformed" — surface it instead.
    Ok(HostGhResolution::Missing(
        cli_failure.unwrap_or(HostMissingReason::HostsFileMalformed),
    ))
}

/// Parse `gh`'s `hosts.yml`, extracting the `github.com.oauth_token`
/// and (best-effort) `github.com.user` fields via `serde_yaml_ng` so
/// quoting, escapes, comments, and indent rules track the YAML 1.x
/// spec rather than a hand-rolled scanner.
///
/// Returns `None` when the document doesn't carry a `github.com` block
/// with a non-empty `oauth_token` field, or when the document is
/// malformed. Malformed input must NOT yield a partial result —
/// silently accepting half-parsed scalars would land bogus credentials
/// in `hosts.yml` and surface as unrelated 401s mid-session.
fn parse_gh_hosts_yml(text: &str) -> Option<HostGhAuth> {
    #[derive(serde::Deserialize)]
    struct HostsFile {
        // `gh` writes the host header literally as `github.com:`, so
        // the top-level map key is `github.com`.
        #[serde(default, rename = "github.com")]
        github_com: Option<HostEntry>,
    }
    #[derive(serde::Deserialize)]
    struct HostEntry {
        #[serde(default)]
        oauth_token: Option<String>,
        #[serde(default)]
        user: Option<String>,
    }

    let parsed: HostsFile = match serde_yaml_ng::from_str(text) {
        Ok(p) => p,
        Err(e) => {
            jackin_diagnostics::debug_log!(
                "github_auth",
                "hosts.yml YAML parse failed: {e}; will fall through to HostsFileMalformed"
            );
            return None;
        }
    };
    let entry = parsed.github_com?;
    let token = entry.oauth_token.filter(|s| !s.trim().is_empty())?;
    Some(HostGhAuth {
        token,
        user: entry.user.filter(|s| !s.trim().is_empty()),
        // Caller (`read_host_gh_token` file-fallback path) overwrites
        // this with the right `GithubTokenSource` variant; the field
        // gets a placeholder so the struct literal compiles.
        source: GithubTokenSource::HostsFile,
    })
}

impl RoleState {
    /// Provision Claude's host-side auth files (`account_json` and
    /// `credentials_json`) according to the chosen auth-forwarding
    /// strategy and report whether the files should be bind-mounted
    /// into the container under `/jackin/claude/`.
    ///
    /// Returns `(outcome, forward_auth)`. `forward_auth` controls
    /// whether the launcher will bind-mount the files; the underlying
    /// host paths are unconditionally tracked on `RoleState` so callers
    /// can still inspect them (tests, debug output, future migration).
    ///
    ///   * **Sync** + host file present → write both files at `0o600`,
    ///     `forward_auth = true`. Container auth flows from host.
    ///   * **Sync** + host file absent → preserve any existing role-
    ///     state files (may carry forward an in-container login),
    ///     `forward_auth = true`. The launcher then mounts only the
    ///     files that actually exist on disk.
    ///   * **`OAuthToken`** → remove any forwarded `credentials.json`
    ///     (revokes prior Sync state) and write a
    ///     `{"hasCompletedOnboarding":true}` skeleton at `account_json`,
    ///     `forward_auth = true`. The skeleton suppresses the CLI's
    ///     "Select login method" wizard so it reads the
    ///     `CLAUDE_CODE_OAUTH_TOKEN` env var instead.
    ///   * **`ApiKey`/`Ignore`** → wipe both role-state files and
    ///     `forward_auth = false`. `ApiKey` authenticates via
    ///     `ANTHROPIC_API_KEY`; `Ignore` forces a fresh login inside
    ///     the durable per-instance agent home.
    ///
    /// On macOS the host credentials live in the system Keychain
    /// ("Claude Code-credentials"), not in a file. On Linux they are
    /// stored at `~/.claude/.credentials.json`.
    pub(super) fn provision_claude_auth(
        account_json: &Path,
        credentials_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        let host_claude_json = host_home.join(".claude.json");

        let outcome = match mode {
            AuthForwardMode::Ignore => {
                // Always ensure a clean slate — if switching from sync/token
                // to ignore, the previously forwarded credentials must be
                // revoked.
                wipe_claude_state(account_json, credentials_json)?;
                AuthProvisionOutcome::Skipped
            }
            // ApiKey: wipe any forwarded host creds; agent authenticates
            // via ANTHROPIC_API_KEY in the env. No skeleton needed —
            // console-API auth path does not require ~/.claude.json.
            AuthForwardMode::ApiKey => {
                wipe_claude_state(account_json, credentials_json)?;
                AuthProvisionOutcome::Skipped
            }
            // OAuthToken: write a minimal skeleton so the Claude CLI skips
            // its interactive login wizard and reads CLAUDE_CODE_OAUTH_TOKEN
            // from the env instead. Without this file, the CLI shows the
            // "Select login method" prompt even when the env var is set.
            AuthForwardMode::OAuthToken => {
                if credentials_json.exists() {
                    std::fs::remove_file(credentials_json)?;
                }
                write_private_file(account_json, r#"{"hasCompletedOnboarding":true}"#)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Sync => {
                if let Some(creds) = read_host_credentials(host_home) {
                    copy_host_claude_json(&host_claude_json, account_json)?;
                    write_private_file(credentials_json, &creds)?;
                    AuthProvisionOutcome::Synced
                } else {
                    // Host has no auth — leave the container's existing
                    // files untouched (they may carry credentials from a
                    // previous manual login). Bootstrap an empty
                    // account.json if nothing exists yet so the file is
                    // always present after `prepare`, simplifying
                    // inspection callers.
                    if !account_json.exists() {
                        write_private_file(account_json, "{}")?;
                    }
                    // Repair permissions on pre-existing auth files that
                    // may have legacy permissive modes (e.g. 0644).
                    repair_permissions(account_json);
                    repair_permissions(credentials_json);
                    AuthProvisionOutcome::HostMissing
                }
            }
        };

        // Sync and token modes forward auth state (the launcher checks
        // file existence at mount time). ApiKey and Ignore do not.
        let forward_auth = matches!(
            outcome,
            AuthProvisionOutcome::Synced
                | AuthProvisionOutcome::HostMissing
                | AuthProvisionOutcome::TokenMode
        );
        Ok((outcome, forward_auth))
    }

    pub(super) fn provision_claude_auth_from_config_dir(
        account_json: &Path,
        credentials_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        let host_claude_json = source_dir.join(".claude.json");

        let outcome = match mode {
            AuthForwardMode::Ignore | AuthForwardMode::ApiKey => {
                wipe_claude_state(account_json, credentials_json)?;
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::OAuthToken => {
                if credentials_json.exists() {
                    std::fs::remove_file(credentials_json)?;
                }
                write_private_file(account_json, r#"{"hasCompletedOnboarding":true}"#)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Sync => {
                // Read ONLY the selected source folder's credentials. An
                // explicit source dir must never fall back to the default
                // host `~/.claude` / default Keychain account — that leak
                // is exactly the bug this path guards against (an operator
                // who picked an Enterprise source folder would otherwise
                // get their default Max account inside the capsule).
                if let Some(creds) =
                    read_host_credentials_from_claude_config_dir(source_dir, host_home)
                {
                    copy_host_claude_json(&host_claude_json, account_json)?;
                    write_private_file(credentials_json, &creds)?;
                    AuthProvisionOutcome::Synced
                } else {
                    eprintln!(
                        "[jackin] Claude source folder {} has no readable credentials — \
                         leaving unauthenticated (no fallback to the default account)",
                        source_dir.display()
                    );
                    if !account_json.exists() {
                        write_private_file(account_json, "{}")?;
                    }
                    repair_permissions(account_json);
                    repair_permissions(credentials_json);
                    AuthProvisionOutcome::HostMissing
                }
            }
        };

        let forward_auth = matches!(
            outcome,
            AuthProvisionOutcome::Synced
                | AuthProvisionOutcome::HostMissing
                | AuthProvisionOutcome::TokenMode
        );
        Ok((outcome, forward_auth))
    }
}

impl RoleState {
    /// Provision Amp's host-side `secrets.json` per the chosen mode.
    ///
    /// Source: `~/.local/share/amp/secrets.json` (`XDG_DATA`). The
    /// `XDG_CONFIG` `~/.config/amp/settings.json` is preferences only
    /// and never holds the token.
    ///
    /// `mounted_secrets_json` is `None` when the bind mount must be
    /// skipped. `Sync` with no host file preserves any prior role-state
    /// file so an in-container login isn't silently dropped.
    /// `OAuthToken` is parser-rejected; the defensive arm wipes + logs
    /// so a bypass is loud rather than silent.
    pub(super) fn provision_amp_auth(
        secrets_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_amp_auth_from_path(
            secrets_json,
            mode,
            &host_home.join(".local/share/amp/secrets.json"),
        )
    }

    pub(super) fn provision_amp_auth_from_source_dir(
        secrets_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_amp_auth_from_path(secrets_json, mode, &source_dir.join("secrets.json"))
    }

    fn provision_amp_auth_from_path(
        secrets_json: &Path,
        mode: AuthForwardMode,
        host_secrets_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        provision_single_file_credential(
            secrets_json,
            host_secrets_json,
            mode,
            "Amp secrets.json",
            "Amp",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Kimi Code's host-side `~/.kimi-code` directory per the chosen mode.
    ///
    /// Sync copies only auth-essential host files into the role-state
    /// directory so it can be bind-mounted into the container. Kimi Code stores
    /// OAuth tokens under `credentials/` (including the `credentials/mcp/`
    /// subtree), `config.toml` carries the OAuth-backed provider/model
    /// references created by login, and `device_id` is the host-bound identity
    /// Kimi sends in OAuth/device headers.
    /// `ApiKey` / `Ignore` wipe any prior role-state directory.
    ///
    ///   * **Sync** + `~/.kimi-code` present → copy `config.toml`, the full
    ///     `credentials/` tree (binary-safe, recursive, symlink-safe), and
    ///     `device_id`. Files land at `0600`, directories at `0700`. Return
    ///     `(Synced, true)`.
    ///   * **Sync** + `~/.kimi-code` absent → return `(HostMissing, true)`.
    ///     Unlike Codex and Amp, no prior role-state files are preserved;
    ///     the role-state dir is still created so the bind-mount exists for
    ///     in-container login state to accumulate.
    ///   * **`ApiKey`** → wipe the role-state directory; return
    ///     `(TokenMode, false)`. Agent authenticates via `KIMI_API_KEY`.
    ///   * **`OAuthToken`** → parser-rejected for Kimi; defensive arm
    ///     wipes role-state and logs loudly, returns `(TokenMode, false)`.
    ///   * **Ignore** → wipe the role-state directory; return
    ///     `(Skipped, false)`.
    ///
    /// Kimi syncs a directory rather than a single file, so the second
    /// return value is `bool` rather than `Option<PathBuf>`. `true` when
    /// `Synced` or `HostMissing` (mount the role-state dir); `false` for
    /// `TokenMode` / `Skipped` (dir was wiped, do not mount).
    pub(super) fn provision_kimi_auth(
        kimi_dir: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        Self::provision_kimi_auth_from_source_dir(kimi_dir, mode, &host_home.join(".kimi-code"))
    }

    pub(super) fn provision_kimi_auth_from_source_dir(
        kimi_dir: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        provision_kimi_dir_credential(
            kimi_dir,
            source_dir,
            mode,
            KIMI_SYNC_FILES,
            "Kimi dir",
            "Kimi",
        )
    }
}

/// Generic directory credential provisioner for agents that sync a directory
/// tree with standard `AuthForwardMode` semantics (OAuthToken/ApiKey/Ignore
/// wipe the dir; Sync copies `sync_files` + a `credentials/` subtree).
///
/// Returns `(outcome, forward_auth)` where `forward_auth` is `true` when
/// the role-state directory should be bind-mounted into the container
/// (`Synced` or `HostMissing`), and `false` when it was wiped.
fn provision_kimi_dir_credential(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
    sync_files: &[&str],
    _label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    use anyhow::Context;

    reject_symlink(target_dir)?;

    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            eprintln!(
                "[jackin] internal: {agent_name} provision received unsupported \
                 OAuthToken mode — parser invariant bypassed; \
                 wiping role state and falling back to token-mode."
            );
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => {
            std::fs::create_dir_all(target_dir)?;

            if host_dir.exists() {
                for name in sync_files {
                    let host_file = host_dir.join(name);
                    if host_file.exists() {
                        let content = std::fs::read_to_string(&host_file)
                            .with_context(|| format!("reading {}", host_file.display()))?;
                        write_private_file(&target_dir.join(name), &content)?;
                    }
                }

                let host_creds = host_dir.join("credentials");
                if host_creds.exists() {
                    let dest_creds = target_dir.join("credentials");
                    copy_kimi_credentials_tree(&host_creds, &dest_creds)
                        .with_context(|| format!("copying {}", host_creds.display()))?;
                }

                AuthProvisionOutcome::Synced
            } else {
                AuthProvisionOutcome::HostMissing
            }
        }
    };

    let forward_auth = matches!(
        outcome,
        AuthProvisionOutcome::Synced | AuthProvisionOutcome::HostMissing
    );
    Ok((outcome, forward_auth))
}

/// Single-file host artifacts forwarded into the role-state directory under
/// `Sync` mode. Listed once so a future addition (or removal) only has to be
/// made here, not threaded through three near-identical copy blocks.
const KIMI_SYNC_FILES: &[&str] = &["config.toml", "device_id"];

/// Recursively copy a Kimi Code credentials tree. Symlinks are skipped with
/// a warning (never followed). Files land at `0o600`, directories at `0o700`.
fn copy_kimi_credentials_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dst, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 {}", dst.display()))?;
    }
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ft.is_symlink() {
            // Route via the TUI-safe channel: the rich loading cockpit owns
            // the terminal while this runs (credentials stage). A bare
            // `eprintln!` would corrupt the cockpit. `emit_compact_line`
            // lands in the diagnostics run jsonl and only prints to stderr
            // when no rich surface is active.
            jackin_diagnostics::emit_compact_line(
                "kimi-auth",
                &format!(
                    "skipping symlink {} under ~/.kimi-code/credentials/ — symlinks are not synced",
                    entry.file_name().to_string_lossy()
                ),
            );
        } else if ft.is_dir() {
            copy_kimi_credentials_tree(&src_path, &dst_path)?;
        } else if ft.is_file() {
            let bytes = std::fs::read(&src_path)
                .with_context(|| format!("reading {}", src_path.display()))?;
            write_private_bytes(&dst_path, &bytes)?;
        }
    }
    Ok(())
}

impl RoleState {
    /// Provision `OpenCode`'s host-side `auth.json` per the chosen mode.
    ///
    /// Source: `~/.local/share/opencode/auth.json` (`XDG_DATA`).
    /// `OpenCode` stores provider credentials (e.g. Z.AI Coding Plan API
    /// keys) in this file.
    ///
    /// Follows the same semantics as `provision_amp_auth`.
    pub(super) fn provision_opencode_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_opencode_auth_from_path(
            auth_json,
            mode,
            &host_home.join(".local/share/opencode/auth.json"),
        )
    }

    pub(super) fn provision_opencode_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_opencode_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_opencode_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "OpenCode auth.json",
            "OpenCode",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Grok's host-side `~/.grok/auth.json` per the chosen mode.
    ///
    /// The auth.json carries OAuth / OIDC tokens (from `grok login`) and is
    /// the handoff for the browser-based login flow. `GROK_DEPLOYMENT_KEY` or
    /// `XAI_API_KEY` in the env take precedence inside the CLI (per install
    /// script and docs); when present we still allow a Sync mount so any
    /// supplementary config or prior tokens are available, but ApiKey/Ignore
    /// correctly suppress the file to force env-only auth.
    pub(super) fn provision_grok_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &host_home.join(".grok/auth.json"))
    }

    pub(super) fn provision_grok_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_grok_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Grok auth.json",
            "Grok",
            true,
            true,
            true,
        )
    }
}

/// Shared file-credential provisioner for agents that use a single JSON
/// credential file with standard `AuthForwardMode` semantics.
///
/// `treat_empty_as_missing` — when `true`, an empty/whitespace file on the
/// host is treated as host-missing. When `false`, an empty file is written as-is.
///
/// `warn_on_oauth` — when `true`, receiving `OAuthToken` mode logs a warning
/// that the parser invariant was bypassed. When `false`, `OAuthToken`
/// silently returns `TokenMode`.
///
/// `wipe_on_oauth` — when `true`, the role-state file is wiped on `OAuthToken`.
/// When `false`, the existing file is preserved (Codex: `OAuthToken` is a
/// parser-rejected no-op; preserving the file allows recovery from a bypass).
#[allow(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
fn provision_single_file_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
    treat_empty_as_missing: bool,
    warn_on_oauth: bool,
    wipe_on_oauth: bool,
) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
    use anyhow::Context;

    reject_symlink(target)?;

    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            if warn_on_oauth {
                eprintln!(
                    "[jackin] internal: {agent_name} provision received unsupported \
                     OAuthToken mode — parser invariant bypassed; \
                     wiping role state and falling back to token-mode."
                );
            }
            if wipe_on_oauth {
                wipe_agent_file_state(target, label)?;
            }
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => match std::fs::read_to_string(host_path) {
            Ok(content) if treat_empty_as_missing && content.trim().is_empty() => {
                eprintln!(
                    "[jackin] host {} is empty/whitespace — treating as host-missing",
                    host_path.display()
                );
                if target.exists() {
                    repair_permissions(target);
                }
                AuthProvisionOutcome::HostMissing
            }
            Ok(content) => {
                // No-churn guard: skip the atomic write when the role-state
                // file already holds identical content. `write_private_file`
                // replaces the inode (temp + rename); on macOS that
                // invalidates a live single-file bind mount into the running
                // container. The background sibling-auth prewarm
                // (`prewarm_auth_for_agents`, spawned during launch) re-runs
                // this for already-foreground-provisioned agents, so an
                // unconditional rename races `docker create` and silently
                // breaks the foreground container's auth mounts — leaving the
                // sibling agent unauthenticated. Mirrors the GitHub
                // provisioner's no-churn guard.
                let unchanged =
                    std::fs::read_to_string(target).is_ok_and(|existing| existing == content);
                if unchanged {
                    repair_permissions(target);
                } else {
                    write_private_file(target, &content).with_context(|| {
                        format!(
                            "failed to write {agent_name} role-state {label} at {}",
                            target.display()
                        )
                    })?;
                }
                AuthProvisionOutcome::Synced
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if target.exists() {
                    repair_permissions(target);
                }
                AuthProvisionOutcome::HostMissing
            }
            Err(e) => {
                let hint = match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        " (check host file permissions on the parent dir)"
                    }
                    _ => "",
                };
                return Err(anyhow::Error::new(e).context(format!(
                    "failed to read host {}{}",
                    host_path.display(),
                    hint
                )));
            }
        },
    };

    let mounted = match outcome {
        AuthProvisionOutcome::Synced => Some(target.to_path_buf()),
        AuthProvisionOutcome::Skipped => None,
        AuthProvisionOutcome::HostMissing | AuthProvisionOutcome::TokenMode => {
            target.exists().then(|| target.to_path_buf())
        }
    };
    Ok((outcome, mounted))
}

/// Wipe a single credential file from role state.
///
/// `label` names the agent + file for the operator-visible error message
/// (e.g. `"Amp secrets.json"`, `"OpenCode auth.json"`).
fn wipe_agent_file_state(path: &Path, label: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    wipe_file_if_present(path).with_context(|| {
        format!(
            "failed to wipe stale {label} at {} \
             (auth_forward switched to ignore/api_key); remove the file \
             manually if it has unexpected ownership",
            path.display()
        )
    })
}

/// Remove role-state Kimi auth files so a prior Sync run cannot leak
/// credentials under env-driven modes.
fn wipe_kimi_state(kimi_dir: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    if kimi_dir.exists() {
        std::fs::remove_dir_all(kimi_dir).with_context(|| {
            format!(
                "failed to wipe stale Kimi state at {} \
                 (auth_forward switched to ignore/api_key); remove the directory \
                 manually if it has unexpected ownership",
                kimi_dir.display()
            )
        })?;
    }
    Ok(())
}

/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = match std::fs::read_to_string(host_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
        Err(e) => {
            jackin_diagnostics::debug_log!(
                "auth",
                "failed to read Claude account metadata at {} while forwarding credentials: {e}",
                host_path.display()
            );
            return Err(anyhow::Error::new(e).context(format!(
                "reading Claude account metadata at {}",
                host_path.display()
            )));
        }
    };
    write_private_file(dest_path, &content)
}

/// Wipe the container's Claude auth state to a clean empty shape.
///
/// Used by every non-Sync mode (`Ignore`, `OAuthToken`, `ApiKey`) — they
/// all must guarantee no stale `.credentials.json` survives from a
/// prior Sync run, and that `.claude.json` is `{}` so Claude Code
/// inside the container authenticates exclusively via env vars (or
/// fresh login) rather than re-using forwarded credentials.
///
/// `account_json` is rewritten only when its current contents differ
/// from `{}` (or the file doesn't exist), to avoid touching mtime on
/// every launch.
fn wipe_claude_state(account_json: &Path, credentials_json: &Path) -> anyhow::Result<()> {
    if !account_json.exists() || std::fs::read_to_string(account_json)? != "{}" {
        write_private_file(account_json, "{}")?;
    }
    if credentials_json.exists() {
        std::fs::remove_file(credentials_json)?;
    }
    Ok(())
}

/// Read a Claude `.credentials.json` file, treating empty/whitespace as
/// absent so a blank file neither shadows the macOS Keychain fallback nor
/// provisions the capsule with credentials that boot the agent
/// unauthenticated. A read error on a file the operator explicitly selected
/// (permissions, IO) is a real failure — log it rather than folding it into
/// the silent not-found path; only `NotFound` is treated as "no file here".
fn read_nonempty_credentials_file(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(content) if !content.trim().is_empty() => Some(content),
        Ok(_) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("[jackin] warning: failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Read the host's Claude Code OAuth credentials for the default
/// `~/.claude` config dir.
///
/// Checks the file-based store at `~/.claude/.credentials.json` first
/// (used on Linux, and makes the function testable with temp dirs).
/// Falls back to the macOS Keychain ("Claude Code-credentials") when
/// the file is absent and `host_home` matches the real home directory.
fn read_host_credentials(host_home: &Path) -> Option<String> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = host_home.join(".claude/.credentials.json");
    if let Some(content) = read_nonempty_credentials_file(&creds_path) {
        return Some(content);
    }

    // macOS Keychain fallback — only attempted when host_home is the
    // real home directory.  This keeps tests hermetic (they use temp
    // dirs) while still supporting the Keychain in production.
    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        return read_claude_keychain(CLAUDE_KEYCHAIN_SERVICE_BASE);
    }

    None
}

/// Read the host's Claude Code OAuth credentials for an explicit
/// `CLAUDE_CONFIG_DIR` source folder (Workspace Auth sync mode).
///
/// Reads ONLY credentials belonging to `source_dir`: the file-based
/// `source_dir/.credentials.json` first, then — on macOS — the Keychain
/// entry Claude Code provisions for that specific config dir. It never
/// falls back to the default `~/.claude` credentials or the default
/// Keychain service; an operator who selected a source folder must get
/// that folder's account (e.g. a company Enterprise login) or nothing,
/// never the default Max account leaking in from the host.
fn read_host_credentials_from_claude_config_dir(
    source_dir: &Path,
    host_home: &Path,
) -> Option<String> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = source_dir.join(".credentials.json");
    if let Some(content) = read_nonempty_credentials_file(&creds_path) {
        return Some(content);
    }

    // macOS Keychain — Claude Code stores per-config-dir credentials
    // under a service name derived from the config dir path. Gated on the
    // real home directory so tests stay hermetic (temp dirs never shell
    // out to `security`).
    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        let service = claude_keychain_service_for_config_dir(source_dir, host_home);
        return read_claude_keychain(&service);
    }

    #[cfg(not(target_os = "macos"))]
    let _ = host_home;
    None
}

/// Base macOS Keychain service name Claude Code uses for the default
/// `~/.claude` config dir.
#[cfg(target_os = "macos")]
const CLAUDE_KEYCHAIN_SERVICE_BASE: &str = "Claude Code-credentials";

/// Read a credential blob from the macOS login Keychain under `service`.
/// Returns `None` on lookup failure or an empty value.
#[cfg(target_os = "macos")]
fn read_claude_keychain(service: &str) -> Option<String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "macOS Keychain read runs inside spawn_blocking during launch"
    )]
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .ok()?;
    if output.status.success() {
        let creds = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !creds.is_empty() {
            return Some(creds);
        }
    }
    None
}

/// Derive the macOS Keychain service name Claude Code uses for a given
/// `CLAUDE_CONFIG_DIR`.
///
/// Claude Code keys the default `~/.claude` config dir under the bare
/// `"Claude Code-credentials"` service, and every other config dir under
/// `"Claude Code-credentials-<suffix>"` where `<suffix>` is the first
/// eight hex chars (four bytes) of the SHA-256 of the absolute config dir
/// path. Verified against a live Keychain entry (`~/.claude-work`
/// → `…-3342f2c7`).
#[cfg(target_os = "macos")]
fn claude_keychain_service_for_config_dir(source_dir: &Path, host_home: &Path) -> String {
    use sha2::{Digest, Sha256};

    // The default config dir uses the bare service name, not a suffix.
    if source_dir == host_home.join(".claude") {
        return CLAUDE_KEYCHAIN_SERVICE_BASE.to_owned();
    }

    let digest = Sha256::digest(source_dir.to_string_lossy().as_bytes());
    let mut suffix = hex::encode(digest);
    suffix.truncate(8);
    format!("{CLAUDE_KEYCHAIN_SERVICE_BASE}-{suffix}")
}

/// Reject symlinks at `path` to prevent a compromised role from
/// redirecting host-side writes to arbitrary files.
///
/// The role's `.claude/` directory is mounted read-write into the
/// container, so an role could replace `.credentials.json` with a
/// symlink.  Without this check, the next `write_private_file` or
/// `repair_permissions` call would follow the symlink and overwrite
/// or chmod the target on the host.
fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    // Use symlink_metadata (lstat) — regular metadata() follows symlinks.
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "refusing to write through symlink at {}; \
             this may indicate a compromised role state — \
             remove the symlink and retry",
            path.display()
        );
    }
    Ok(())
}

/// Write a file with restricted permissions (`0o600` on Unix) since it
/// may contain authentication credentials.
///
/// Rejects symlinks to prevent a compromised role from redirecting
/// writes to arbitrary host paths.  Uses `tempfile::NamedTempFile` to
/// create an unpredictable temp file (opened with `O_EXCL`, so a
/// pre-planted symlink at the temp path is impossible), then renames
/// it to the destination — closing the TOCTOU window entirely.
fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    write_private_bytes(path, content.as_bytes())
}

/// Write raw bytes to `path` with `0o600` permissions, symlink-safe and atomic.
fn write_private_bytes(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    reject_symlink(path)?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let parent = path
            .parent()
            .ok_or_else(|| InstanceError::NoParentDirectory {
                path: path.to_path_buf(),
            })?;

        // NamedTempFile uses O_EXCL internally, so it will never follow
        // a pre-planted symlink.  The random suffix makes the path
        // unpredictable.
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(content)?;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
        tmp.persist(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)?;
    }
    Ok(())
}

/// Create `path` with `content` at `0o600` only when it does not yet exist.
///
/// Race-free via `O_CREAT|O_EXCL`; on `EEXIST` (file already present)
/// the function returns `Ok(())` and leaves the existing content
/// untouched. Use when a process-private skeleton must be seeded
/// before a downstream consumer (e.g. the Claude CLI) may persist
/// real state into the same path.
pub(super) fn create_private_file_if_absent(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "auth file provisioning is called from spawn_blocking during launch"
    )]
    match opts.open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(content)
                .with_context(|| format!("writing private skeleton at {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(anyhow::Error::new(error)
            .context(format!("creating private skeleton at {}", path.display()))),
    }
}

/// Tighten permissions on an existing file to `0o600`. No-op on
/// symlinks, non-Unix, or missing files. Errors are logged rather
/// than returned: callers are mid-Sync and must not abort the launch,
/// but a silent chmod failure on a credential file is a security
/// regression.
fn repair_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    eprintln!(
                        "[jackin] warning: refusing to chmod symlink at {}",
                        path.display()
                    );
                    return;
                }
                if meta.is_file()
                    && let Err(e) =
                        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                {
                    eprintln!(
                        "[jackin] warning: failed to chmod 0o600 on {}: {e}",
                        path.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!("[jackin] warning: stat failed on {}: {e}", path.display());
            }
        }
    }
    #[cfg(not(unix))]
    {
        drop(path);
    }
}

#[cfg(test)]
mod tests;
