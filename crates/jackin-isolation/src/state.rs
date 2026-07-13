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

use anyhow::Context;
use jackin_core::WorkspaceName;
use jackin_diagnostics::debug_log;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// Re-export so test code using `use super::*` still finds it.
pub use crate::MountIsolation;

// Pure data types — now in jackin-core.
pub use jackin_core::isolation_record::{CleanupStatus, IsolationRecord};

const ISOLATION_FILE: &str = "isolation.json";
const STATE_DIR: &str = ".jackin";
const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IsolationFile {
    version: u32,
    #[serde(default)]
    records: Vec<IsolationRecord>,
}

/// Path to `isolation.json` for a given container's state directory.
pub fn isolation_file_path(container_state_dir: &Path) -> PathBuf {
    container_state_dir.join(STATE_DIR).join(ISOLATION_FILE)
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

/// Read every record for a container. Returns empty Vec when the file is
/// missing (a fresh container has no isolated mounts yet).
pub fn read_records(container_state_dir: &Path) -> anyhow::Result<Vec<IsolationRecord>> {
    let path = isolation_file_path(container_state_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read isolation file at {}", path.display()))?;
    let file: IsolationFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse isolation file at {}", path.display()))?;
    if file.version != CURRENT_VERSION {
        return Err(crate::IsolationError::UnsupportedStateVersion {
            got: file.version,
            path,
            expected: CURRENT_VERSION,
        }
        .into());
    }
    Ok(file.records)
}

/// Atomically replace `isolation.json` with the supplied record set.
/// Creates the parent `.jackin/` directory if needed.
pub fn write_records(
    container_state_dir: &Path,
    records: &[IsolationRecord],
) -> anyhow::Result<()> {
    let path = isolation_file_path(container_state_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create state dir {}", parent.display()))?;
    }
    let file = IsolationFile {
        version: CURRENT_VERSION,
        records: records.to_vec(),
    };
    let body = serde_json::to_vec_pretty(&file)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)
        .with_context(|| format!("write tmp isolation file {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    debug_log!(
        "isolation",
        "wrote {n} record(s) to {path}",
        n = records.len(),
        path = path.display(),
    );
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
    let mut records = read_records(container_state_dir)?;
    let mount_dst = record.mount_dst.clone();
    let action =
        if let Some(existing) = records.iter_mut().find(|r| r.mount_dst == record.mount_dst) {
            *existing = record;
            "replaced"
        } else {
            records.push(record);
            "inserted"
        };
    debug_log!(
        "isolation",
        "isolation.json upsert: {action} record for {dst} in {dir}",
        dst = mount_dst,
        dir = container_state_dir.display(),
    );
    write_records(container_state_dir, &records)
}

/// Remove the record with the matching `mount_dst`. No-op if missing.
pub fn remove_record(container_state_dir: &Path, mount_dst: &str) -> anyhow::Result<()> {
    let mut records = read_records(container_state_dir)?;
    let before = records.len();
    records.retain(|r| r.mount_dst != mount_dst);
    if records.len() == before {
        debug_log!(
            "isolation",
            "isolation.json remove: no record for {dst} in {dir} (no-op)",
            dst = mount_dst,
            dir = container_state_dir.display(),
        );
    } else {
        debug_log!(
            "isolation",
            "isolation.json remove: dropped record for {dst} in {dir}",
            dst = mount_dst,
            dir = container_state_dir.display(),
        );
        write_records(container_state_dir, &records)?;
    }
    Ok(())
}

/// Walk every `<data_dir>/jk-*/` directory and collect records whose
/// `workspace` matches the given name. Missing data dir → empty result.
/// Per-container parse failures bubble up.
pub fn list_records_for_workspace(
    data_dir: &Path,
    workspace: &WorkspaceName,
) -> anyhow::Result<Vec<IsolationRecord>> {
    if !data_dir.exists() {
        return Ok(Vec::new());
    }
    let key = workspace.as_str();
    let mut all = Vec::new();
    for entry in std::fs::read_dir(data_dir)
        .with_context(|| format!("read data dir {}", data_dir.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with(jackin_core::constants::CONTAINER_PREFIX_DASH) {
            continue;
        }
        let records = read_records(&entry.path())?;
        for rec in records {
            if rec.workspace == key {
                all.push(rec);
            }
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests;
