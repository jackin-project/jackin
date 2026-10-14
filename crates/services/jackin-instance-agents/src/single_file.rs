// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use jackin_instance_credentials::AuthProvisionOutcome;

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::{private_file_exists, read_source_bytes, read_source_text, wipe_file_if_present};
use jackin_instance_credentials::{
    auth_directory, reject_auth_path, repair_permissions, write_private_bytes, write_private_file,
};
use zeroize::Zeroizing;

pub fn provision_single_blob_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    let content = if mode == AuthForwardMode::Sync {
        read_source_bytes(host_path, &format!("{agent_name} {label}"))?.map(Zeroizing::new)
    } else {
        None
    };
    provision_single_blob_credential_from_content(target, mode, content, label, agent_name)
}

pub fn provision_single_blob_credential_from_content(
    target: &Path,
    mode: AuthForwardMode,
    content: Option<Zeroizing<Vec<u8>>>,
    label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    use anyhow::Context;

    reject_auth_path(target)?;

    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            eprintln!(
                "[jackin] internal: {agent_name} provision received unsupported \
                 OAuthToken mode — parser invariant bypassed; \
                 wiping role state and falling back to token-mode."
            );
            wipe_agent_file_state(target, label)?;
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
        AuthForwardMode::Sync => match content {
            Some(content) if content.is_empty() => {
                eprintln!(
                    "[jackin] host {agent_name} credential is empty — treating as host-missing"
                );
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
            Some(content) => {
                write_private_bytes(target, content.as_slice()).with_context(|| {
                    format!(
                        "failed to write {agent_name} role-state {label} at {}",
                        target.display()
                    )
                })?;
                AuthProvisionOutcome::Synced
            }
            None => {
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
        },
    };

    let mounted = match outcome {
        AuthProvisionOutcome::Synced => Some(target.to_path_buf()),
        AuthProvisionOutcome::Skipped => None,
        AuthProvisionOutcome::HostMissing | AuthProvisionOutcome::TokenMode => {
            private_file_exists(target)?.then(|| target.to_path_buf())
        }
    };
    Ok((outcome, mounted))
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn provision_single_file_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
    treat_empty_as_missing: bool,
    warn_on_oauth: bool,
    wipe_on_oauth: bool,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    let content = if mode == AuthForwardMode::Sync {
        read_source_text(host_path, label)?
    } else {
        None
    };
    provision_single_file_credential_with_content(
        target,
        mode,
        content,
        label,
        agent_name,
        treat_empty_as_missing,
        warn_on_oauth,
        wipe_on_oauth,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "content and mode policy are deliberately explicit at this security boundary"
)]
pub fn provision_single_file_credential_with_content(
    target: &Path,
    mode: AuthForwardMode,
    content: Option<String>,
    label: &str,
    agent_name: &str,
    treat_empty_as_missing: bool,
    warn_on_oauth: bool,
    wipe_on_oauth: bool,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    use anyhow::Context;

    reject_auth_path(target)?;

    let mut retain_existing_on_missing = true;
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
        AuthForwardMode::Sync => match content {
            Some(content) if treat_empty_as_missing && content.trim().is_empty() => {
                eprintln!("[jackin] host {label} is empty/whitespace — treating as host-missing");
                retain_existing_on_missing = false;
                wipe_agent_file_state(target, label)?;
                AuthProvisionOutcome::HostMissing
            }
            Some(content) => {
                write_private_file(target, &content).with_context(|| {
                    format!(
                        "failed to write {agent_name} role-state {label} at {}",
                        target.display()
                    )
                })?;
                AuthProvisionOutcome::Synced
            }
            None => {
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
        },
    };

    let mounted = match outcome {
        AuthProvisionOutcome::Synced => Some(target.to_path_buf()),
        AuthProvisionOutcome::Skipped => None,
        AuthProvisionOutcome::HostMissing => {
            if retain_existing_on_missing && private_file_exists(target)? {
                Some(target.to_path_buf())
            } else {
                None
            }
        }
        AuthProvisionOutcome::TokenMode => {
            private_file_exists(target)?.then(|| target.to_path_buf())
        }
    };
    Ok((outcome, mounted))
}

pub fn wipe_agent_file_state(path: &Path, label: &str) -> anyhow::Result<()> {
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

pub fn wipe_kimi_state(kimi_dir: &Path) -> anyhow::Result<()> {
    auth_directory::wipe_auth_directory(kimi_dir).map_err(|error| {
        anyhow::anyhow!(format!(
            "failed to wipe stale Kimi state at {}: {error:#} \
                 (auth_forward switched to ignore/api_key); remove the directory \
                 manually if it has unexpected ownership",
            kimi_dir.display()
        ))
    })?;
    Ok(())
}
