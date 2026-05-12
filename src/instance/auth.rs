use super::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason, RoleState,
};
use crate::config::{AuthForwardMode, GithubAuthMode};
use std::path::Path;

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
    ///     for Codex (Task 6). Defensive arm returns `TokenMode` without
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
        auth_json: &std::path::Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<std::path::PathBuf>)> {
        // Reject any pre-existing symlink at the role-state auth.json path
        // BEFORE branching on mode. The host bind-mounts this file RW into
        // the container, so a compromised role could otherwise replace it
        // with a symlink between launches and trick subsequent provisioning
        // calls into reading/writing/deleting through the symlink.
        if auth_json.exists() {
            reject_symlink(auth_json)?;
        }

        let host_auth_json = host_home.join(".codex/auth.json");
        let outcome = match mode {
            // OAuthToken is parser-rejected for Codex (Task 6), so this arm
            // is unreachable in production — kept for match exhaustiveness
            // and to preserve historical no-wipe behavior if a config ever
            // bypasses the parser. Treated as TokenMode without touching
            // role-state files.
            AuthForwardMode::OAuthToken => AuthProvisionOutcome::TokenMode,
            // ApiKey is env-driven (OPENAI_API_KEY): the agent inside the
            // container must NOT see a forwarded auth.json from a prior
            // Sync run, otherwise it would silently fall back to OAuth
            // credentials that the operator has explicitly chosen to
            // bypass. Wipe role-state auth.json identically to Ignore,
            // and surface the env-driven nature via TokenMode.
            AuthForwardMode::ApiKey => {
                wipe_codex_state(auth_json)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Ignore => {
                wipe_codex_state(auth_json)?;
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::Sync => match std::fs::read_to_string(&host_auth_json) {
                Ok(content) => {
                    write_private_file(auth_json, &content)?;
                    AuthProvisionOutcome::Synced
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if auth_json.exists() {
                        repair_permissions(auth_json);
                    }
                    AuthProvisionOutcome::HostMissing
                }
                Err(e) => {
                    // Preserve `io::Error` source chain so `{e:#}` /
                    // `--debug` exposes the kind (PermissionDenied,
                    // NotADirectory) instead of misdiagnosing as
                    // host-missing.
                    let hint = match e.kind() {
                        std::io::ErrorKind::PermissionDenied => {
                            " (check host file permissions on the parent dir)"
                        }
                        _ => "",
                    };
                    return Err(anyhow::Error::new(e).context(format!(
                        "failed to read host {}{}",
                        host_auth_json.display(),
                        hint
                    )));
                }
            },
        };

        let mounted_auth_json = match outcome {
            AuthProvisionOutcome::Synced => Some(auth_json.to_path_buf()),
            AuthProvisionOutcome::Skipped => None,
            AuthProvisionOutcome::HostMissing | AuthProvisionOutcome::TokenMode => {
                auth_json.exists().then(|| auth_json.to_path_buf())
            }
        };
        Ok((outcome, mounted_auth_json))
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
            GithubAuthMode::Sync => match read_host_gh_token(host_home)? {
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
            },
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
            return Err(anyhow::anyhow!(
                "failed to read host {}: {e} (run with --debug to capture the underlying error)",
                hosts_path.display()
            ));
        }
    };

    let mut cli_failure: Option<HostMissingReason> = None;

    if host_home_is_real(host_home) {
        match std::process::Command::new("gh")
            .args(["auth", "token", "--hostname", "github.com"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
                crate::debug_log!(
                    "github_auth",
                    "gh auth token returned empty stdout; falling back to hosts.yml parse"
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                crate::debug_log!(
                    "github_auth",
                    "gh auth token exited non-zero ({}); stderr={stderr}",
                    output.status,
                );
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: stderr.trim().to_string(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::debug_log!("github_auth", "gh not on PATH: {e}");
            }
            Err(e) => {
                crate::debug_log!(
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
    crate::debug_log!(
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
            crate::debug_log!(
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
        use anyhow::Context;

        // `reject_symlink` no-ops on ENOENT, so no pre-stat needed.
        reject_symlink(secrets_json)?;

        let host_secrets = host_home.join(".local/share/amp/secrets.json");
        let outcome = match mode {
            // Parser-rejected for Amp. Defensive arm: wipe + log loudly
            // so a parser bypass cannot leak prior Sync residue.
            AuthForwardMode::OAuthToken => {
                eprintln!(
                    "[jackin] internal: provision_amp_auth received unsupported \
                     OAuthToken mode for Amp — parser invariant bypassed; \
                     wiping role state and falling back to token-mode."
                );
                wipe_amp_state(secrets_json)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::ApiKey => {
                wipe_amp_state(secrets_json)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Ignore => {
                wipe_amp_state(secrets_json)?;
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::Sync => match std::fs::read_to_string(&host_secrets) {
                Ok(content) if content.trim().is_empty() => {
                    // Empty/whitespace host file would otherwise silently
                    // copy as Synced and the agent would re-prompt login
                    // inside the container with no breadcrumb.
                    eprintln!(
                        "[jackin] host {} is empty/whitespace — treating as host-missing",
                        host_secrets.display()
                    );
                    if secrets_json.exists() {
                        repair_permissions(secrets_json);
                    }
                    AuthProvisionOutcome::HostMissing
                }
                Ok(content) => {
                    write_private_file(secrets_json, &content).with_context(|| {
                        format!(
                            "failed to write Amp role-state secrets.json at {}",
                            secrets_json.display()
                        )
                    })?;
                    AuthProvisionOutcome::Synced
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if secrets_json.exists() {
                        repair_permissions(secrets_json);
                    }
                    AuthProvisionOutcome::HostMissing
                }
                Err(e) => {
                    // Preserve `io::Error` source chain so `{e:#}` /
                    // `--debug` exposes the kind (PermissionDenied,
                    // NotADirectory) instead of misdiagnosing as
                    // host-missing.
                    let hint = match e.kind() {
                        std::io::ErrorKind::PermissionDenied => {
                            " (check host file permissions on the parent dir)"
                        }
                        _ => "",
                    };
                    return Err(anyhow::Error::new(e).context(format!(
                        "failed to read host {}{}",
                        host_secrets.display(),
                        hint
                    )));
                }
            },
        };

        let mounted_secrets_json = match outcome {
            AuthProvisionOutcome::Synced => Some(secrets_json.to_path_buf()),
            AuthProvisionOutcome::Skipped => None,
            AuthProvisionOutcome::HostMissing | AuthProvisionOutcome::TokenMode => {
                secrets_json.exists().then(|| secrets_json.to_path_buf())
            }
        };
        Ok((outcome, mounted_secrets_json))
    }
}

/// Remove role-state `secrets.json` so a prior Sync run cannot leak
/// credentials under env-driven modes (`Ignore`, `ApiKey`).
fn wipe_amp_state(secrets_json: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    wipe_file_if_present(secrets_json).with_context(|| {
        format!(
            "failed to wipe stale Amp secrets.json at {} \
             (auth_forward switched to ignore/api_key); remove the file \
             manually if it has unexpected ownership",
            secrets_json.display()
        )
    })
}

impl RoleState {
    /// Provision Kimi's host-side `~/.kimi` directory per the chosen mode.
    ///
    /// Sync copies the host directory recursively into the role-state
    /// directory so it can be bind-mounted into the container.
    /// ApiKey / Ignore wipe any prior role-state directory.
    ///
    /// Returns `(outcome, forward_auth)` where `forward_auth` controls
    /// whether the launcher bind-mounts the directory.
    pub(super) fn provision_kimi_auth(
        kimi_dir: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        let host_kimi = host_home.join(".kimi");

        let outcome = match mode {
            AuthForwardMode::OAuthToken => {
                eprintln!(
                    "[jackin] internal: provision_kimi_auth received unsupported \
                     OAuthToken mode for Kimi — parser invariant bypassed; \
                     wiping role state and falling back to token-mode."
                );
                wipe_kimi_state(kimi_dir)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::ApiKey => {
                wipe_kimi_state(kimi_dir)?;
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Ignore => {
                wipe_kimi_state(kimi_dir)?;
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::Sync => {
                if host_kimi.exists() {
                    std::fs::create_dir_all(kimi_dir)?;

                    // Selectively copy only auth-essential files.
                    // Skip logs/, sessions/, telemetry/, user-history/,
                    // plans/, kimi.json (host-specific paths), and
                    // latest_version.txt to avoid bloating role state
                    // and leaking session data between containers.

                    // config.toml — OAuth references and model settings
                    let host_config = host_kimi.join("config.toml");
                    if host_config.exists() {
                        let content = std::fs::read_to_string(&host_config)?;
                        write_private_file(&kimi_dir.join("config.toml"), &content)?;
                    }

                    // credentials/ — OAuth tokens (e.g. kimi-code.json)
                    let host_creds = host_kimi.join("credentials");
                    if host_creds.exists() {
                        let dest_creds = kimi_dir.join("credentials");
                        std::fs::create_dir_all(&dest_creds)?;
                        for entry in std::fs::read_dir(&host_creds)? {
                            let entry = entry?;
                            if entry.file_type()?.is_file() {
                                let content = std::fs::read_to_string(&entry.path())?;
                                write_private_file(&dest_creds.join(entry.file_name()), &content)?;
                            }
                        }
                    }

                    // device_id — linked to OAuth tokens
                    let host_device_id = host_kimi.join("device_id");
                    if host_device_id.exists() {
                        let content = std::fs::read_to_string(&host_device_id)?;
                        write_private_file(&kimi_dir.join("device_id"), &content)?;
                    }

                    AuthProvisionOutcome::Synced
                } else {
                    if !kimi_dir.exists() {
                        std::fs::create_dir_all(kimi_dir)?;
                    }
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
}

/// Remove the role-state Kimi directory so a prior Sync run cannot leak
/// credentials under env-driven modes (`Ignore`, `ApiKey`).
fn wipe_kimi_state(kimi_dir: &Path) -> anyhow::Result<()> {
    if kimi_dir.exists() {
        std::fs::remove_dir_all(kimi_dir)?;
    }
    Ok(())
}

/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(host_path).unwrap_or_else(|_| "{}".to_string());
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

/// Wipe the container's Codex auth state to a clean empty shape.
///
/// Used by every non-Sync mode that owns the file (`Ignore`, `ApiKey`)
/// — they must guarantee no stale `auth.json` from a prior Sync run
/// survives so the agent inside the container authenticates exclusively
/// via env vars (or fresh `codex login`) rather than re-using forwarded
/// credentials.
///
/// Unlike `wipe_claude_state`, there is no companion "account" file to
/// reset — Codex's role-state surface is just `auth.json`. Removing it
/// is sufficient because the launcher only bind-mounts `auth.json` when
/// `provision_codex_auth` reports it exists post-call (see
/// `mounted_auth_json` in the caller).
fn wipe_codex_state(auth_json: &Path) -> anyhow::Result<()> {
    if auth_json.exists() {
        std::fs::remove_file(auth_json)?;
    }
    Ok(())
}

/// Read the host's Claude Code OAuth credentials.
///
/// Checks the file-based store at `~/.claude/.credentials.json` first
/// (used on Linux, and makes the function testable with temp dirs).
/// Falls back to the macOS Keychain ("Claude Code-credentials") when
/// the file is absent and `host_home` matches the real home directory.
fn read_host_credentials(host_home: &Path) -> Option<String> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = host_home.join(".claude/.credentials.json");
    if let Ok(content) = std::fs::read_to_string(creds_path) {
        return Some(content);
    }

    // macOS Keychain fallback — only attempted when host_home is the
    // real home directory.  This keeps tests hermetic (they use temp
    // dirs) while still supporting the Keychain in production.
    #[cfg(target_os = "macos")]
    {
        let real_home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        if real_home.as_deref() == Some(host_home) {
            let output = std::process::Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ])
                .output()
                .ok()?;
            if output.status.success() {
                let creds = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !creds.is_empty() {
                    return Some(creds);
                }
            }
        }
    }

    None
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
    reject_symlink(path)?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent directory for {}", path.display()))?;

        // NamedTempFile uses O_EXCL internally, so it will never follow
        // a pre-planted symlink.  The random suffix makes the path
        // unpredictable.
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(content.as_bytes())?;
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
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AuthForwardMode;
    use crate::instance::{AuthProvisionOutcome, RoleState};
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    const TEST_CREDENTIALS: &str =
        r#"{"claudeAiOauth":{"accessToken":"test","refreshToken":"test"}}"#;

    /// Set up a fake host auth environment in the temp dir.
    fn seed_host_auth(temp: &tempfile::TempDir) {
        std::fs::write(
            temp.path().join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
        )
        .unwrap();
        let creds_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&creds_dir).unwrap();
        std::fs::write(creds_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    }

    fn simple_manifest(temp: &tempfile::TempDir) -> crate::manifest::RoleManifest {
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
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

    // ── Auth forwarding tests ───────────────────────────────────────────

    // ── Auth forwarding tests ───────────────────────────────────────────

    #[test]
    fn ignore_mode_writes_empty_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(!state.claude_credentials_json().unwrap().exists());
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    }

    #[test]
    fn sync_mode_copies_host_auth_on_first_run() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert!(
            std::fs::read_to_string(state.claude_account_json().unwrap())
                .unwrap()
                .contains("test@example.com")
        );
        assert_eq!(
            std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
            TEST_CREDENTIALS
        );
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
    }

    #[test]
    fn sync_mode_falls_back_to_empty_json_when_host_has_none() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // No host auth seeded
        let manifest = simple_manifest(&temp);

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(!state.claude_credentials_json().unwrap().exists());
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    }

    #[test]
    fn sync_mode_overwrites_existing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run with host auth
        seed_host_auth(&temp);
        let (state, outcome1) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(outcome1, AuthProvisionOutcome::Synced);

        // Simulate container modifying its own .claude.json
        std::fs::write(
            state.claude_account_json().unwrap(),
            r#"{"container":"data"}"#,
        )
        .unwrap();

        // Update host credentials
        let updated_creds = r#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new"}}"#;
        std::fs::write(temp.path().join(".claude/.credentials.json"), updated_creds).unwrap();

        // Second run: should overwrite with host content
        let (state2, outcome2) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
            updated_creds
        );
        assert_eq!(outcome2, AuthProvisionOutcome::Synced);
    }

    // ── Mode transition tests ───────────────────────────────────────────

    #[test]
    fn switching_from_sync_to_ignore_revokes_forwarded_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: sync mode writes credentials
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert!(state.claude_credentials_json().unwrap().exists());

        // Operator switches to ignore — credentials must be wiped
        let (state2, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(!state2.claude_credentials_json().unwrap().exists());
    }

    #[test]
    fn token_mode_writes_onboarding_skeleton_and_no_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // Seed host auth — token mode must NOT copy it.
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::OAuthToken,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Skeleton tells Claude CLI to skip the interactive login wizard;
        // actual auth comes from CLAUDE_CODE_OAUTH_TOKEN in the env.
        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            r#"{"hasCompletedOnboarding":true}"#
        );
        assert!(
            !state.claude_credentials_json().unwrap().exists(),
            "token mode must not write .credentials.json"
        );
        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    }

    /// `ApiKey` shares the wipe-state contract with `OAuthToken` (both
    /// env-driven modes) but is dispatched as a distinct enum variant —
    /// pin its filesystem behavior independently so a future per-mode
    /// split can't silently break the `ApiKey` path. The pre-seeded
    /// `.credentials.json` here doubles as a "switching from sync to
    /// `ApiKey` revokes forwarded creds" assertion: the file existed
    /// before the `ApiKey` run and must be gone after.
    #[test]
    fn api_key_mode_wipes_credentials_and_writes_empty_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // Seed host auth — api_key mode must NOT copy it.
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: sync mode writes credentials we'll then need to verify
        // get wiped under api_key.
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert!(
            state.claude_credentials_json().unwrap().exists(),
            "precondition: sync seeded .credentials.json"
        );

        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::ApiKey,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
            "{}",
            "api_key mode must reset .claude.json to empty object"
        );
        assert!(
            !state2.claude_credentials_json().unwrap().exists(),
            "api_key mode must wipe .credentials.json"
        );
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    }

    #[test]
    fn switching_from_sync_to_token_revokes_forwarded_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: sync mode writes credentials
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert!(state.claude_credentials_json().unwrap().exists());

        // Operator switches to token — credentials must be wiped and
        // .claude.json reset to skeleton so Claude Code skips the login
        // wizard and authenticates exclusively via CLAUDE_CODE_OAUTH_TOKEN.
        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::OAuthToken,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
            r#"{"hasCompletedOnboarding":true}"#
        );
        assert!(!state2.claude_credentials_json().unwrap().exists());
        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    }

    #[test]
    fn switching_from_token_to_sync_forwards_fresh_host_creds() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: token mode writes the onboarding skeleton
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::OAuthToken,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            r#"{"hasCompletedOnboarding":true}"#
        );

        // Operator switches to sync — host auth must now be forwarded
        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(state2.claude_account_json().unwrap())
                .unwrap()
                .contains("test@example.com")
        );
        assert_eq!(
            std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
            TEST_CREDENTIALS
        );
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
    }

    #[test]
    fn switching_from_token_to_ignore_remains_empty() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // Token mode seeds an empty state
        let (_, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::OAuthToken,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Switching to ignore must keep the empty shape (no .credentials.json)
        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(!state2.claude_credentials_json().unwrap().exists());
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    }

    #[test]
    fn sync_mode_preserves_container_auth_when_host_file_missing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: host has auth, sync copies it
        seed_host_auth(&temp);
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Host auth disappears (e.g. user logged out)
        std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
        std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

        // Container may have its own auth by now (from manual login inside)
        let container_auth = r#"{"oauthAccount":{"emailAddress":"container@example.com"}}"#;
        std::fs::write(state.claude_account_json().unwrap(), container_auth).unwrap();

        // Second run: host auth missing — container auth must be preserved
        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
            container_auth
        );
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    }

    #[cfg(unix)]
    #[test]
    fn auth_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        let perms = std::fs::metadata(state.claude_account_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "claude.json should have 0600 permissions"
        );
        let creds_perms = std::fs::metadata(state.claude_credentials_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(
            creds_perms.mode() & 0o777,
            0o600,
            ".credentials.json should have 0600 permissions"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_permissions_on_legacy_permissive_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: create the file with ignore mode (gets 0600)
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Simulate a legacy state file with permissive mode
        std::fs::set_permissions(
            state.claude_account_json().unwrap(),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let perms = std::fs::metadata(state.claude_account_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(perms.mode() & 0o777, 0o644, "precondition: file is 0644");

        // Sync with host auth — must tighten permissions
        seed_host_auth(&temp);
        let (state2, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        let perms = std::fs::metadata(state2.claude_account_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "sync should repair permissions on existing file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_permissions_when_host_auth_missing() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: sync with host auth to seed both files
        seed_host_auth(&temp);
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Simulate legacy permissive modes on both auth files
        std::fs::set_permissions(
            state.claude_account_json().unwrap(),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let creds_path = state.claude_credentials_json().unwrap();
        std::fs::set_permissions(creds_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Remove host auth so sync takes the preserve path
        std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
        std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

        // Second run: host auth missing — files preserved but permissions repaired
        let (state2, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);

        let json_perms = std::fs::metadata(state2.claude_account_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(
            json_perms.mode() & 0o777,
            0o600,
            "sync should repair .claude.json permissions even when host auth is missing"
        );
        let creds_perms = std::fs::metadata(state2.claude_credentials_json().unwrap())
            .unwrap()
            .permissions();
        assert_eq!(
            creds_perms.mode() & 0o777,
            0o600,
            "sync should repair .credentials.json permissions even when host auth is missing"
        );
    }

    // ── Symlink traversal protection ────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_claude_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: create the state directory
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Replace .claude.json with a symlink to a decoy file
        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "original").unwrap();
        std::fs::remove_file(state.claude_account_json().unwrap()).unwrap();
        std::os::unix::fs::symlink(&decoy, state.claude_account_json().unwrap()).unwrap();

        // Sync should refuse to write through the symlink
        let err = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink error, got: {err}"
        );

        // Decoy file must be untouched
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "original");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_credentials_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: create the state directory with credentials
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        // Replace .credentials.json with a symlink
        let decoy = temp.path().join("decoy-creds.txt");
        std::fs::write(&decoy, "secret").unwrap();
        let creds_path = state.claude_credentials_json().unwrap();
        std::fs::remove_file(creds_path).unwrap();
        std::os::unix::fs::symlink(&decoy, creds_path).unwrap();

        // Sync should refuse to write through the symlink
        let err = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            &|_| AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink error, got: {err}"
        );

        // Decoy file must be untouched
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
    }
}

#[cfg(test)]
mod codex_auth_tests {
    use crate::config::AuthForwardMode;
    use crate::instance::{AuthProvisionOutcome, RoleState};
    use std::path::Path;
    use tempfile::tempdir;

    /// Stage a fake host home with a populated `~/.codex/auth.json` so
    /// the sync-mode tests below have a real source file to copy from.
    /// Returns the host-home root and the auth.json contents written.
    fn stage_host_auth_json(temp: &tempfile::TempDir, tail: &str) -> (std::path::PathBuf, String) {
        let host_home = temp.path().join("host_home");
        let codex_dir = host_home.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let content = format!(
            "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"{tail}\"}}}}",
        );
        std::fs::write(codex_dir.join("auth.json"), &content).unwrap();
        (host_home, content)
    }

    #[test]
    fn sync_copies_host_auth_json_when_present() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let (host_home, expected) = stage_host_auth_json(&temp, "abc.test");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
    }

    #[test]
    fn sync_returns_host_missing_when_host_lacks_auth_json() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let host_home = temp.path().join("host_home_without_codex_dir");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        assert!(!auth_json.exists(), "no bootstrap file should be created");
    }

    #[test]
    fn sync_preserves_existing_role_auth_json_when_host_file_missing() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        std::fs::write(&auth_json, "{\"in_container_login\":true}").unwrap();
        let host_home = temp.path().join("empty_host_home");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        assert_eq!(
            std::fs::read_to_string(&auth_json).unwrap(),
            "{\"in_container_login\":true}",
            "in-container login state must survive sync-with-no-host"
        );
    }

    #[test]
    fn ignore_deletes_existing_role_auth_json() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();

        let (outcome, _) = RoleState::provision_codex_auth(
            &auth_json,
            AuthForwardMode::Ignore,
            Path::new("/nonexistent"),
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert!(!auth_json.exists());
    }

    /// `OAuthToken` is parser-rejected for Codex (Task 6) so this arm is
    /// unreachable from operator config in production. The test pins the
    /// defensive no-wipe behavior of the `OAuthToken` arm anyway: if a
    /// parser bypass ever lands a Codex+OAuthToken config at this layer,
    /// the existing role-state `auth.json` is preserved (rather than
    /// silently destroyed) so the operator can recover.
    #[test]
    fn token_mode_leaves_role_auth_json_untouched() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        std::fs::write(&auth_json, "{\"existing\":true}").unwrap();
        let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::OAuthToken, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
        assert_eq!(
            std::fs::read_to_string(&auth_json).unwrap(),
            "{\"existing\":true}"
        );
    }

    /// `ApiKey` mode authenticates via `OPENAI_API_KEY`; a leftover
    /// `auth.json` from a prior Sync run would let Codex silently fall
    /// back to forwarded OAuth credentials that the operator has
    /// explicitly chosen to bypass. Pin the wipe contract here so a
    /// future refactor can't quietly downgrade `ApiKey` to no-op.
    #[test]
    fn api_key_mode_wipes_role_auth_json() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();
        // Stage a host auth.json too — api_key mode must NOT copy it,
        // and must NOT leave the stale role-state file in place either.
        let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

        let (outcome, mounted) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
        assert!(
            !auth_json.exists(),
            "api_key mode must wipe role-state auth.json"
        );
        assert!(
            mounted.is_none(),
            "api_key mode must report no auth.json to mount"
        );
    }

    /// Switching from Sync (creds present on host) to `ApiKey` must wipe
    /// the synced `auth.json` so the next container start cannot fall
    /// back to forwarded OAuth credentials. Without this, an operator
    /// who toggles to `ApiKey` to use `OPENAI_API_KEY` would still be
    /// running on stale OAuth state from the previous sync run.
    #[test]
    fn switching_from_sync_to_api_key_wipes_synced_auth_json() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let (host_home, _) = stage_host_auth_json(&temp, "switch.test");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        assert!(auth_json.exists());

        let (outcome, mounted) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home)
                .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
        assert!(!auth_json.exists(), "ApiKey must wipe prior synced creds");
        assert!(mounted.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn synced_auth_json_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let (host_home, _) = stage_host_auth_json(&temp, "perm.test");

        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

        let mode = std::fs::metadata(&auth_json).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "codex auth.json must be 0o600, got {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_auth_json_under_ignore() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");

        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

        let err = RoleState::provision_codex_auth(
            &auth_json,
            AuthForwardMode::Ignore,
            Path::new("/nonexistent"),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "expected symlink rejection, got: {err}"
        );
        // Decoy file must be untouched.
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
    }

    /// Pin symlink rejection across all credential-bearing modes via the
    /// pre-mode-dispatch check at the top of `provision_codex_auth`.
    /// Without this, a compromised role could plant a symlink at the
    /// role-state `auth.json` and have subsequent sync/token-mode
    /// provisioning bind-mount it into the container as-is.
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_auth_json_under_sync_and_token() {
        for mode in [
            AuthForwardMode::Sync,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::ApiKey,
        ] {
            let temp = tempdir().unwrap();
            let auth_json = temp.path().join("auth.json");

            let decoy = temp.path().join("decoy.txt");
            std::fs::write(&decoy, "secret").unwrap();
            std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

            let err = RoleState::provision_codex_auth(&auth_json, mode, Path::new("/nonexistent"))
                .unwrap_err();
            assert!(
                err.to_string().contains("symlink"),
                "mode {mode:?} did not reject symlink: {err}"
            );
            assert_eq!(
                std::fs::read_to_string(&decoy).unwrap(),
                "secret",
                "mode {mode:?} clobbered decoy"
            );
        }
    }

    /// Switching from Sync (creds present on host) to Ignore must wipe
    /// the synced auth.json so the next container start forces a fresh
    /// in-container login. Without this, an operator who toggles to
    /// Ignore to revoke access keeps the prior credentials accessible.
    #[test]
    fn switching_from_sync_to_ignore_wipes_synced_auth_json() {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let (host_home, _) = stage_host_auth_json(&temp, "rev.test");

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        assert!(auth_json.exists());

        let (outcome, _) =
            RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Ignore, &host_home)
                .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert!(!auth_json.exists(), "Ignore must wipe prior synced creds");
    }

    /// An unreadable host `auth.json` (e.g. `chmod 0` after a `sudo
    /// codex login`) used to be silently bucketed as `HostMissing`,
    /// trapping operators in a re-login loop. Verify the EACCES path
    /// now surfaces an explicit error mentioning the host path.
    #[cfg(unix)]
    #[test]
    fn surfaces_unreadable_host_auth_json_as_error() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");
        let host_home = temp.path().join("host_home");
        let host_codex = host_home.join(".codex");
        std::fs::create_dir_all(&host_codex).unwrap();
        let host_auth_json = host_codex.join("auth.json");
        std::fs::write(&host_auth_json, "{\"auth_mode\":\"chatgpt\"}").unwrap();
        // chmod 0 — file exists but is unreadable. Skip if we can't
        // produce an unreadable file (e.g. running as root in CI).
        std::fs::set_permissions(&host_auth_json, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(&host_auth_json).is_ok() {
            // Running as root — chmod 0 doesn't block reads. Skip.
            return;
        }

        let err = RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("auth.json"),
            "error must mention the host path: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("not found"),
            "EACCES must not be reported as not-found: {msg}"
        );
    }
}

#[cfg(test)]
mod github_auth_tests {
    // The `gh auth token` shellout in `read_host_gh_token` is gated on
    // `host_home_is_real(host_home)` — every test in this module passes
    // a temp-dir `host_home` so the shellout is skipped and the real
    // host's `gh` binary cannot leak into hermetic tests. The file
    // fallback is the only path exercised here. See
    // `read_host_gh_token` source for the gate.
    use super::{
        GithubAuthContext, GithubAuthMode, GithubProvisionOutcome, GithubTokenSource,
        HostMissingReason, parse_gh_hosts_yml,
    };
    use crate::instance::{GithubProvisionKind, RoleState};
    use tempfile::tempdir;

    /// Stage a fake host home with a populated `~/.config/gh/hosts.yml`
    /// so the file-fallback path can be exercised hermetically.
    fn stage_host_hosts_yml(temp: &tempfile::TempDir, token: &str) -> std::path::PathBuf {
        let host_home = temp.path().join("host_home");
        let gh_dir = host_home.join(".config/gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        std::fs::write(
            gh_dir.join("hosts.yml"),
            format!(
                "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: alice\n",
            ),
        )
        .unwrap();
        host_home
    }

    fn ctx(mode: GithubAuthMode, token: Option<&str>) -> GithubAuthContext {
        GithubAuthContext {
            mode,
            token: token.map(str::to_string),
        }
    }

    // ── parse_gh_hosts_yml ────────────────────────────────────────────────

    #[test]
    fn parse_hosts_yml_extracts_oauth_token_and_user() {
        let text = "github.com:\n    oauth_token: ghp_xxx\n    user: alice\n";
        let parsed = parse_gh_hosts_yml(text).expect("must parse");
        assert_eq!(parsed.token, "ghp_xxx");
        assert_eq!(parsed.user.as_deref(), Some("alice"));
    }

    #[test]
    fn parse_hosts_yml_handles_quoted_values() {
        let text = "github.com:\n    oauth_token: \"ghp_xxx\"\n    user: \'bob\'\n";
        let parsed = parse_gh_hosts_yml(text).expect("must parse");
        assert_eq!(parsed.token, "ghp_xxx");
        assert_eq!(parsed.user.as_deref(), Some("bob"));
    }

    #[test]
    fn parse_hosts_yml_returns_none_when_github_block_missing() {
        let text = "ghe.acme.com:\n    oauth_token: ghp_acme\n";
        assert!(parse_gh_hosts_yml(text).is_none());
    }

    #[test]
    fn parse_hosts_yml_returns_none_without_oauth_token() {
        let text = "github.com:\n    user: alice\n";
        assert!(parse_gh_hosts_yml(text).is_none());
    }

    #[test]
    fn parse_hosts_yml_ignores_other_hosts() {
        let text = concat!(
            "ghe.acme.com:\n    oauth_token: ghp_acme\n    user: bob\n",
            "github.com:\n    oauth_token: ghp_real\n    user: alice\n",
        );
        let parsed = parse_gh_hosts_yml(text).expect("must parse");
        assert_eq!(parsed.token, "ghp_real");
        assert_eq!(parsed.user.as_deref(), Some("alice"));
    }

    /// Per YAML 1.x spec, a `#` inside a bare scalar is part of the
    /// value (only `#` preceded by whitespace starts a comment). A
    /// real-world `gh` token will never contain `#`, but pinning this
    /// behavior protects against regressions in the YAML parser
    /// dependency.
    #[test]
    fn parse_hosts_yml_preserves_hash_inside_token_value() {
        let text = "github.com:\n    oauth_token: ghp_real#segment\n";
        let parsed = parse_gh_hosts_yml(text).expect("must parse");
        assert_eq!(parsed.token, "ghp_real#segment");
    }

    /// Trailing-whitespace `#` IS a comment per YAML and must be
    /// stripped from the parsed value.
    #[test]
    fn parse_hosts_yml_strips_trailing_whitespace_comment() {
        let text = "github.com:\n    oauth_token: ghp_real # rotated 2026-01\n";
        let parsed = parse_gh_hosts_yml(text).expect("must parse");
        assert_eq!(parsed.token, "ghp_real");
    }

    /// Malformed YAML (e.g. mismatched quotes) must NOT yield a
    /// partial result. The `serde_yaml_ng` parser returns an error;
    /// `parse_gh_hosts_yml` maps that to `None` so callers fall
    /// through to `HostMissing` instead of writing a bogus token.
    #[test]
    fn parse_hosts_yml_rejects_malformed_yaml() {
        let text = "github.com:\n    oauth_token: \'broken\"\n";
        assert!(parse_gh_hosts_yml(text).is_none());
    }

    // ── provision_github_auth ────────────────────────────────────────────

    #[test]
    fn sync_falls_back_to_hosts_yml_file_when_gh_binary_absent() {
        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_filebased");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();

        match &outcome {
            GithubProvisionOutcome::Synced { token, source } => {
                assert_eq!(token, "ghp_filebased");
                assert_eq!(*source, GithubTokenSource::HostsFile);
            }
            other => panic!("expected Synced, got {other:?}"),
        }
        assert_eq!(outcome.token(), Some("ghp_filebased"));
        assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
        let written = std::fs::read_to_string(&hosts_yml).unwrap();
        assert!(written.contains("oauth_token: ghp_filebased"));
        assert!(written.contains("git_protocol: https"));
        assert!(written.contains("user: alice"));
    }

    #[test]
    fn sync_returns_host_missing_when_neither_source_resolves() {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("host_home_with_no_gh_state");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();

        assert_eq!(
            outcome,
            GithubProvisionOutcome::HostMissing {
                reason: HostMissingReason::NoGhAndNoHostsFile
            }
        );
        assert!(outcome.token().is_none());
        assert!(!hosts_yml.exists());
    }

    #[test]
    fn sync_preserves_existing_role_hosts_yml_when_host_lacks_token() {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("empty_host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");
        std::fs::write(&hosts_yml, "github.com:\n    oauth_token: in_container\n").unwrap();

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();

        assert_eq!(outcome.kind(), GithubProvisionKind::HostMissing);
        let preserved = std::fs::read_to_string(&hosts_yml).unwrap();
        assert!(
            preserved.contains("in_container"),
            "in-container login state must survive sync-with-no-host"
        );
    }

    #[test]
    fn token_mode_wipes_role_hosts_yml() {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");
        std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Token, Some("ghp_token")),
            &host_home,
        )
        .unwrap();

        assert_eq!(
            outcome,
            GithubProvisionOutcome::TokenMode {
                token: "ghp_token".to_string()
            }
        );
        assert_eq!(outcome.token(), Some("ghp_token"));
        assert!(
            !hosts_yml.exists(),
            "token mode must wipe role-state hosts.yml"
        );
    }

    #[test]
    fn ignore_mode_wipes_role_hosts_yml() {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");
        std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Ignore, None),
            &host_home,
        )
        .unwrap();

        assert_eq!(outcome, GithubProvisionOutcome::Skipped);
        assert!(outcome.token().is_none());
        assert!(!hosts_yml.exists());
    }

    #[test]
    fn switching_from_sync_to_token_wipes_synced_hosts_yml() {
        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
        assert!(hosts_yml.exists());

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Token, Some("ghp_scoped")),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
        assert!(!hosts_yml.exists());
    }

    #[test]
    fn switching_from_sync_to_ignore_wipes_synced_hosts_yml() {
        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::Synced);

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Ignore, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome, GithubProvisionOutcome::Skipped);
        assert!(!hosts_yml.exists());
    }

    /// Round-trip across all four modes — pins state cleanliness on
    /// every transition so a regression in `wipe_file_if_present`
    /// (e.g. leaving a 0o600 stub) gets caught here.
    #[test]
    fn round_trip_ignore_sync_token_ignore_state_clean() {
        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_round");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Ignore, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
        assert!(!hosts_yml.exists());

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
        assert!(hosts_yml.exists());

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Token, Some("scoped")),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
        assert!(!hosts_yml.exists());

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Ignore, None),
            &host_home,
        )
        .unwrap();
        assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
        assert!(!hosts_yml.exists());
    }

    /// Two consecutive Sync calls with the same host token must not
    /// re-write `hosts.yml` — mtime stable. Mirrors the codex no-churn
    /// guard; a regression that drops the content-equal check would
    /// fire `write_private_file` (atomic rename) on every launch.
    #[test]
    fn sync_skips_write_when_content_unchanged() {
        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_unchanged");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();
        let mtime_first = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1100));
        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();
        let mtime_second = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

        assert_eq!(
            mtime_first, mtime_second,
            "no-op Sync provisioning must not touch hosts.yml mtime"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_hosts_yml_under_sync_and_token_and_ignore() {
        for mode in [
            GithubAuthMode::Sync,
            GithubAuthMode::Token,
            GithubAuthMode::Ignore,
        ] {
            let temp = tempdir().unwrap();
            let host_home = temp.path().join("host_home");
            let hosts_yml = temp.path().join("role-state-hosts.yml");

            let decoy = temp.path().join("decoy.yml");
            std::fs::write(&decoy, "secret").unwrap();
            std::os::unix::fs::symlink(&decoy, &hosts_yml).unwrap();

            let token = matches!(mode, GithubAuthMode::Token).then_some("tok");
            let err = RoleState::provision_github_auth(&hosts_yml, &ctx(mode, token), &host_home)
                .unwrap_err();

            assert!(
                err.to_string().contains("symlink"),
                "mode {mode:?} did not reject symlink: {err}"
            );
            assert_eq!(
                std::fs::read_to_string(&decoy).unwrap(),
                "secret",
                "mode {mode:?} clobbered decoy"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn synced_hosts_yml_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let host_home = stage_host_hosts_yml(&temp, "ghp_perm");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        RoleState::provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home)
            .unwrap();

        let mode = std::fs::metadata(&hosts_yml).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "synced hosts.yml must be 0o600, got {mode:o}");
    }

    /// Sync mode does not consume the operator-supplied `Token`-mode
    /// token. Strengthened from a bare `is_none()` to also assert the
    /// outcome doesn't carry the supplied value via any other path.
    #[test]
    fn sync_does_not_consume_supplied_token() {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("empty_host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let outcome = RoleState::provision_github_auth(
            &hosts_yml,
            &ctx(GithubAuthMode::Sync, Some("operator_supplied")),
            &host_home,
        )
        .unwrap();

        assert_eq!(outcome.kind(), GithubProvisionKind::HostMissing);
        assert!(outcome.token().is_none());
        assert_ne!(outcome.token(), Some("operator_supplied"));
    }

    /// Manual `Debug` impl on `GithubAuthContext` must redact the
    /// token so `tracing::debug!("{ctx:?}")` cannot leak it.
    #[test]
    fn github_auth_context_debug_redacts_token() {
        let ctx = ctx(GithubAuthMode::Token, Some("ghp_secret_value"));
        let s = format!("{ctx:?}");
        assert!(
            !s.contains("ghp_secret_value"),
            "token leaked in Debug: {s}"
        );
        assert!(s.contains("<redacted>"));
    }

    /// Manual `Debug` impl on `GithubProvisionOutcome` must redact the
    /// token in `Synced` and `TokenMode` variants.
    #[test]
    fn github_provision_outcome_debug_redacts_token() {
        let synced = GithubProvisionOutcome::Synced {
            token: "ghp_synced_secret".to_string(),
            source: GithubTokenSource::GhCli,
        };
        let s = format!("{synced:?}");
        assert!(!s.contains("ghp_synced_secret"), "Synced token leaked: {s}");

        let tok = GithubProvisionOutcome::TokenMode {
            token: "ghp_token_secret".to_string(),
        };
        let s = format!("{tok:?}");
        assert!(
            !s.contains("ghp_token_secret"),
            "TokenMode token leaked: {s}"
        );
    }
}

#[cfg(test)]
mod amp_auth_tests {
    use crate::config::AuthForwardMode;
    use crate::instance::{AuthProvisionOutcome, RoleState};
    use std::path::Path;
    use tempfile::tempdir;

    fn stage_host_secrets(temp: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
        let host_home = temp.path().join("host_home");
        let amp_dir = host_home.join(".local/share/amp");
        std::fs::create_dir_all(&amp_dir).unwrap();
        std::fs::write(amp_dir.join("secrets.json"), content).unwrap();
        host_home
    }

    #[test]
    fn sync_copies_host_secrets_json_when_present() {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = stage_host_secrets(
            &temp,
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
        );

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
        assert_eq!(
            std::fs::read_to_string(&secrets_json).unwrap(),
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}"
        );
    }

    #[test]
    fn sync_preserves_existing_secrets_when_host_file_missing() {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
        let host_home = temp.path().join("empty_host_home");

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
        assert_eq!(
            std::fs::read_to_string(&secrets_json).unwrap(),
            "{\"in_container_login\":true}"
        );
    }

    #[test]
    fn sync_with_no_host_and_no_prior_file_skips_mount() {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = temp.path().join("empty_host_home");

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        assert!(mounted.is_none());
        assert!(!secrets_json.exists());
    }

    #[test]
    fn api_key_mode_wipes_role_secrets_json() {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();
        let host_home = stage_host_secrets(
            &temp,
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
        );

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::ApiKey, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
        assert!(mounted.is_none());
        assert!(!secrets_json.exists());
    }

    #[test]
    fn ignore_mode_wipes_role_secrets_json() {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();

        let (outcome, mounted) = RoleState::provision_amp_auth(
            &secrets_json,
            AuthForwardMode::Ignore,
            Path::new("/nonexistent"),
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert!(mounted.is_none());
        assert!(!secrets_json.exists());
    }

    #[test]
    fn oauth_token_defensive_arm_wipes_role_state() {
        // OAuthToken is parser-rejected for Amp; the defensive arm in
        // provision_amp_auth wipes any prior Sync's role-state file so a
        // config bypass cannot leak forwarded credentials into the
        // container. The arm should never run in production, but if it
        // does the bypass must be loud and safe.
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        std::fs::write(&secrets_json, "{\"prior_sync\":true}").unwrap();

        let (outcome, mounted) = RoleState::provision_amp_auth(
            &secrets_json,
            AuthForwardMode::OAuthToken,
            Path::new("/nonexistent"),
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
        assert!(mounted.is_none(), "bypass arm must not produce a mount");
        assert!(
            !secrets_json.exists(),
            "bypass arm must wipe the prior Sync residue"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_secrets_json_under_every_mode() {
        // Loop every mode. The symlink check is hoisted above the mode
        // match, so the defense holds for all four arms today — but a
        // future refactor that pushes the check inside specific arms
        // could silently regress Sync (highest blast radius — it would
        // otherwise read through the symlink).
        for mode in [
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ] {
            let temp = tempdir().unwrap();
            let secrets_json = temp.path().join("secrets.json");
            let decoy = temp.path().join("decoy.txt");
            std::fs::write(&decoy, "secret").unwrap();
            std::os::unix::fs::symlink(&decoy, &secrets_json).unwrap();

            let err = RoleState::provision_amp_auth(&secrets_json, mode, Path::new("/nonexistent"))
                .unwrap_err();

            assert!(
                err.to_string().contains("symlink"),
                "mode={mode:?}: expected symlink rejection, got: {err}"
            );
            assert_eq!(
                std::fs::read_to_string(&decoy).unwrap(),
                "secret",
                "mode={mode:?}: decoy contents must survive"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn surfaces_unreadable_host_secrets_json_as_error() {
        // Sync arm with an unreadable host secrets.json must surface
        // the io::Error rather than misdiagnosing as HostMissing —
        // otherwise the operator gets trapped in a re-login loop they
        // cannot escape until they spot the bad permissions on the
        // host file.
        use std::os::unix::fs::PermissionsExt;
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = temp.path().join("host_home");
        let amp_dir = host_home.join(".local/share/amp");
        std::fs::create_dir_all(&amp_dir).unwrap();
        let host_secrets = amp_dir.join("secrets.json");
        std::fs::write(
            &host_secrets,
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
        )
        .unwrap();
        std::fs::set_permissions(&host_secrets, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home);

        // Restore perms so tempdir cleanup succeeds regardless of test
        // outcome.
        let _ = std::fs::set_permissions(&host_secrets, std::fs::Permissions::from_mode(0o600));

        let err = result.expect_err("EACCES on host secrets.json must surface as an error");
        let rendered = format!("{err:#}");
        assert!(
            rendered.contains("secrets.json"),
            "error must name the host file: {rendered}"
        );
        assert!(
            !rendered.contains("not found")
                && !rendered.to_ascii_lowercase().contains("nonexistent"),
            "EACCES must not be reported as not-found: {rendered}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn synced_secrets_json_has_restricted_permissions() {
        // Bypassing `write_private_file` would land at 0o644 and leak
        // the token. Pin 0o600 explicitly.
        use std::os::unix::fs::PermissionsExt;
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = stage_host_secrets(
            &temp,
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
        );

        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

        let mode = std::fs::metadata(&secrets_json)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "synced secrets.json must be 0o600, got {mode:o}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_permissions_when_host_secrets_missing() {
        // HostMissing is the only path that carries a prior file
        // across launches; the arm must tighten its perms.
        use std::os::unix::fs::PermissionsExt;
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
        std::fs::set_permissions(&secrets_json, std::fs::Permissions::from_mode(0o644)).unwrap();
        let host_home = temp.path().join("empty_host_home");

        let (outcome, _) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        let mode = std::fs::metadata(&secrets_json)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "HostMissing must repair the carry-over file's permissions; got {mode:o}"
        );
    }

    #[test]
    fn sync_ignores_xdg_config_settings_json_decoy() {
        // Catches a regression that swapped the Sync source from
        // XDG_DATA `secrets.json` to XDG_CONFIG `settings.json`.
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = temp.path().join("host_home");

        // Only the XDG_CONFIG decoy exists; the canonical XDG_DATA
        // path is empty.
        let xdg_config = host_home.join(".config/amp");
        std::fs::create_dir_all(&xdg_config).unwrap();
        std::fs::write(xdg_config.join("settings.json"), "WRONG").unwrap();

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(
            outcome,
            AuthProvisionOutcome::HostMissing,
            "decoy at XDG_CONFIG must not produce a Sync"
        );
        assert!(mounted.is_none());
        assert!(
            !secrets_json.exists(),
            "decoy contents must not be copied into the role state"
        );
    }

    #[test]
    fn sync_treats_empty_host_secrets_as_host_missing() {
        // Without this guard, an empty host file would be Synced and
        // the agent would fail to auth with no breadcrumb.
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let host_home = stage_host_secrets(&temp, "   \n\t  \n");

        let (outcome, mounted) =
            RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home)
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        assert!(mounted.is_none());
        assert!(
            !secrets_json.exists(),
            "empty host file must not produce a role-state copy"
        );
    }
}
