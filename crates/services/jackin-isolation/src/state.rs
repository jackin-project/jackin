// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `IsolationRecord` persistence: write/read `isolation.json` inside the container state directory.
//!
//! Not responsible for worktree or branch lifecycle — those are in
//! `cleanup.rs`. The file is the sole authority on whether a container has
//! active isolation that must be preserved before purge.
//!
//! Pure data types `IsolationRecord`, `CleanupStatus`, and `DriftDetection`
//! now live in `jackin-core` so that `jackin-console` can reference them
//! without depending on `jackin-runtime`. Re-exported here for existing call
//! sites in this crate and downstream consumers.

mod state_io;
use state_io::{FileSnapshot, StateDirectory};

use anyhow::Context;
use jackin_core::WorkspaceName;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

// Re-export so test code using `use super::*` still finds it.
pub use crate::MountIsolation;

// Pure data types — now in jackin-core.
pub use jackin_core::{CleanupStatus, IsolationRecord};

const ISOLATION_FILE: &str = "isolation.json";
const WORKTREE_CLEANUP_PREFIX: &str = "worktree-cleanup-";
const STATE_DIR: &str = ".jackin";
const CURRENT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsolationFile {
    version: u32,
    #[serde(default)]
    records: Vec<IsolationRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionEnvelope {
    version: u32,
    #[serde(rename = "records")]
    _records: serde::de::IgnoredAny,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalIsolationFile {
    version: u32,
    records: Vec<HistoricalIsolationRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalIsolationRecord {
    workspace: String,
    mount_dst: String,
    original_src: String,
    isolation: MountIsolation,
    worktree_path: String,
    scratch_branch: String,
    base_commit: String,
    selector_key: String,
    container_name: String,
    cleanup_status: CleanupStatus,
}

/// Identity fields present in the v3 instance manifest. Keep this separate
/// from `InstanceManifest`: v4 deliberately rejects old manifests globally,
/// while isolation v1 needs this narrow, versioned witness to recover the
/// workspace identity it did not store itself.
#[derive(Deserialize)]
struct HistoricalInstanceManifestV3 {
    version: u32,
    instance_id: String,
    container_base: String,
    #[serde(deserialize_with = "deserialize_required_workspace_name")]
    workspace_name: Option<String>,
    workspace_label: String,
    docker: HistoricalDockerIdentityV3,
    #[serde(default)]
    backend: Option<HistoricalBackendIdentityV3>,
}

#[derive(Deserialize)]
struct HistoricalDockerIdentityV3 {
    role_container: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum HistoricalBackendIdentityV3 {
    Docker { role_container: String },
    AppleContainer { container_name: String },
}

#[derive(Deserialize)]
struct InstanceManifestVersion {
    version: u32,
}

struct InstanceIdentityWitness {
    instance_id: String,
    container_base: String,
    workspace_name: Option<String>,
    workspace_label: String,
    resource_name: String,
}

fn deserialize_required_workspace_name<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

/// Path to `isolation.json` for a given container's state directory.
pub fn isolation_file_path(container_state_dir: &Path) -> PathBuf {
    container_state_dir.join(STATE_DIR).join(ISOLATION_FILE)
}

pub(crate) fn worktree_cleanup_journal_name(mount_dst: &str) -> String {
    let digest = Sha256::digest(mount_dst.as_bytes());
    let digest = hex::encode(digest);
    format!("{WORKTREE_CLEANUP_PREFIX}{digest}.json")
}

/// Snapshot counts of a container's mount records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountSummary {
    pub total: usize,
    pub dirty: usize,
    pub unpushed: usize,
}

impl MountSummary {
    #[must_use]
    pub fn from_records(records: &[IsolationRecord]) -> Self {
        Self {
            total: records.len(),
            dirty: records
                .iter()
                .filter(|r| r.cleanup_status == CleanupStatus::PreservedDirty)
                .count(),
            unpushed: records
                .iter()
                .filter(|r| r.cleanup_status == CleanupStatus::PreservedUnpushed)
                .count(),
        }
    }

    /// `Err` propagates the `isolation.json` read/parse error; callers
    /// that want the "unknown" rendering should map it themselves.
    pub fn for_state_dir(container_state_dir: &Path) -> anyhow::Result<Self> {
        Ok(Self::from_records(&read_records(container_state_dir)?))
    }

    /// Prompt-style mount summary for a container's state dir. Returns
    /// `"mounts:unknown"` when the isolation manifest can't be read.
    #[must_use]
    pub fn prompt_label_for_state_dir(state_dir: &Path) -> String {
        Self::for_state_dir(state_dir)
            .map_or_else(|_| "mounts:unknown".to_owned(), Self::prompt_label)
    }

    /// `"mounts:N dirty:N unpushed:N"`. Returns `"mounts:none"` for the
    /// empty case and `"mounts:N"` when no records are dirty/unpushed.
    #[must_use]
    pub fn prompt_label(self) -> String {
        if self.total == 0 {
            return "mounts:none".to_owned();
        }
        if self.dirty > 0 || self.unpushed > 0 {
            return format!(
                "mounts:{} dirty:{} unpushed:{}",
                self.total, self.dirty, self.unpushed
            );
        }
        format!("mounts:{}", self.total)
    }

    /// `"N total, N dirty, N unpushed"`.
    #[must_use]
    pub fn inspect_label(self) -> String {
        if self.total == 0 {
            return "none".to_owned();
        }
        if self.dirty > 0 || self.unpushed > 0 {
            return format!(
                "{} total, {} dirty, {} unpushed",
                self.total, self.dirty, self.unpushed
            );
        }
        format!("{} total", self.total)
    }
}

/// Read admitted records without publishing a migration or changing filesystem state.
/// Historical identity requires an independently bound instance manifest.
/// Returns empty when the file is missing.
pub fn read_records(container_state_dir: &Path) -> anyhow::Result<Vec<IsolationRecord>> {
    let Some(directory) = StateDirectory::open(container_state_dir, false)? else {
        return Ok(Vec::new());
    };
    read_records_in_directory(container_state_dir, &directory)
}

fn read_records_in_directory(
    container_state_dir: &Path,
    directory: &StateDirectory,
) -> anyhow::Result<Vec<IsolationRecord>> {
    let Some(source) = directory.read_file(ISOLATION_FILE)? else {
        return Ok(Vec::new());
    };
    let admission = admit_records(container_state_dir, directory, &source)?;
    validate_admission(directory, &source, &admission)?;
    Ok(admission.records)
}

struct RecordsAdmission {
    records: Vec<IsolationRecord>,
    historical_manifest: Option<FileSnapshot>,
}

fn admit_records(
    container_state_dir: &Path,
    directory: &StateDirectory,
    source: &FileSnapshot,
) -> anyhow::Result<RecordsAdmission> {
    let path = isolation_file_path(container_state_dir);
    let bytes = &source.bytes;
    let envelope: VersionEnvelope = serde_json::from_slice(bytes)
        .with_context(|| format!("parse isolation file at {}", path.display()))?;
    if envelope.version == 1 {
        return admit_historical_identity(container_state_dir, directory, source)
            .context(crate::IsolationError::IdentityRecoveryRequired { path });
    }
    if envelope.version != CURRENT_VERSION {
        return Err(crate::IsolationError::UnsupportedStateVersion {
            got: envelope.version,
            path,
            expected: CURRENT_VERSION,
        }
        .into());
    }
    let file: IsolationFile = serde_json::from_slice(bytes)
        .with_context(|| format!("parse isolation file at {}", path.display()))?;
    Ok(RecordsAdmission {
        records: file.records,
        historical_manifest: None,
    })
}

fn validate_admission(
    directory: &StateDirectory,
    source: &FileSnapshot,
    admission: &RecordsAdmission,
) -> anyhow::Result<()> {
    let mut witnesses = vec![(ISOLATION_FILE, source)];
    if let Some(manifest) = &admission.historical_manifest {
        witnesses.push(("instance.json", manifest));
    }
    directory.validate_files_unchanged(&witnesses)
}

/// Explicitly publish one fully admitted historical record set as current schema.
/// No publication occurs until every row and independent identity witness validates.
pub fn migrate_records(container_state_dir: &Path) -> anyhow::Result<Vec<IsolationRecord>> {
    let Some(directory) = StateDirectory::open(container_state_dir, false)? else {
        return Ok(Vec::new());
    };
    let Some(source) = directory.read_file(ISOLATION_FILE)? else {
        return Ok(Vec::new());
    };
    let admission = admit_records(container_state_dir, &directory, &source)?;
    validate_admission(&directory, &source, &admission)?;
    if let Some(manifest) = &admission.historical_manifest {
        let body = serde_json::to_vec_pretty(&IsolationFile {
            version: CURRENT_VERSION,
            records: admission.records.clone(),
        })?;
        directory.write_file_if_unchanged(
            ISOLATION_FILE,
            &body,
            &source,
            &[("instance.json", manifest)],
        )?;
    }
    Ok(admission.records)
}

// Admission never turns display labels into identities or publishes recovered bytes.
fn admit_historical_identity(
    state_dir: &Path,
    directory: &StateDirectory,
    source: &FileSnapshot,
) -> anyhow::Result<RecordsAdmission> {
    let file: HistoricalIsolationFile = serde_json::from_slice(&source.bytes)?;
    anyhow::ensure!(file.version == 1, "unexpected historical isolation version");
    let manifest_source = directory
        .read_file("instance.json")?
        .context("version 1 isolation identity needs its independent instance manifest")?;
    let manifest = instance_identity_witness(&manifest_source.bytes)?;
    jackin_core::WorkspaceLabel::parse(&manifest.workspace_label)?;
    let directory_name = state_dir
        .file_name()
        .and_then(|name| name.to_str())
        .context("instance state directory has no valid container name")?;
    anyhow::ensure!(
        manifest.container_base == directory_name,
        "instance manifest does not belong to this state directory"
    );
    anyhow::ensure!(
        jackin_core::instance_id_from_container_base(&manifest.container_base)
            == Some(manifest.instance_id.as_str()),
        "instance manifest ID does not match its container identity"
    );
    anyhow::ensure!(
        manifest.resource_name == directory_name,
        "instance resource does not belong to this state directory"
    );
    let workspace_name = manifest
        .workspace_name
        .as_deref()
        .map(WorkspaceName::parse)
        .transpose()?;
    let mut recovered = Vec::with_capacity(file.records.len());
    for record in file.records {
        anyhow::ensure!(
            record.container_name == directory_name,
            "isolation record belongs to a different instance"
        );
        anyhow::ensure!(
            record.workspace == manifest.workspace_label,
            "isolation label disagrees with independent instance manifest"
        );
        recovered.push(IsolationRecord {
            workspace_name: workspace_name.clone(),
            mount_dst: record.mount_dst,
            original_src: record.original_src,
            isolation: record.isolation,
            worktree_path: record.worktree_path,
            scratch_branch: record.scratch_branch,
            base_commit: record.base_commit,
            selector_key: record.selector_key,
            container_name: record.container_name,
            cleanup_status: record.cleanup_status,
        });
    }
    Ok(RecordsAdmission {
        records: recovered,
        historical_manifest: Some(manifest_source),
    })
}

fn instance_identity_witness(bytes: &[u8]) -> anyhow::Result<InstanceIdentityWitness> {
    let version: InstanceManifestVersion = serde_json::from_slice(bytes)
        .context("parse independent instance-manifest version for isolation identity recovery")?;
    match version.version {
        3 => {
            let manifest: HistoricalInstanceManifestV3 = serde_json::from_slice(bytes)
                .context("parse v3 identity witness for isolation identity recovery")?;
            anyhow::ensure!(
                manifest.version == 3,
                "unexpected historical instance-manifest version"
            );
            let resource_name = match manifest.backend {
                Some(HistoricalBackendIdentityV3::Docker { role_container }) => {
                    anyhow::ensure!(
                        role_container == manifest.docker.role_container,
                        "Docker resource identity disagrees within historical instance manifest"
                    );
                    role_container
                }
                Some(HistoricalBackendIdentityV3::AppleContainer { container_name }) => {
                    container_name
                }
                None => manifest.docker.role_container,
            };
            Ok(InstanceIdentityWitness {
                instance_id: manifest.instance_id,
                container_base: manifest.container_base,
                workspace_name: manifest.workspace_name,
                workspace_label: manifest.workspace_label,
                resource_name,
            })
        }
        current_version
            if current_version != 3
                && current_version == jackin_instance::manifest::INSTANCE_MANIFEST_VERSION =>
        {
            let manifest: jackin_instance::manifest::InstanceManifest =
                serde_json::from_slice(bytes)
                    .context("parse current identity witness for isolation identity recovery")?;
            let resource_name = match manifest.backend.as_ref() {
                Some(jackin_instance::manifest::BackendResources::AppleContainer(resources)) => {
                    &resources.container_name
                }
                Some(jackin_instance::manifest::BackendResources::Docker(resources)) => {
                    &resources.role_container
                }
                None => &manifest.docker.role_container,
            };
            Ok(InstanceIdentityWitness {
                instance_id: manifest.instance_id,
                container_base: manifest.container_base,
                workspace_name: manifest.workspace_name,
                workspace_label: manifest.workspace_label,
                resource_name: resource_name.to_owned(),
            })
        }
        _ => anyhow::bail!(
            "unsupported independent instance manifest version for isolation identity recovery"
        ),
    }
}

/// Atomically replace `isolation.json` with the supplied record set.
/// Creates the parent `.jackin/` directory if needed.
pub fn write_records(
    container_state_dir: &Path,
    records: &[IsolationRecord],
) -> anyhow::Result<()> {
    let directory = StateDirectory::open(container_state_dir, true)?
        .context("creating pinned isolation state directory")?;
    let file = IsolationFile {
        version: CURRENT_VERSION,
        records: records.to_vec(),
    };
    let body = serde_json::to_vec_pretty(&file)?;
    directory.write_file(ISOLATION_FILE, &body)?;
    Ok(())
}

/// Lookup a single record by mount destination.
pub fn read_record(
    container_state_dir: &Path,
    mount_dst: &str,
) -> anyhow::Result<Option<IsolationRecord>> {
    Ok(read_records(container_state_dir)?
        .into_iter()
        .find(|r| r.mount_dst == mount_dst))
}

/// Replace one record (by `mount_dst`) or insert if missing.
pub fn upsert_record(container_state_dir: &Path, record: IsolationRecord) -> anyhow::Result<()> {
    mutate_records(container_state_dir, true, |records| {
        if let Some(existing) = records.iter_mut().find(|r| r.mount_dst == record.mount_dst) {
            *existing = record;
        } else {
            records.push(record);
        }
        true
    })
}

/// Remove the record with the matching `mount_dst`. No-op if missing.
pub fn remove_record(container_state_dir: &Path, mount_dst: &str) -> anyhow::Result<()> {
    mutate_records(container_state_dir, false, |records| {
        let before = records.len();
        records.retain(|r| r.mount_dst != mount_dst);
        records.len() != before
    })
}

pub(crate) fn remove_record_if_matches(
    container_state_dir: &Path,
    expected: &IsolationRecord,
) -> anyhow::Result<()> {
    let mut found = false;
    let mut matched = false;
    mutate_records(container_state_dir, false, |records| {
        let Some(index) = records
            .iter()
            .position(|record| record.mount_dst == expected.mount_dst)
        else {
            return false;
        };
        found = true;
        if &records[index] != expected {
            return false;
        }
        records.remove(index);
        matched = true;
        true
    })?;
    anyhow::ensure!(
        !found || matched,
        "isolation record changed during cleanup; record retained"
    );
    Ok(())
}

pub(crate) fn read_worktree_cleanup(
    container_state_dir: &Path,
    journal_name: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(directory) = StateDirectory::open(container_state_dir, false)? else {
        return Ok(None);
    };
    Ok(directory
        .read_file(journal_name)?
        .map(|snapshot| snapshot.bytes))
}

pub(crate) fn write_worktree_cleanup(
    container_state_dir: &Path,
    journal_name: &str,
    contents: &[u8],
) -> anyhow::Result<()> {
    let directory = StateDirectory::open(container_state_dir, true)?
        .context("creating pinned isolation state directory")?;
    directory.write_file(journal_name, contents)
}

pub(crate) fn create_worktree_cleanup(
    container_state_dir: &Path,
    journal_name: &str,
    contents: &[u8],
) -> anyhow::Result<()> {
    let directory = StateDirectory::open(container_state_dir, true)?
        .context("creating pinned isolation state directory")?;
    directory.create_file(journal_name, contents)
}

pub(crate) fn remove_worktree_cleanup(
    container_state_dir: &Path,
    journal_name: &str,
) -> anyhow::Result<()> {
    let Some(directory) = StateDirectory::open(container_state_dir, false)? else {
        return Ok(());
    };
    directory.remove_file(journal_name)
}

// Keep the directory lock across the complete read-modify-write operation.
fn mutate_records(
    state_dir: &Path,
    create: bool,
    mutate: impl FnOnce(&mut Vec<IsolationRecord>) -> bool,
) -> anyhow::Result<()> {
    let Some(directory) = StateDirectory::open(state_dir, create)? else {
        return Ok(());
    };
    let source = directory.read_file(ISOLATION_FILE)?;
    let mut admission = match &source {
        Some(source) => admit_records(state_dir, &directory, source)?,
        None => RecordsAdmission {
            records: Vec::new(),
            historical_manifest: None,
        },
    };
    if let Some(source) = &source {
        validate_admission(&directory, source, &admission)?;
    }
    if mutate(&mut admission.records) {
        let body = serde_json::to_vec_pretty(&IsolationFile {
            version: CURRENT_VERSION,
            records: admission.records,
        })?;
        if let Some(source) = &source {
            let witnesses = admission
                .historical_manifest
                .as_ref()
                .map(|manifest| vec![("instance.json", manifest)])
                .unwrap_or_default();
            directory.write_file_if_unchanged(ISOLATION_FILE, &body, source, &witnesses)?;
        } else {
            directory.write_file(ISOLATION_FILE, &body)?;
        }
    }
    Ok(())
}

/// Walk every `<data_dir>/jk-*/` directory and collect records whose
/// `workspace_name` matches the given config stem. Ad-hoc records are excluded.
/// Missing data dir → empty result.
/// Per-container parse failures bubble up.
pub fn list_records_for_workspace(
    data_dir: &Path,
    workspace: &WorkspaceName,
) -> anyhow::Result<Vec<IsolationRecord>> {
    let entries = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("read data dir {}", data_dir.display()));
        }
    };
    let mut all = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with(jackin_core::CONTAINER_PREFIX_DASH) {
            continue;
        }
        let records = read_records(&entry.path())?;
        for rec in records {
            if rec.workspace_name.as_ref() == Some(workspace) {
                all.push(rec);
            }
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests;
