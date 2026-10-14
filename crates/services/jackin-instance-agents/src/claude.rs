// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance_credentials::AuthProvisionOutcome;

use anyhow::Context;
use jackin_config::AuthForwardMode;

use std::path::Path;

#[cfg(target_os = "macos")]
use crate::host_home_is_real;
use crate::{read_source_text, wipe_file_if_present};
use jackin_instance_credentials::{auth_directory, repair_permissions, write_private_file};

pub fn provision_claude_auth(
    account_json: &Path,
    credentials_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    let host_claude_json = host_home.join(".claude.json");

    let outcome = match mode {
        AuthForwardMode::Ignore => {
            wipe_claude_state(account_json, credentials_json)?;
            AuthProvisionOutcome::Skipped
        }

        AuthForwardMode::ApiKey => {
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
            if let Some(creds) = read_host_credentials(host_home)? {
                copy_host_claude_json(&host_claude_json, account_json)?;
                write_private_file(credentials_json, &creds)?;
                AuthProvisionOutcome::Synced
            } else {
                if !account_json.exists() {
                    write_private_file(account_json, "{}")?;
                }

                repair_permissions(account_json)?;
                repair_permissions(credentials_json)?;
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

pub fn provision_claude_auth_from_config_dir(
    account_json: &Path,
    credentials_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    let outcome = match mode {
        AuthForwardMode::Ignore | AuthForwardMode::ApiKey => {
            wipe_claude_state(account_json, credentials_json)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::OAuthToken => {
            wipe_file_if_present(credentials_json)?;
            write_private_file(account_json, r#"{"hasCompletedOnboarding":true}"#)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Sync => {
            #[cfg(unix)]
            let source_data = {
                let source = auth_directory::lock_source_dir(source_dir)?;
                let creds = match source.as_ref() {
                    Some(source) => locked_claude_credentials(source, source_dir, host_home)?,
                    None => None,
                };
                let account = match source.as_ref() {
                    Some(source) => auth_directory::read_locked_source_file(
                        &source.root,
                        &[".claude.json"],
                        "Claude account metadata",
                    )?
                    .map(|bytes| {
                        String::from_utf8(bytes)
                            .context("Claude account metadata is not valid UTF-8")
                    })
                    .transpose()?
                    .unwrap_or_else(|| "{}".to_owned()),
                    None => "{}".to_owned(),
                };
                (creds, account)
            };
            #[cfg(not(unix))]
            let source_data = (
                read_host_credentials_from_claude_config_dir(source_dir, host_home)?,
                read_source_text(&source_dir.join(".claude.json"), "Claude account metadata")?
                    .unwrap_or_else(|| "{}".to_owned()),
            );

            if let (Some(creds), account) = source_data {
                write_private_file(account_json, &account)?;
                write_private_file(credentials_json, &creds)?;
                AuthProvisionOutcome::Synced
            } else {
                anyhow::bail!(
                    "Not a Claude config folder: {} has no .credentials.json and no matching \
                         macOS Keychain login. Select the folder you set as CLAUDE_CONFIG_DIR when \
                         you logged in to Claude.",
                    source_dir.display()
                );
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

pub fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = read_source_text(host_path, "Claude account metadata")
        .with_context(|| format!("reading Claude account metadata at {}", host_path.display()))?
        .unwrap_or_else(|| "{}".to_owned());
    write_private_file(dest_path, &content)
}

pub(crate) fn wipe_claude_state(
    account_json: &Path,
    credentials_json: &Path,
) -> anyhow::Result<()> {
    write_private_file(account_json, "{}")?;
    wipe_file_if_present(credentials_json)?;
    Ok(())
}

pub(crate) fn read_host_credentials(host_home: &Path) -> anyhow::Result<Option<String>> {
    let creds_path = host_home.join(".claude/.credentials.json");
    if let Some(content) = read_source_text(&creds_path, "Claude credentials")?
        .filter(|content| !content.trim().is_empty())
    {
        return Ok(Some(content));
    }

    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        return read_claude_keychain(jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
    }

    Ok(None)
}

#[cfg(unix)]
pub fn locked_claude_credentials(
    source: &auth_directory::LockedSource,
    source_dir: &Path,
    host_home: &Path,
) -> anyhow::Result<Option<String>> {
    let credentials = auth_directory::read_locked_source_file(
        &source.root,
        &[".credentials.json"],
        "Claude credentials",
    )?;
    if let Some(credentials) = credentials {
        let credentials = String::from_utf8(credentials)
            .context("Claude .credentials.json is not valid UTF-8")?;
        if !credentials.trim().is_empty() {
            return Ok(Some(credentials));
        }
    }

    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        let scope = jackin_core::claude_keychain_scope(source_dir, host_home, source_dir)
            .ok_or_else(|| anyhow::anyhow!("invalid Claude config directory"))?;
        return read_claude_keychain(&scope.service);
    }

    let _ = (source_dir, host_home);
    Ok(None)
}

#[cfg(not(unix))]
pub fn read_host_credentials_from_claude_config_dir(
    source_dir: &Path,
    host_home: &Path,
) -> anyhow::Result<Option<String>> {
    let creds_path = source_dir.join(".credentials.json");
    if let Some(content) = read_source_text(&creds_path, "Claude credentials")?
        .filter(|content| !content.trim().is_empty())
    {
        return Ok(Some(content));
    }

    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        let scope = jackin_core::claude_keychain_scope(source_dir, host_home, source_dir)
            .ok_or_else(|| anyhow::anyhow!("invalid Claude config directory"))?;
        return read_claude_keychain(&scope.service);
    }

    #[cfg(not(target_os = "macos"))]
    let _ = host_home;
    Ok(None)
}

#[cfg(target_os = "macos")]
pub fn read_claude_keychain(service: &str) -> anyhow::Result<Option<String>> {
    let Ok(output) = crate::process_telemetry::exec_sync(&jackin_process::ExecRequest::new(
        "security",
        ["find-generic-password", "-s", service, "-w"],
    )) else {
        return Ok(None);
    };
    if output.success {
        let creds = String::from_utf8(output.stdout)
            .context("Claude Keychain credential is not valid UTF-8")?
            .trim()
            .to_owned();
        if !creds.is_empty() {
            return Ok(Some(creds));
        }
    }
    Ok(None)
}
