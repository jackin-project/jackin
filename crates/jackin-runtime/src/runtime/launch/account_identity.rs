// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account admission identity for a container's immutable environment.
#![expect(
    clippy::disallowed_methods,
    reason = "credential directory durability runs inside the launch blocking task"
)]

use jackin_config::AppConfig;
use jackin_core::WorkspaceName;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const ACCOUNT_FINGERPRINT_FILE: &str = "account-config.sha256";

static CREDENTIAL_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialWriteFailure {
    StagedFile(usize),
    PreviousRename,
    Install,
}

#[cfg(test)]
thread_local! {
    static CREDENTIAL_WRITE_FAILURE: std::cell::Cell<Option<CredentialWriteFailure>> =
        const { std::cell::Cell::new(None) };
}

fn maybe_inject_credential_write_failure(point: CredentialWriteFailure) -> anyhow::Result<()> {
    #[cfg(test)]
    if CREDENTIAL_WRITE_FAILURE.with(|failure| failure.get() == Some(point)) {
        anyhow::bail!("injected credential publication failure at {point:?}");
    }

    #[cfg(not(test))]
    let _ = point;
    Ok(())
}

#[cfg(test)]
struct CredentialWriteFailureGuard {
    previous: Option<CredentialWriteFailure>,
}

#[cfg(test)]
fn inject_credential_write_failure(point: CredentialWriteFailure) -> CredentialWriteFailureGuard {
    let previous = CREDENTIAL_WRITE_FAILURE.with(|failure| failure.replace(Some(point)));
    assert!(
        previous.is_none(),
        "nested credential-write failure injection is not supported"
    );
    CredentialWriteFailureGuard { previous }
}

#[cfg(test)]
impl Drop for CredentialWriteFailureGuard {
    fn drop(&mut self) {
        CREDENTIAL_WRITE_FAILURE.with(|failure| failure.set(self.previous));
    }
}

fn unique_credential_sibling(root: &Path, prefix: &str) -> anyhow::Result<std::path::PathBuf> {
    for _ in 0..128 {
        let sequence = CREDENTIAL_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(".{prefix}-{}-{sequence}", std::process::id()));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not allocate a unique private credential staging path")
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

fn validate_existing_credentials_directory(directory: &Path) -> anyhow::Result<bool> {
    let metadata = match std::fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_dir(),
        "private credentials path is not a directory: {}",
        directory.display()
    );
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        anyhow::ensure!(
            entry.file_type()?.is_file(),
            "unexpected non-file entry in private credentials directory: {}",
            entry.path().display()
        );
    }
    Ok(true)
}

fn rollback_credential_swap(
    directory: &Path,
    previous: Option<&Path>,
    installed: bool,
    cause: anyhow::Error,
) -> anyhow::Result<()> {
    let mut rollback_error = None;
    if installed && let Err(error) = std::fs::remove_dir_all(directory) {
        rollback_error = Some(anyhow::anyhow!(error));
    }
    if let Some(previous) = previous
        && rollback_error.is_none()
        && let Err(error) = std::fs::rename(previous, directory)
    {
        rollback_error = Some(anyhow::anyhow!(error));
    }
    if let Some(rollback_error) = rollback_error {
        return Err(cause.context(format!(
            "credential publication failed and rollback failed: {rollback_error:#}"
        )));
    }
    Err(cause)
}

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
        // v4 extends admission to the instance set: agent configurations and
        // launch defaults select which instances resolve. v2 keeps Claude
        // metadata inside its directory mount; v1 containers pin a mutable
        // .claude.json inode and cannot support atomic replacement.
        "account-config-v4-isolated-sessions",
        accounts,
        &config.account_bindings,
        ws.map(|ws| &ws.account_bindings),
        ws.and_then(|ws| ws.roles.get(role))
            .map(|role| &role.account_bindings),
        &config.agent_configurations,
        &config.default_launch,
        ws.map(|ws| &ws.default_launch),
        ws.and_then(|ws| ws.roles.get(role))
            .map(|role| &role.default_launch),
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
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::create_dir_all(root)?;
    let directory = root.join("credentials");
    let had_previous = validate_existing_credentials_directory(&directory)?;
    let payloads = credentials
        .iter()
        .map(|(instance, credential)| {
            let staged = jackin_protocol::StagedInstanceCredential {
                schema_version: 1,
                instance: instance.clone(),
                credential: credential.clone(),
            };
            Ok::<_, anyhow::Error>((
                jackin_protocol::account_credentials_filename(instance),
                serde_json::to_vec(&staged)?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    // The caller holds the role/container lock while this runs. Build a fully
    // private replacement beside the live directory, then swap directory
    // names. Readers therefore observe either the old complete set or the new
    // complete set; they never observe delete-then-write intermediates.
    let staged_directory = tempfile::Builder::new()
        .prefix(".credentials-stage-")
        .tempdir_in(root)?;
    std::fs::set_permissions(
        staged_directory.path(),
        std::fs::Permissions::from_mode(0o700),
    )?;
    for (index, (instance, bytes)) in payloads.iter().enumerate() {
        let mut file = tempfile::NamedTempFile::new_in(staged_directory.path())?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        file.persist(staged_directory.path().join(instance))
            .map_err(|error| error.error)?;
        maybe_inject_credential_write_failure(CredentialWriteFailure::StagedFile(index))?;
    }
    sync_directory(staged_directory.path())?;

    let previous_directory = if had_previous {
        Some(unique_credential_sibling(root, "credentials-previous")?)
    } else {
        None
    };
    if let Some(previous_directory) = previous_directory.as_deref() {
        std::fs::rename(&directory, previous_directory)?;
    }
    if let Err(error) =
        maybe_inject_credential_write_failure(CredentialWriteFailure::PreviousRename)
    {
        return rollback_credential_swap(&directory, previous_directory.as_deref(), false, error);
    }

    if let Err(error) = std::fs::rename(staged_directory.path(), &directory) {
        return rollback_credential_swap(
            &directory,
            previous_directory.as_deref(),
            false,
            error.into(),
        );
    }
    if let Err(error) = maybe_inject_credential_write_failure(CredentialWriteFailure::Install) {
        return rollback_credential_swap(&directory, previous_directory.as_deref(), true, error);
    }
    if let Err(error) = sync_directory(root) {
        return rollback_credential_swap(
            &directory,
            previous_directory.as_deref(),
            true,
            error.into(),
        );
    }
    if let Some(previous_directory) = previous_directory {
        std::fs::remove_dir_all(previous_directory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
