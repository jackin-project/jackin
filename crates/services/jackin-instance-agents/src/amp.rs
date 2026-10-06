// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Amp credential provisioning and source helpers.

use jackin_instance_credentials::AuthProvisionOutcome;

use anyhow::Context;
use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::{provision_single_file_credential, provision_single_file_credential_with_content};
use jackin_instance_credentials::auth_directory;

#[cfg(not(unix))]
pub fn amp_credentials_dir(source: &Path) -> PathBuf {
    let nested = source.join("data/amp");
    if nested.is_dir() {
        nested
    } else {
        source.to_path_buf()
    }
}

#[cfg(unix)]
pub fn lock_amp_source_dir(source: &Path) -> anyhow::Result<Option<auth_directory::LockedSource>> {
    match auth_directory::lock_source_dir(&source.join("data/amp"))? {
        Some(source) => Ok(Some(source)),
        None => auth_directory::lock_source_dir(source),
    }
}

/// Require a non-empty credential file named `name` directly inside `dir`.
#[cfg(not(unix))]
pub fn require_credential_file(
    dir: &Path,
    name: &str,
    agent: &str,
) -> Result<(), SyncSourceValidationError> {
    match read_source_text(&dir.join(name), &format!("{agent} {name}")) {
        Ok(content) if !content.trim().is_empty() => Ok(()),
        Ok(_) => Err(SyncSourceValidationError::new(format!(
            "{agent} credential {name} in {} is empty.",
            dir.display()
        ))),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            }) =>
        {
            Err(SyncSourceValidationError::new(format!(
                "Not a {agent} config folder: expected {name} directly inside {}.",
                dir.display()
            )))
        }
        Err(error) => Err(SyncSourceValidationError::new(format!(
            "{agent} source rejected: {error:#}"
        ))),
    }
}

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
pub fn provision_amp_auth(
    secrets_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_amp_auth_from_path(
        secrets_json,
        mode,
        &host_home.join(".local/share/amp/secrets.json"),
    )
}

pub fn provision_amp_auth_from_source_dir(
    secrets_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    #[cfg(unix)]
    if mode == AuthForwardMode::Sync {
        let content = match lock_amp_source_dir(source_dir)? {
            Some(source) => auth_directory::read_locked_source_file(
                &source.root,
                &["secrets.json"],
                "Amp secrets.json",
            )?
            .map(|bytes| String::from_utf8(bytes).context("Amp secrets.json is not valid UTF-8"))
            .transpose()?,
            None => None,
        };
        return provision_single_file_credential_with_content(
            secrets_json,
            mode,
            content,
            "Amp secrets.json",
            "Amp",
            true,
            true,
            true,
        );
    }

    #[cfg(unix)]
    let host_secrets_json = source_dir.join("secrets.json");
    #[cfg(not(unix))]
    let host_secrets_json = amp_credentials_dir(source_dir).join("secrets.json");
    provision_amp_auth_from_path(secrets_json, mode, &host_secrets_json)
}

fn provision_amp_auth_from_path(
    secrets_json: &Path,
    mode: AuthForwardMode,
    host_secrets_json: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
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
