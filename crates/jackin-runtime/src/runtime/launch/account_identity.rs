// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account admission identity for a container's immutable environment.
#![expect(
    clippy::disallowed_methods,
    reason = "credential directory durability runs inside the launch blocking task"
)]

use crate::instance::{AdmittedInstance, InstanceManifest};
use anyhow::Context as _;
use jackin_config::{AppConfig, ConfigGeneration, ConfigReadGuard, ReadOnlyConfigSnapshot};
use jackin_core::WorkspaceName;
use jackin_docker::docker_client::DockerApi;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const ACCOUNT_FINGERPRINT_FILE: &str = "account-config.sha256";
const ACCOUNT_ADMISSION_FILE: &str = "account-admission.sha256";
const ACCOUNT_MARKER_TRANSACTION_FILE: &str = ".account-markers-transaction";
const ACCOUNT_MARKER_TRANSACTION_VERSION: u8 = 1;
const ACCOUNT_MARKER_STAGE_PREFIX: &str = ".account-marker-stage-";
const CREDENTIAL_TRANSACTION_FILE: &str = ".credentials-transaction";
const CREDENTIAL_TRANSACTION_VERSION: u8 = 1;
const CREDENTIAL_STAGE_PREFIX: &str = ".credentials-stage-";

#[derive(Debug)]
pub(crate) struct GenerationLeaseViolation(String);

impl std::fmt::Display for GenerationLeaseViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "generation lease admission failed: {}", self.0)
    }
}

impl std::error::Error for GenerationLeaseViolation {}

/// Immutable persisted-config revision held from launch account resolution
/// through credential publication and admission recording.
#[derive(Debug)]
pub(crate) struct AccountConfigRevision {
    generation: ConfigGeneration,
    _read_guard: ConfigReadGuard,
}

impl AccountConfigRevision {
    /// Acquire a strict immutable lease over the persisted config generation.
    ///
    /// The required lock makes absence of the writer lock an admission failure;
    /// the generation check detects writers that bypass the advisory protocol.
    pub(crate) fn acquire(paths: &jackin_core::JackinPaths) -> anyhow::Result<Self> {
        Self::acquire_inner(paths, None).map_err(mark_generation_lease_error)
    }

    /// Acquire a lease only when the caller's in-memory config is the same
    /// snapshot that was admitted from disk.
    pub(crate) fn acquire_bound(
        paths: &jackin_core::JackinPaths,
        caller_config: &AppConfig,
    ) -> anyhow::Result<Self> {
        Self::acquire_inner(paths, Some(caller_config)).map_err(mark_generation_lease_error)
    }

    fn acquire_inner(
        paths: &jackin_core::JackinPaths,
        caller_config: Option<&AppConfig>,
    ) -> anyhow::Result<Self> {
        let read_guard = jackin_config::acquire_config_read_lock_required(&paths.config_file)?;
        let snapshot = verified_config_snapshot(paths)?;
        ensure_lock_file_present(paths)?;
        if let Some(caller_config) = caller_config {
            ensure_caller_snapshot_matches(caller_config, &snapshot.config)?;
        }
        Ok(Self {
            generation: snapshot.generation,
            _read_guard: read_guard,
        })
    }

    pub(crate) fn current_snapshot(
        &self,
        paths: &jackin_core::JackinPaths,
    ) -> anyhow::Result<ReadOnlyConfigSnapshot> {
        self.current_snapshot_inner(paths)
            .map_err(mark_generation_lease_error)
    }

    fn current_snapshot_inner(
        &self,
        paths: &jackin_core::JackinPaths,
    ) -> anyhow::Result<ReadOnlyConfigSnapshot> {
        let _read_guard = jackin_config::acquire_config_read_lock_required(&paths.config_file)?;
        let snapshot = verified_config_snapshot(paths)?;
        ensure_lock_file_present(paths)?;
        anyhow::ensure!(
            snapshot.generation == self.generation,
            "configuration changed during launch; aborting credential publication"
        );
        Ok(snapshot)
    }

    pub(crate) fn ensure_current(&self, paths: &jackin_core::JackinPaths) -> anyhow::Result<()> {
        self.current_snapshot(paths).map(drop)
    }

    /// Hold the generation read lease across an awaited operation and validate
    /// again after it completes. Cooperative config writers cannot commit while
    /// this future owns the read guard; the post-check closes direct-writer and
    /// crash/reconnect races that bypass that protocol.
    pub(crate) async fn run_with_lease<T, F>(
        &self,
        paths: &jackin_core::JackinPaths,
        operation: F,
    ) -> anyhow::Result<T>
    where
        F: std::future::Future<Output = anyhow::Result<T>>,
    {
        self.ensure_current(paths)?;
        let operation_result = operation.await;
        let lease_result = self.ensure_current(paths);
        match (operation_result, lease_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(operation_error), Ok(())) => Err(operation_error),
            (Ok(_), Err(lease_error)) => Err(lease_error),
            (Err(operation_error), Err(lease_error)) => Err(lease_error.context(format!(
                "Docker operation failed while generation lease was invalid: {operation_error:#}"
            ))),
        }
    }
}

/// Validate a generation after an awaited container start/run. A direct writer
/// can bypass the advisory read lock while Docker is in flight, so a mismatch
/// after the operation means the container already holds stale credentials and
/// must be force-removed before the error escapes.
pub(crate) async fn ensure_current_or_remove_stale_container(
    revision: &AccountConfigRevision,
    paths: &jackin_core::JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let Err(error) = revision.ensure_current(paths) else {
        return Ok(());
    };
    if let Err(cleanup_error) = docker.remove_container(container_name).await {
        return Err(error.context(format!(
            "stale-generation container {container_name} cleanup failed: {cleanup_error:#}"
        )));
    }
    Err(error)
}

fn mark_generation_lease_error(error: anyhow::Error) -> anyhow::Error {
    let summary = error.to_string();
    error.context(GenerationLeaseViolation(summary))
}

fn ensure_lock_file_present(paths: &jackin_core::JackinPaths) -> anyhow::Result<()> {
    let lock_path = paths.config_file.with_file_name("config.lock");
    std::fs::metadata(&lock_path)
        .with_context(|| format!("required config lock is absent: {}", lock_path.display()))?;
    Ok(())
}

fn ensure_caller_snapshot_matches(caller: &AppConfig, persisted: &AppConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        serde_json::to_vec(caller)? == serde_json::to_vec(persisted)?,
        "caller configuration snapshot is stale; reload configuration and retry"
    );
    Ok(())
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
enum AccountMarkerPhase {
    Prepared,
    SelectedPublished,
    AdmissionPublished,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct AccountMarkerTransaction {
    schema_version: u8,
    phase: AccountMarkerPhase,
    selected_tmp: String,
    admission_tmp: String,
    previous_selected: Option<Vec<u8>>,
    previous_admission: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialWriteFailure {
    StagedFile(usize),
    PreviousRename,
    Install,
    #[cfg(test)]
    InstallAndRollbackCleanup,
    Cleanup,
    RollbackCleanup,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialSwapPhase {
    Prepared,
    PreviousMoved,
    Installed,
    CleanupPending,
    RollbackTargetQuarantined,
    Restored,
    Committed,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct CredentialSwapTransaction {
    schema_version: u8,
    phase: CredentialSwapPhase,
    staged: Option<String>,
    previous: Option<String>,
    cleanup: Option<String>,
    rollback: Option<String>,
}

#[cfg(test)]
thread_local! {
    static CREDENTIAL_WRITE_FAILURE: std::cell::Cell<Option<CredentialWriteFailure>> =
        const { std::cell::Cell::new(None) };
}

fn maybe_inject_credential_write_failure(point: CredentialWriteFailure) -> anyhow::Result<()> {
    #[cfg(test)]
    if CREDENTIAL_WRITE_FAILURE.with(|failure| {
        matches!(
            (failure.get(), point),
            (
                Some(CredentialWriteFailure::InstallAndRollbackCleanup),
                CredentialWriteFailure::Install | CredentialWriteFailure::RollbackCleanup
            )
        ) || failure.get() == Some(point)
    }) {
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

fn is_valid_credential_stage_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(CREDENTIAL_STAGE_PREFIX) else {
        return false;
    };
    let mut parts = suffix.split('-');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(pid), Some(sequence), None)
            if !pid.is_empty()
                && !sequence.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
    )
}

/// Remove only private, daemon-created staging directories left without a
/// transaction. Symlinks and malformed names are deliberately left alone.
fn sweep_orphan_credential_staging(root: &Path, active: Option<&str>) -> anyhow::Result<()> {
    let mut removed = false;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if active == Some(name.as_str()) || !is_valid_credential_stage_name(&name) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        std::fs::remove_dir_all(entry.path())?;
        removed = true;
    }
    if removed {
        sync_directory(root)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

fn credential_transaction_entry(root: &Path, name: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = Path::new(name);
    anyhow::ensure!(
        !name.is_empty()
            && name.starts_with(".credentials-")
            && path.components().count() == 1
            && matches!(
                path.components().next(),
                Some(std::path::Component::Normal(_))
            ),
        "invalid private credential transaction entry: {name:?}"
    );
    Ok(root.join(path))
}

fn validate_credential_transaction(transaction: &CredentialSwapTransaction) -> anyhow::Result<()> {
    anyhow::ensure!(
        transaction.schema_version == CREDENTIAL_TRANSACTION_VERSION,
        "unsupported private credential transaction version: {}",
        transaction.schema_version
    );
    for name in [
        transaction.staged.as_deref(),
        transaction.previous.as_deref(),
        transaction.cleanup.as_deref(),
        transaction.rollback.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        credential_transaction_entry(Path::new("."), name)?;
    }
    Ok(())
}

fn persist_credential_transaction(
    root: &Path,
    transaction: &CredentialSwapTransaction,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    validate_credential_transaction(transaction)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec(transaction)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join(CREDENTIAL_TRANSACTION_FILE))
        .map_err(|error| error.error)?;
    sync_directory(root)?;
    Ok(())
}

fn load_credential_transaction(root: &Path) -> anyhow::Result<Option<CredentialSwapTransaction>> {
    let path = root.join(CREDENTIAL_TRANSACTION_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let transaction: CredentialSwapTransaction = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid private credential transaction: {}", path.display()))?;
    validate_credential_transaction(&transaction)?;
    Ok(Some(transaction))
}

fn clear_credential_transaction(root: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(root.join(CREDENTIAL_TRANSACTION_FILE)) {
        Ok(()) => sync_directory(root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn credential_entry_exists(root: &Path, name: Option<&str>) -> anyhow::Result<bool> {
    let Some(name) = name else {
        return Ok(false);
    };
    match std::fs::symlink_metadata(credential_transaction_entry(root, name)?) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn credential_path_exists(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn remove_credential_entry(root: &Path, name: Option<&str>) -> anyhow::Result<()> {
    let Some(name) = name else {
        return Ok(());
    };
    let path = credential_transaction_entry(root, name)?;
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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

fn finish_credential_rollback(
    root: &Path,
    transaction: &mut CredentialSwapTransaction,
    installed: bool,
    inject_cleanup_failure: bool,
) -> anyhow::Result<()> {
    validate_credential_transaction(transaction)?;
    let directory = root.join("credentials");
    let previous_exists = credential_entry_exists(root, transaction.previous.as_deref())?;
    let cleanup_exists = credential_entry_exists(root, transaction.cleanup.as_deref())?;
    anyhow::ensure!(
        !(previous_exists && cleanup_exists),
        "private credential transaction has two rollback sources"
    );

    let live_exists = credential_path_exists(&directory)?;
    let rollback_exists = credential_entry_exists(root, transaction.rollback.as_deref())?;
    if installed && live_exists && !rollback_exists {
        let rollback_name = if let Some(name) = transaction.rollback.as_deref() {
            name.to_owned()
        } else {
            unique_credential_sibling(root, "credentials-rollback")?
                .file_name()
                .and_then(|name| name.to_str())
                .context("private credential rollback path is not valid UTF-8")?
                .to_owned()
        };
        let rollback = credential_transaction_entry(root, &rollback_name)?;
        transaction.rollback = Some(rollback_name);
        transaction.phase = CredentialSwapPhase::RollbackTargetQuarantined;
        persist_credential_transaction(root, transaction)?;
        std::fs::rename(&directory, rollback)?;
        sync_directory(root)?;
        persist_credential_transaction(root, transaction)?;
    }

    remove_credential_entry(root, transaction.staged.as_deref())?;
    transaction.staged = None;

    let previous = credential_entry_exists(root, transaction.previous.as_deref())?;
    let cleanup = credential_entry_exists(root, transaction.cleanup.as_deref())?;
    anyhow::ensure!(
        !(previous && cleanup),
        "private credential rollback is ambiguous"
    );
    let source = if previous {
        transaction.previous.clone()
    } else if cleanup {
        transaction.cleanup.clone()
    } else {
        None
    };

    if credential_path_exists(&directory)? {
        anyhow::ensure!(
            source.is_none(),
            "private credential rollback found a live directory and a previous directory"
        );
    } else {
        let Some(source) = source.as_deref() else {
            anyhow::ensure!(
                !installed || transaction.rollback.is_some(),
                "private credential transaction lost its live directory"
            );
            transaction.phase = CredentialSwapPhase::Restored;
            persist_credential_transaction(root, transaction)?;
            if transaction.rollback.is_some() {
                if inject_cleanup_failure {
                    maybe_inject_credential_write_failure(CredentialWriteFailure::RollbackCleanup)?;
                }
                remove_credential_entry(root, transaction.rollback.as_deref())?;
                transaction.rollback = None;
                persist_credential_transaction(root, transaction)?;
            }
            clear_credential_transaction(root)?;
            return Ok(());
        };
        let source_path = credential_transaction_entry(root, source)?;
        std::fs::rename(source_path, &directory)?;
        sync_directory(root)?;
        if transaction.previous.as_deref() == Some(source) {
            transaction.previous = None;
        }
        if transaction.cleanup.as_deref() == Some(source) {
            transaction.cleanup = None;
        }
    }

    transaction.phase = CredentialSwapPhase::Restored;
    persist_credential_transaction(root, transaction)?;

    if transaction.rollback.is_some() {
        if inject_cleanup_failure {
            maybe_inject_credential_write_failure(CredentialWriteFailure::RollbackCleanup)?;
        }
        remove_credential_entry(root, transaction.rollback.as_deref())?;
        transaction.rollback = None;
        persist_credential_transaction(root, transaction)?;
    }
    clear_credential_transaction(root)
}

fn rollback_credential_swap(
    root: &Path,
    transaction: &mut CredentialSwapTransaction,
    installed: bool,
    cause: anyhow::Error,
) -> anyhow::Result<()> {
    if let Err(rollback_error) = finish_credential_rollback(root, transaction, installed, true) {
        return Err(cause.context(format!(
            "credential publication failed and rollback failed: {rollback_error:#}"
        )));
    }
    Err(cause)
}

fn recover_credential_swap(root: &Path) -> anyhow::Result<()> {
    let transaction = load_credential_transaction(root)?;
    sweep_orphan_credential_staging(
        root,
        transaction.as_ref().and_then(|value| value.staged.as_deref()),
    )?;
    let Some(mut transaction) = transaction else {
        return Ok(());
    };
    let directory = root.join("credentials");
    let live_exists = credential_path_exists(&directory)?;
    let previous_exists = credential_entry_exists(root, transaction.previous.as_deref())?;
    let cleanup_exists = credential_entry_exists(root, transaction.cleanup.as_deref())?;
    if live_exists
        && !previous_exists
        && !cleanup_exists
        && transaction.rollback.is_none()
        && matches!(
            transaction.phase,
            CredentialSwapPhase::Installed | CredentialSwapPhase::CleanupPending
        )
    {
        transaction.phase = CredentialSwapPhase::Committed;
        persist_credential_transaction(root, &transaction)?;
        return clear_credential_transaction(root);
    }
    match transaction.phase {
        CredentialSwapPhase::Prepared => {
            let previous_exists = credential_entry_exists(root, transaction.previous.as_deref())?;
            let live_exists = credential_path_exists(&directory)?;
            anyhow::ensure!(
                !(previous_exists && live_exists),
                "prepared credential transaction has ambiguous live state"
            );
            finish_credential_rollback(root, &mut transaction, false, false)?;
        }
        CredentialSwapPhase::PreviousMoved => {
            let installed = credential_path_exists(&directory)?;
            finish_credential_rollback(root, &mut transaction, installed, false)?;
        }
        CredentialSwapPhase::Installed
        | CredentialSwapPhase::CleanupPending
        | CredentialSwapPhase::RollbackTargetQuarantined => {
            finish_credential_rollback(root, &mut transaction, true, false)?;
        }
        CredentialSwapPhase::Restored => {
            finish_credential_rollback(root, &mut transaction, false, false)?;
        }
        CredentialSwapPhase::Committed => {
            remove_credential_entry(root, transaction.staged.as_deref())?;
            remove_credential_entry(root, transaction.previous.as_deref())?;
            remove_credential_entry(root, transaction.cleanup.as_deref())?;
            remove_credential_entry(root, transaction.rollback.as_deref())?;
            clear_credential_transaction(root)?;
        }
    }
    Ok(())
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

fn account_marker_transaction_entry(root: &Path, name: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = Path::new(name);
    anyhow::ensure!(
        !name.is_empty()
            && name.starts_with(ACCOUNT_MARKER_STAGE_PREFIX)
            && path.components().count() == 1
            && matches!(
                path.components().next(),
                Some(std::path::Component::Normal(_))
            ),
        "invalid account marker staging entry: {name:?}"
    );
    Ok(root.join(path))
}

fn validate_account_marker_transaction(
    transaction: &AccountMarkerTransaction,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        transaction.schema_version == ACCOUNT_MARKER_TRANSACTION_VERSION,
        "unsupported account marker transaction version: {}",
        transaction.schema_version
    );
    account_marker_transaction_entry(Path::new("."), &transaction.selected_tmp)?;
    account_marker_transaction_entry(Path::new("."), &transaction.admission_tmp)?;
    Ok(())
}

fn persist_account_marker_transaction(
    root: &Path,
    transaction: &AccountMarkerTransaction,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    validate_account_marker_transaction(transaction)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec(transaction)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join(ACCOUNT_MARKER_TRANSACTION_FILE))
        .map_err(|error| error.error)?;
    sync_directory(root)?;
    Ok(())
}

fn load_account_marker_transaction(
    root: &Path,
) -> anyhow::Result<Option<AccountMarkerTransaction>> {
    let path = root.join(ACCOUNT_MARKER_TRANSACTION_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let transaction: AccountMarkerTransaction = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid account marker transaction: {}", path.display()))?;
    validate_account_marker_transaction(&transaction)?;
    Ok(Some(transaction))
}

fn clear_account_marker_transaction(root: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(root.join(ACCOUNT_MARKER_TRANSACTION_FILE)) {
        Ok(()) => sync_directory(root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn write_account_marker_stage(path: &Path, value: &[u8]) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(value)?;
    file.sync_all()?;
    Ok(())
}

fn remove_account_marker_stage(root: &Path, name: &str) -> anyhow::Result<()> {
    let path = account_marker_transaction_entry(root, name)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
                "account marker staging entry is not a regular file: {}",
                path.display()
            );
            std::fs::remove_file(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn write_account_marker_atomically(
    root: &Path,
    path: &Path,
    value: &[u8],
) -> anyhow::Result<()> {
    let stage = unique_credential_sibling(root, "account-marker-stage")?;
    write_account_marker_stage(&stage, value)?;
    if let Err(error) = std::fs::rename(&stage, path) {
        drop(std::fs::remove_file(&stage));
        return Err(error.into());
    }
    sync_directory(root)?;
    Ok(())
}

fn recover_account_marker_transaction(root: &Path) -> anyhow::Result<()> {
    let Some(transaction) = load_account_marker_transaction(root)? else {
        return Ok(());
    };
    let selected_path = root.join(ACCOUNT_FINGERPRINT_FILE);
    let admission_path = root.join(ACCOUNT_ADMISSION_FILE);
    restore_marker(root, &selected_path, transaction.previous_selected.as_deref())?;
    restore_marker(root, &admission_path, transaction.previous_admission.as_deref())?;
    remove_account_marker_stage(root, &transaction.selected_tmp)?;
    remove_account_marker_stage(root, &transaction.admission_tmp)?;
    clear_account_marker_transaction(root)
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
    recover_account_marker_transaction(root)?;
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
    publish_account_fingerprints(
        root,
        paths,
        revision,
        &selected_fingerprint,
        &admission_fingerprint,
    )
}

fn publish_account_fingerprints(
    root: &Path,
    paths: &jackin_core::JackinPaths,
    revision: &AccountConfigRevision,
    selected_fingerprint: &str,
    admission_fingerprint: &str,
) -> anyhow::Result<()> {
    recover_account_marker_transaction(root)?;
    let selected_path = root.join(ACCOUNT_FINGERPRINT_FILE);
    let admission_path = root.join(ACCOUNT_ADMISSION_FILE);
    let selected_tmp = unique_credential_sibling(root, "account-marker-stage")?;
    let admission_tmp = unique_credential_sibling(root, "account-marker-stage")?;
    let selected_tmp_name = selected_tmp
        .file_name()
        .and_then(|name| name.to_str())
        .context("account marker staging path is not valid UTF-8")?
        .to_owned();
    let admission_tmp_name = admission_tmp
        .file_name()
        .and_then(|name| name.to_str())
        .context("account marker staging path is not valid UTF-8")?
        .to_owned();
    let previous_selected = read_marker(&selected_path)?;
    let previous_admission = read_marker(&admission_path)?;

    let mut transaction = AccountMarkerTransaction {
        schema_version: ACCOUNT_MARKER_TRANSACTION_VERSION,
        phase: AccountMarkerPhase::Prepared,
        selected_tmp: selected_tmp_name,
        admission_tmp: admission_tmp_name,
        previous_selected,
        previous_admission,
    };
    persist_account_marker_transaction(root, &transaction)?;

    let publish_result = (|| -> anyhow::Result<()> {
        write_account_marker_stage(&selected_tmp, selected_fingerprint.as_bytes())?;
        write_account_marker_stage(&admission_tmp, admission_fingerprint.as_bytes())?;
        std::fs::rename(&selected_tmp, &selected_path)?;
        sync_directory(root)?;
        transaction.phase = AccountMarkerPhase::SelectedPublished;
        persist_account_marker_transaction(root, &transaction)?;
        std::fs::rename(&admission_tmp, &admission_path)?;
        sync_directory(root)?;
        transaction.phase = AccountMarkerPhase::AdmissionPublished;
        persist_account_marker_transaction(root, &transaction)?;
        revision.ensure_current(paths)?;
        Ok(())
    })();

    if let Err(error) = publish_result {
        return match recover_account_marker_transaction(root) {
            Ok(()) => Err(error),
            Err(recovery_error) => Err(error.context(format!(
                "account marker publication failed and recovery failed: {recovery_error:#}"
            ))),
        };
    }
    clear_account_marker_transaction(root)
}

fn read_marker(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn restore_marker(root: &Path, path: &Path, previous: Option<&[u8]>) -> anyhow::Result<()> {
    match previous {
        Some(bytes) => write_account_marker_atomically(root, path, bytes)?,
        None => match std::fs::remove_file(path) {
            Ok(()) => sync_directory(root)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }
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
    recover_account_marker_transaction(root)?;
    let stored = match std::fs::read_to_string(root.join(ACCOUNT_ADMISSION_FILE)) {
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

#[expect(
    clippy::too_many_lines,
    reason = "credential publication is one transaction with explicit durable state transitions"
)]
pub(super) fn write_account_credentials(
    root: &Path,
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::create_dir_all(root)?;
    recover_credential_swap(root)?;
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

    // Allocate and journal the private replacement path before creating it.
    // A crash can therefore leave only a recoverable journaled path, never an
    // unowned credential staging directory.
    let staged_path = unique_credential_sibling(root, "credentials-stage")?;
    let staged_name = staged_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("private credential staging path is not valid UTF-8")?
        .to_owned();
    let previous_name = if had_previous {
        Some(
            unique_credential_sibling(root, "credentials-previous")?
                .file_name()
                .and_then(|name| name.to_str())
                .context("private credential previous path is not valid UTF-8")?
                .to_owned(),
        )
    } else {
        None
    };

    let mut transaction = CredentialSwapTransaction {
        schema_version: CREDENTIAL_TRANSACTION_VERSION,
        phase: CredentialSwapPhase::Prepared,
        staged: Some(staged_name),
        previous: previous_name,
        cleanup: None,
        rollback: None,
    };
    persist_credential_transaction(root, &transaction)?;

    // The caller holds the role/container lock while this runs. Build a fully
    // private replacement beside the live directory, then swap directory
    // names. Readers therefore observe either the old complete set or the new
    // complete set; they never observe delete-then-write intermediates.
    if let Err(error) = std::fs::create_dir(&staged_path) {
        return rollback_credential_swap(root, &mut transaction, false, error.into());
    }
    if let Err(error) = std::fs::set_permissions(
        &staged_path,
        std::fs::Permissions::from_mode(0o700),
    ) {
        return rollback_credential_swap(root, &mut transaction, false, error.into());
    }
    for (index, (instance, bytes)) in payloads.iter().enumerate() {
        let mut file = match tempfile::NamedTempFile::new_in(&staged_path) {
            Ok(file) => file,
            Err(error) => {
                return rollback_credential_swap(root, &mut transaction, false, error.into());
            }
        };
        if let Err(error) = file
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
        {
            return rollback_credential_swap(root, &mut transaction, false, error.into());
        }
        if let Err(error) = file.write_all(bytes) {
            return rollback_credential_swap(root, &mut transaction, false, error.into());
        }
        if let Err(error) = file.as_file().sync_all() {
            return rollback_credential_swap(root, &mut transaction, false, error.into());
        }
        if let Err(error) = file.persist(staged_path.join(instance)) {
            return rollback_credential_swap(root, &mut transaction, false, error.error.into());
        }
        if let Err(error) = maybe_inject_credential_write_failure(CredentialWriteFailure::StagedFile(index)) {
            return rollback_credential_swap(root, &mut transaction, false, error);
        }
    }
    if let Err(error) = sync_directory(&staged_path) {
        return rollback_credential_swap(root, &mut transaction, false, error.into());
    }

    if let Some(previous_name) = transaction.previous.as_deref() {
        let previous_directory = credential_transaction_entry(root, previous_name)?;
        if let Err(error) = std::fs::rename(&directory, previous_directory) {
            return Err(error.into());
        }
        transaction.phase = CredentialSwapPhase::PreviousMoved;
        if let Err(error) = persist_credential_transaction(root, &transaction) {
            return rollback_credential_swap(root, &mut transaction, false, error);
        }
    }
    if let Err(error) =
        maybe_inject_credential_write_failure(CredentialWriteFailure::PreviousRename)
    {
        return rollback_credential_swap(root, &mut transaction, false, error);
    }

    if let Err(error) = std::fs::rename(&staged_path, &directory) {
        return rollback_credential_swap(root, &mut transaction, false, error.into());
    }
    transaction.staged = None;
    transaction.phase = CredentialSwapPhase::Installed;
    if let Err(error) = persist_credential_transaction(root, &transaction) {
        return rollback_credential_swap(root, &mut transaction, true, error);
    }
    if let Err(error) = maybe_inject_credential_write_failure(CredentialWriteFailure::Install) {
        return rollback_credential_swap(root, &mut transaction, true, error);
    }
    if let Err(error) = sync_directory(root) {
        return rollback_credential_swap(root, &mut transaction, true, error.into());
    }

    if let Some(previous_name) = transaction.previous.clone() {
        let cleanup_path = match unique_credential_sibling(root, "credentials-cleanup") {
            Ok(path) => path,
            Err(error) => return rollback_credential_swap(root, &mut transaction, true, error),
        };
        let cleanup_name = match cleanup_path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_owned(),
            None => {
                return rollback_credential_swap(
                    root,
                    &mut transaction,
                    true,
                    anyhow::anyhow!("private credential cleanup path is not valid UTF-8"),
                );
            }
        };
        transaction.cleanup = Some(cleanup_name);
        transaction.phase = CredentialSwapPhase::CleanupPending;
        if let Err(error) = persist_credential_transaction(root, &transaction) {
            return rollback_credential_swap(root, &mut transaction, true, error);
        }
        let previous_directory = match credential_transaction_entry(root, &previous_name) {
            Ok(path) => path,
            Err(error) => return rollback_credential_swap(root, &mut transaction, true, error),
        };
        let Some(cleanup_name) = transaction.cleanup.as_deref() else {
            return rollback_credential_swap(
                root,
                &mut transaction,
                true,
                anyhow::anyhow!("credential cleanup transaction entry is missing"),
            );
        };
        let cleanup_directory = match credential_transaction_entry(root, cleanup_name) {
            Ok(path) => path,
            Err(error) => return rollback_credential_swap(root, &mut transaction, true, error),
        };
        if let Err(error) = std::fs::rename(previous_directory, cleanup_directory) {
            return rollback_credential_swap(root, &mut transaction, true, error.into());
        }
        transaction.previous = None;
        if let Err(error) = persist_credential_transaction(root, &transaction) {
            return rollback_credential_swap(root, &mut transaction, true, error);
        }
        if let Err(error) = maybe_inject_credential_write_failure(CredentialWriteFailure::Cleanup) {
            return rollback_credential_swap(root, &mut transaction, true, error);
        }
        if let Err(error) = remove_credential_entry(root, transaction.cleanup.as_deref()) {
            return rollback_credential_swap(root, &mut transaction, true, error);
        }
        transaction.cleanup = None;
    }
    transaction.phase = CredentialSwapPhase::Committed;
    if let Err(error) = persist_credential_transaction(root, &transaction) {
        return Err(error.context(
            "credential publication committed but could not finalize its transaction record",
        ));
    }
    clear_credential_transaction(root)?;
    Ok(())
}

#[cfg(test)]
mod tests;
