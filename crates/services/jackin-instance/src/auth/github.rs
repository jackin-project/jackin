// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! GitHub auth provisioning plus host gh token/hosts.yml reading.

use crate::InstanceError;
use crate::{
    GithubAuthContext, GithubProvisionOutcome, GithubTokenSource, HostMissingReason, RoleState,
};
use anyhow::Context;
use jackin_config::GithubAuthMode;

use std::path::Path;

use crate::auth::{
    auth_directory, read_bounded_local_file, reject_auth_path, repair_permissions,
    write_private_file,
};

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
    pub(crate) fn provision_github_auth(
        hosts_yml: &Path,
        github: &GithubAuthContext,
        host_home: &Path,
    ) -> anyhow::Result<GithubProvisionOutcome> {
        // Reject pre-existing symlinks before branching on mode. The
        // role-state dir is bind-mounted RW, so a compromised role could
        // plant a symlink between launches; calling reject_auth_path
        // unconditionally is fine — it lstat's and no-ops on ENOENT.
        reject_auth_path(hosts_yml)?;

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
                        let needs_write = !read_bounded_local_file(hosts_yml)
                            .is_ok_and(|existing| existing == content.as_bytes());
                        if needs_write {
                            write_private_file(hosts_yml, &content)?;
                        } else {
                            repair_permissions(hosts_yml)?;
                        }
                        Ok(GithubProvisionOutcome::Synced {
                            token: resolved.token,
                            source: resolved.source,
                        })
                    }
                    HostGhResolution::Missing(reason) => {
                        repair_permissions(hosts_yml)?;
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
pub(crate) fn render_hosts_yml(token: &str, user: Option<&str>) -> String {
    let user_field = user.filter(|s| !s.trim().is_empty()).unwrap_or("git");
    format!(
        "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: {user_field}\n",
    )
}

/// Resolved host-side `gh` auth + which source produced it, so the
/// caller can attribute the value in the launch summary.
pub(crate) struct HostGhAuth {
    pub(crate) token: String,
    pub(crate) user: Option<String>,
    pub(crate) source: GithubTokenSource,
}

/// Result of the host-side resolver. `Missing` carries the typed
/// reason so the launch-summary line can render the actual cause
/// instead of guessing "host logged out".
pub(crate) enum HostGhResolution {
    Resolved(HostGhAuth),
    Missing(HostMissingReason),
}

/// Wipe a file if it exists, ignoring `NotFound` so the call is
/// idempotent without a pre-stat that races with the unlink.
pub(crate) fn wipe_file_if_present(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::remove_file(path)
    }
    #[cfg(not(unix))]
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// True when `host_home` is the operator's real home directory. Gates
/// the host-binary shellouts so hermetic tests with a temp-dir
/// `host_home` cannot leak to the real `gh` binary.
pub(crate) fn host_home_is_real(host_home: &Path) -> bool {
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
pub(crate) fn read_host_gh_token(host_home: &Path) -> anyhow::Result<HostGhResolution> {
    // Read hosts.yml once up front so both the CLI-success path (which
    // reads it for the `user` field) and the file-fallback path share
    // one IO.
    let hosts_path = host_home.join(".config/gh/hosts.yml");
    let hosts_yml = match read_bounded_local_file(&hosts_path) {
        Ok(bytes) => Some(String::from_utf8(bytes).context("GitHub hosts.yml is not valid UTF-8")?),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            }) =>
        {
            None
        }
        Err(error) => {
            let source = error
                .chain()
                .find_map(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .map(|error| std::io::Error::new(error.kind(), error.to_string()))
                })
                .unwrap_or_else(|| std::io::Error::other(error.to_string()));
            return Err(InstanceError::HostConfigRead {
                path: hosts_path,
                source,
            }
            .into());
        }
    };

    let mut cli_failure: Option<HostMissingReason> = None;

    if host_home_is_real(host_home) {
        match crate::process_telemetry::exec_sync(&jackin_process::ExecRequest::new(
            "gh",
            ["auth", "token", "--hostname", "github.com"],
        )) {
            Ok(output) if output.success => {
                let token = String::from_utf8(output.stdout)
                    .context("gh authentication command returned invalid UTF-8")?
                    .trim()
                    .to_owned();
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
            }
            Ok(_) => {
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: "gh authentication command failed".to_owned(),
                });
            }
            Err(_) => {
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: "gh authentication command could not start".to_owned(),
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
pub(crate) fn parse_gh_hosts_yml(text: &str) -> Option<HostGhAuth> {
    #[derive(serde::Deserialize)]
    pub(crate) struct HostsFile {
        // `gh` writes the host header literally as `github.com:`, so
        // the top-level map key is `github.com`.
        #[serde(default, rename = "github.com")]
        github_com: Option<HostEntry>,
    }
    #[derive(serde::Deserialize)]
    pub(crate) struct HostEntry {
        #[serde(default)]
        oauth_token: Option<String>,
        #[serde(default)]
        user: Option<String>,
    }

    let parsed: HostsFile = match serde_yaml_ng::from_str(text) {
        Ok(p) => p,
        Err(_) => return None,
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
