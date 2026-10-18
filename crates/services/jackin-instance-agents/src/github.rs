// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use jackin_config::GithubAuthMode;
use jackin_instance_credentials::InstanceError;
use jackin_instance_credentials::{
    GithubAuthContext, GithubProvisionOutcome, GithubTokenSource, HostMissingReason,
};

use std::path::Path;

use jackin_instance_credentials::{
    auth_directory, read_bounded_local_file, reject_auth_path, repair_permissions,
    write_private_file,
};

pub fn provision_github_auth(
    hosts_yml: &Path,
    github: &GithubAuthContext,
    host_home: &Path,
) -> anyhow::Result<GithubProvisionOutcome> {
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

pub(crate) fn render_hosts_yml(token: &str, user: Option<&str>) -> String {
    let user_field = user.filter(|s| !s.trim().is_empty()).unwrap_or("git");
    format!(
        "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: {user_field}\n",
    )
}

#[derive(Debug)]
pub struct HostGhAuth {
    pub token: String,
    pub user: Option<String>,
    pub source: GithubTokenSource,
}

pub(crate) enum HostGhResolution {
    Resolved(HostGhAuth),
    Missing(HostMissingReason),
}

pub fn wipe_file_if_present(path: &Path) -> anyhow::Result<()> {
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

pub fn host_home_is_real(host_home: &Path) -> bool {
    let real_home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    real_home.as_deref() == Some(host_home)
}

pub(crate) fn read_host_gh_token(host_home: &Path) -> anyhow::Result<HostGhResolution> {
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
    Ok(HostGhResolution::Missing(
        cli_failure.unwrap_or(HostMissingReason::HostsFileMalformed),
    ))
}

pub fn parse_gh_hosts_yml(text: &str) -> Option<HostGhAuth> {
    #[derive(serde::Deserialize)]
    struct HostsFile {
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

    let parsed: HostsFile = match serde_saphyr::from_str(text) {
        Ok(p) => p,
        Err(_) => return None,
    };
    let entry = parsed.github_com?;
    let token = entry.oauth_token.filter(|s| !s.trim().is_empty())?;
    Some(HostGhAuth {
        token,
        user: entry.user.filter(|s| !s.trim().is_empty()),
        source: GithubTokenSource::HostsFile,
    })
}
