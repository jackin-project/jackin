// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account admission identity for a container's immutable environment.
use jackin_config::AppConfig;
use jackin_core::WorkspaceName;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::path::Path;

const ACCOUNT_FINGERPRINT_FILE: &str = "account-config.sha256";

/// Hash account admission, selected bindings, and credential declarations.
/// Values are hashed in memory; only the digest is stored with an instance.
///
/// # Errors
/// Rejects unknown workspaces and account references or serialization failures.
pub fn account_configuration_fingerprint(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<String> {
    let ws = workspace
        .map(|name| {
            config
                .workspaces
                .get(name.as_str())
                .ok_or_else(|| anyhow::anyhow!("workspace {name} is not configured"))
        })
        .transpose()?;
    let ids = ws.map_or_else(
        || {
            config
                .accounts
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        },
        |ws| ws.accounts.iter().cloned().collect(),
    );
    let accounts = ids
        .into_iter()
        .map(|id| {
            let account = config
                .accounts
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("unknown account {id:?}"))?;
            Ok((id, account))
        })
        .collect::<anyhow::Result<std::collections::BTreeMap<_, _>>>()?;
    let bytes = serde_json::to_vec(&(
        // v2 keeps Claude metadata inside its directory mount; v1 containers
        // pin a mutable .claude.json inode and cannot support atomic replacement.
        "account-config-v2",
        accounts,
        &config.account_bindings,
        ws.map(|ws| &ws.account_bindings),
        ws.and_then(|ws| ws.roles.get(role))
            .map(|role| &role.account_bindings),
    ))?;
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut encoded, "{byte:02x}")?;
    }
    Ok(encoded)
}

/// Whether an existing instance was provisioned under current account admission.
/// An absent identity belongs to an unverified pre-account container.
///
/// # Errors
/// Propagates configuration and non-absence filesystem errors.
pub fn account_configuration_matches(
    root: &Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<bool> {
    let stored = match std::fs::read_to_string(root.join(ACCOUNT_FINGERPRINT_FILE)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(stored == account_configuration_fingerprint(config, workspace, role)?)
}

pub(super) fn record_account_configuration(
    root: &Path,
    paths: &jackin_core::JackinPaths,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<()> {
    std::fs::write(
        root.join(ACCOUNT_FINGERPRINT_FILE),
        account_configuration_fingerprint(config, workspace, role)?,
    )?;
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)?;
    anyhow::ensure!(
        snapshot.diagnostics.is_empty(),
        "cannot record account policy from invalid persisted configuration"
    );
    std::fs::write(
        root.join("account-admission.sha256"),
        account_configuration_fingerprint(&snapshot.config, workspace, role)?,
    )?;
    Ok(())
}

/// Whether persisted account policy still permits reconnecting an instance.
/// The baseline precedes any ephemeral per-launch selection.
///
/// # Errors
/// Propagates configuration and non-absence filesystem errors.
pub fn account_admission_matches(
    root: &Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<bool> {
    let stored = match std::fs::read_to_string(root.join("account-admission.sha256")) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(stored == account_configuration_fingerprint(config, workspace, role)?)
}

pub(super) fn admit_restore(
    resolution: super::RestoreResolution,
    root: &Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<super::RestoreResolution> {
    let container = match &resolution {
        super::RestoreResolution::StartFresh
        | super::RestoreResolution::PurgeAndRestartFresh(_) => return Ok(resolution),
        super::RestoreResolution::StartCurrentRole(name)
        | super::RestoreResolution::RecreateCurrentRole(name)
        | super::RestoreResolution::RestoreCurrentRole(name)
        | super::RestoreResolution::RecoverRelatedRole(name) => name,
        super::RestoreResolution::RebuildRelatedRole(manifest) => &manifest.container_base,
    };
    if account_configuration_matches(&root.join(container), config, workspace, role)? {
        Ok(resolution)
    } else {
        Ok(super::RestoreResolution::StartFresh)
    }
}

pub(super) fn write_account_credentials(
    root: &Path,
    credentials: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    let directory = root.join("credentials");
    std::fs::create_dir_all(&directory)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec(&credentials)?)?;
    file.as_file().sync_all()?;
    file.persist(directory.join("account-credentials.json"))?;
    Ok(())
}
