// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use jackin_instance_credentials::AuthProvisionOutcome;

use jackin_config::AuthForwardMode;

use std::path::Path;

use crate::wipe_kimi_state;
use jackin_instance_credentials::auth_directory;

#[cfg(not(unix))]
pub fn validate_kimi_source_dir_unixless(source_dir: &Path) -> anyhow::Result<()> {
    let config = source_dir.join("config.toml");
    let credentials = source_dir.join("credentials");
    let config_bytes = read_bounded_local_file(&config)?;
    String::from_utf8(config_bytes).context("Kimi config.toml is not valid UTF-8")?;
    let metadata = std::fs::symlink_metadata(credentials)?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink());
    Ok(())
}

pub fn provision_kimi_auth(
    kimi_dir: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    provision_kimi_dir_credential(
        kimi_dir,
        &host_home.join(".kimi-code"),
        mode,
        KIMI_SYNC_FILES,
        "Kimi dir",
        "Kimi",
        false,
    )
}

pub fn provision_kimi_auth_from_source_dir(
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
        true,
    )
}

pub(crate) fn provision_kimi_dir_credential(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
    sync_files: &[&str],
    _label: &str,
    agent_name: &str,
    validate_source: bool,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
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
            #[cfg(unix)]
            {
                let source = auth_directory::lock_source_dir(host_dir)?;
                if validate_source && let Some(source) = source.as_ref() {
                    validate_kimi_locked_source(source, host_dir)?;
                }
                auth_directory::stage_auth_directory_with_locked_source(
                    target_dir,
                    host_dir,
                    source,
                    |host_dir, source, staged| {
                        for name in sync_files {
                            auth_directory::copy_optional_source_file(
                                source,
                                name,
                                staged,
                                name,
                                &format!("reading {}", host_dir.join(name).display()),
                            )?;
                        }

                        auth_directory::copy_optional_source_tree(
                            source,
                            "credentials",
                            staged,
                            "credentials",
                            &format!("copying {}", host_dir.join("credentials").display()),
                        )?;
                        Ok(())
                    },
                )?
            }
            #[cfg(not(unix))]
            {
                auth_directory::stage_auth_directory(
                    target_dir,
                    host_dir,
                    |host_dir, source, staged| {
                        for name in sync_files {
                            auth_directory::copy_optional_source_file(
                                source,
                                name,
                                staged,
                                name,
                                &format!("reading {}", host_dir.join(name).display()),
                            )?;
                        }

                        auth_directory::copy_optional_source_tree(
                            source,
                            "credentials",
                            staged,
                            "credentials",
                            &format!("copying {}", host_dir.join("credentials").display()),
                        )?;
                        Ok(())
                    },
                )?
            }
        }
    };

    let forward_auth = matches!(
        outcome,
        AuthProvisionOutcome::Synced | AuthProvisionOutcome::HostMissing
    );
    Ok((outcome, forward_auth))
}

#[cfg(unix)]
pub fn validate_kimi_locked_source(
    source: &auth_directory::LockedSource,
    host_dir: &Path,
) -> anyhow::Result<()> {
    let config = auth_directory::read_locked_source_file(
        &source.root,
        &["config.toml"],
        "Kimi config.toml",
    )?;
    let credentials = auth_directory::validate_locked_source_directory(
        &source.root,
        &["credentials"],
        "Kimi credentials",
    )?;
    anyhow::ensure!(
        config.is_some() && credentials,
        "Kimi source {} must contain config.toml and a credentials/ directory",
        host_dir.display()
    );
    Ok(())
}

pub(crate) const KIMI_SYNC_FILES: &[&str] = &["config.toml", "device_id"];
