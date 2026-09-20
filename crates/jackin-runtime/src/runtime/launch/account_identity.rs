// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account admission identity for a container's immutable environment.
#![expect(
    clippy::disallowed_methods,
    reason = "credential directory durability runs inside the launch blocking task"
)]

use crate::instance::{AdmittedInstance, InstanceManifest};
use jackin_config::{AppConfig, ConfigGeneration, ConfigReadGuard, ReadOnlyConfigSnapshot};
use jackin_core::WorkspaceName;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const ACCOUNT_FINGERPRINT_FILE: &str = "account-config.sha256";

/// Immutable persisted-config revision held from launch account resolution
/// through credential publication and admission recording.
pub(super) struct AccountConfigRevision {
    generation: ConfigGeneration,
    _read_guard: ConfigReadGuard,
}

impl AccountConfigRevision {
    pub(super) fn acquire(paths: &jackin_core::JackinPaths) -> anyhow::Result<Self> {
        let before = verified_config_snapshot(paths)?;
        let read_guard = jackin_config::acquire_config_read_lock(&paths.config_file)?;
        let after = verified_config_snapshot(paths)?;
        anyhow::ensure!(
            before.generation == after.generation,
            "configuration changed while starting launch; retry"
        );
        Ok(Self {
            generation: after.generation,
            _read_guard: read_guard,
        })
    }

    fn current_snapshot(
        &self,
        paths: &jackin_core::JackinPaths,
    ) -> anyhow::Result<ReadOnlyConfigSnapshot> {
        let snapshot = verified_config_snapshot(paths)?;
        anyhow::ensure!(
            snapshot.generation == self.generation,
            "configuration changed during launch; aborting credential publication"
        );
        Ok(snapshot)
    }

    pub(super) fn ensure_current(&self, paths: &jackin_core::JackinPaths) -> anyhow::Result<()> {
        self.current_snapshot(paths).map(drop)
    }
}

fn verified_config_snapshot(
    paths: &jackin_core::JackinPaths,
) -> anyhow::Result<ReadOnlyConfigSnapshot> {
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)?;
    anyhow::ensure!(
        snapshot.diagnostics.is_empty(),
        "cannot authorize launch from invalid persisted configuration"
    );
    Ok(snapshot)
}

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

fn configured_workspace<'a>(
    config: &'a AppConfig,
    workspace: Option<&WorkspaceName>,
) -> anyhow::Result<Option<&'a jackin_config::WorkspaceConfig>> {
    workspace
        .map(|name| {
            config
                .workspaces
                .get(name.as_str())
                .ok_or_else(|| anyhow::anyhow!("workspace {name} is not configured"))
        })
        .transpose()
}

#[derive(serde::Serialize)]
struct AdmittedIdentity<'a> {
    config_id: &'a str,
    agent: jackin_core::Agent,
    account_id: &'a str,
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
    admitted: &[AdmittedInstance],
) -> anyhow::Result<String> {
    // Role defaults are not hashed after admission. The binding for an
    // admitted agent remains a relevant capability revision: changing it can
    // change which credential a reconnect would authorize.
    let _ = role;
    let ws = configured_workspace(config, workspace)?;
    let mut admitted = admitted.to_vec();
    admitted.sort_by(|left, right| {
        left.config_id
            .cmp(&right.config_id)
            .then(left.agent.slug().cmp(right.agent.slug()))
            .then(left.account_id.cmp(&right.account_id))
    });
    let admitted_identities = admitted
        .iter()
        .map(|instance| AdmittedIdentity {
            config_id: &instance.config_id,
            agent: instance.agent,
            account_id: &instance.account_id,
        })
        .collect::<Vec<_>>();
    let mut credential_revisions = std::collections::BTreeMap::new();
    let mut capability_revisions = std::collections::BTreeMap::new();
    for instance in &admitted {
        let account = config.accounts.get(&instance.account_id);
        credential_revisions.insert(
            instance.account_id.clone(),
            account.map(|account| {
                (
                    account.enabled,
                    account.provider,
                    account.credential.clone(),
                )
            }),
        );
        let declared = config
            .agent_configurations
            .get(&instance.config_id)
            .map(|configuration| {
                (
                    configuration.agent,
                    configuration.account.clone(),
                    configuration.model.clone(),
                    configuration.base_url.clone(),
                    configuration.invoked_via_wrapper.clone(),
                )
            });
        let binding = ws
            .and_then(|workspace| workspace.roles.get(role))
            .and_then(|role| role.account_bindings.get(&instance.agent))
            .or_else(|| ws.and_then(|workspace| workspace.account_bindings.get(&instance.agent)))
            .or_else(|| config.account_bindings.get(&instance.agent))
            .cloned();
        capability_revisions.insert(
            instance.config_id.clone(),
            (
                instance.agent,
                instance.account_id.clone(),
                declared,
                account.map(|account| account.supports_agent(instance.agent)),
                ws.map(|workspace| {
                    workspace
                        .accounts
                        .iter()
                        .any(|account_id| account_id == &instance.account_id)
                }),
                binding,
            ),
        );
    }
    let bytes = serde_json::to_vec(&(
        "account-config-v7-admitted-revisions",
        admitted_identities,
        credential_revisions,
        capability_revisions,
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
    let manifest = InstanceManifest::read(root)?;
    Ok(stored
        == account_configuration_fingerprint(
            config,
            workspace,
            role,
            &manifest.admitted_instances,
        )?)
}

pub(super) struct AccountConfigurationRecord<'a> {
    pub(super) root: &'a Path,
    pub(super) paths: &'a jackin_core::JackinPaths,
    pub(super) revision: &'a AccountConfigRevision,
    pub(super) config: &'a AppConfig,
    pub(super) admission_config: &'a AppConfig,
    pub(super) workspace: Option<&'a WorkspaceName>,
    pub(super) role: &'a str,
    pub(super) admitted: &'a [AdmittedInstance],
}

pub(super) fn record_account_configuration(
    record: AccountConfigurationRecord<'_>,
) -> anyhow::Result<()> {
    let AccountConfigurationRecord {
        root,
        paths,
        revision,
        config,
        admission_config,
        workspace,
        role,
        admitted,
    } = record;
    let selected_fingerprint =
        account_configuration_fingerprint(config, workspace, role, admitted)?;
    let admission_fingerprint =
        account_configuration_fingerprint(admission_config, workspace, role, admitted)?;
    let snapshot = revision.current_snapshot(paths)?;
    anyhow::ensure!(
        admission_fingerprint
            == account_configuration_fingerprint(&snapshot.config, workspace, role, admitted)?,
        "account configuration changed during launch; aborting credential publication"
    );
    std::fs::write(root.join(ACCOUNT_FINGERPRINT_FILE), selected_fingerprint)?;
    std::fs::write(root.join("account-admission.sha256"), admission_fingerprint)?;
    revision.ensure_current(paths)?;
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
    let manifest = InstanceManifest::read(root)?;
    Ok(stored
        == account_configuration_fingerprint(
            config,
            workspace,
            role,
            &manifest.admitted_instances,
        )?)
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
