use crate::isolation::MountIsolation;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ISOLATION_FILE: &str = "isolation.json";
const STATE_DIR: &str = ".jackin";
const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStatus {
    Active,
    PreservedDirty,
    PreservedUnpushed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationRecord {
    pub workspace: String,
    pub mount_dst: String,
    pub original_src: String,
    pub isolation: MountIsolation,
    pub worktree_path: String,
    pub scratch_branch: String,
    pub base_commit: String,
    pub selector_key: String,
    pub container_name: String,
    pub cleanup_status: CleanupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IsolationFile {
    version: u32,
    #[serde(default)]
    records: Vec<IsolationRecord>,
}

/// Path to `isolation.json` for a given container's state directory.
/// `container_state_dir` is `<data_dir>/jackin-<container>`.
pub fn isolation_file_path(container_state_dir: &Path) -> PathBuf {
    container_state_dir.join(STATE_DIR).join(ISOLATION_FILE)
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
    anyhow::ensure!(
        file.version == CURRENT_VERSION,
        "unsupported isolation.json version {} at {}; expected {}",
        file.version,
        path.display(),
        CURRENT_VERSION
    );
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
    if let Some(existing) = records.iter_mut().find(|r| r.mount_dst == record.mount_dst) {
        *existing = record;
    } else {
        records.push(record);
    }
    write_records(container_state_dir, &records)
}

/// Remove the record with the matching `mount_dst`. No-op if missing.
pub fn remove_record(container_state_dir: &Path, mount_dst: &str) -> anyhow::Result<()> {
    let mut records = read_records(container_state_dir)?;
    let before = records.len();
    records.retain(|r| r.mount_dst != mount_dst);
    if records.len() != before {
        write_records(container_state_dir, &records)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_record() -> IsolationRecord {
        IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: "/workspace/jackin".into(),
            original_src: "/home/u/projects/jackin".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: "/home/u/.jackin/data/jackin-x/isolated/workspace/jackin".into(),
            scratch_branch: "jackin/scratch/the-architect".into(),
            base_commit: "deadbeef".into(),
            selector_key: "the-architect".into(),
            container_name: "jackin-the-architect".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    #[test]
    fn read_records_returns_empty_when_file_missing() {
        let dir = TempDir::new().unwrap();
        assert!(read_records(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn write_then_read_roundtrip_preserves_record() {
        let dir = TempDir::new().unwrap();
        let rec = sample_record();
        write_records(dir.path(), &[rec.clone()]).unwrap();
        let read = read_records(dir.path()).unwrap();
        assert_eq!(read, vec![rec]);
    }

    #[test]
    fn write_emits_version_1_envelope() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[sample_record()]).unwrap();
        let raw = std::fs::read_to_string(isolation_file_path(dir.path())).unwrap();
        assert!(raw.contains("\"version\": 1"));
        assert!(raw.contains("\"records\""));
    }

    #[test]
    fn read_record_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[sample_record()]).unwrap();
        assert!(read_record(dir.path(), "/nope").unwrap().is_none());
    }

    #[test]
    fn read_record_returns_match() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[sample_record()]).unwrap();
        let r = read_record(dir.path(), "/workspace/jackin").unwrap();
        assert!(r.is_some());
    }

    #[test]
    fn upsert_replaces_existing_by_dst() {
        let dir = TempDir::new().unwrap();
        let mut rec = sample_record();
        write_records(dir.path(), &[rec.clone()]).unwrap();
        rec.base_commit = "cafe".into();
        upsert_record(dir.path(), rec.clone()).unwrap();
        let all = read_records(dir.path()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].base_commit, "cafe");
    }

    #[test]
    fn upsert_appends_when_dst_new() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[sample_record()]).unwrap();
        let mut other = sample_record();
        other.mount_dst = "/workspace/docs".into();
        upsert_record(dir.path(), other).unwrap();
        assert_eq!(read_records(dir.path()).unwrap().len(), 2);
    }

    #[test]
    fn remove_record_drops_match_and_keeps_others() {
        let dir = TempDir::new().unwrap();
        let mut other = sample_record();
        other.mount_dst = "/workspace/docs".into();
        write_records(dir.path(), &[sample_record(), other.clone()]).unwrap();
        remove_record(dir.path(), "/workspace/jackin").unwrap();
        let all = read_records(dir.path()).unwrap();
        assert_eq!(all, vec![other]);
    }

    #[test]
    fn remove_record_is_noop_when_missing() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[sample_record()]).unwrap();
        remove_record(dir.path(), "/nope").unwrap();
        assert_eq!(read_records(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn unsupported_version_errors_clearly() {
        let dir = TempDir::new().unwrap();
        let path = isolation_file_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"version": 99, "records": []}"#).unwrap();
        let err = read_records(dir.path()).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported isolation.json version 99")
        );
    }
}
