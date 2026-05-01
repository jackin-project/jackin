use crate::debug_log;
use crate::isolation::MountIsolation;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ISOLATION_FILE: &str = "isolation.json";
const STATE_DIR: &str = ".jackin";
const CURRENT_VERSION: u32 = 1;

/// Persisted cleanup state written into `isolation.json`.
///
/// `PreservedDirty` and `PreservedUnpushed` map one-to-one to the
/// `PreservedReason::Dirty` and `PreservedReason::Unpushed` variants in
/// `finalize.rs`, which drive the transient prompt wording. When adding a
/// new preservation cause, update both types.
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

const CONTAINER_DIR_PREFIX: &str = "jackin-";

/// Walk every `<data_dir>/jackin-*/` directory and collect records whose
/// `workspace` matches the given name. Missing data dir → empty result.
/// Per-container parse failures bubble up.
pub fn list_records_for_workspace(
    data_dir: &Path,
    workspace: &str,
) -> anyhow::Result<Vec<IsolationRecord>> {
    if !data_dir.exists() {
        return Ok(Vec::new());
    }
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
        if !name_str.starts_with(CONTAINER_DIR_PREFIX) {
            continue;
        }
        let records = read_records(&entry.path())?;
        for rec in records {
            if rec.workspace == workspace {
                all.push(rec);
            }
        }
    }
    Ok(all)
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
        write_records(dir.path(), std::slice::from_ref(&rec)).unwrap();
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
        write_records(dir.path(), std::slice::from_ref(&rec)).unwrap();
        rec.base_commit = "cafe".into();
        upsert_record(dir.path(), rec).unwrap();
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

    #[test]
    fn list_records_for_workspace_walks_all_container_dirs() {
        let data = TempDir::new().unwrap();
        // Container A: workspace=jackin
        let a = data.path().join("jackin-the-architect");
        std::fs::create_dir_all(&a).unwrap();
        let mut rec_a = sample_record();
        rec_a.container_name = "jackin-the-architect".into();
        write_records(&a, std::slice::from_ref(&rec_a)).unwrap();
        // Container B: workspace=jackin
        let b = data.path().join("jackin-the-builder");
        std::fs::create_dir_all(&b).unwrap();
        let mut rec_b = sample_record();
        rec_b.container_name = "jackin-the-builder".into();
        rec_b.scratch_branch = "jackin/scratch/the-builder".into();
        write_records(&b, std::slice::from_ref(&rec_b)).unwrap();
        // Container C: workspace=docs (must be skipped when filtering by jackin)
        let c = data.path().join("jackin-doc-writer");
        std::fs::create_dir_all(&c).unwrap();
        let mut rec_c = sample_record();
        rec_c.workspace = "docs".into();
        rec_c.container_name = "jackin-doc-writer".into();
        write_records(&c, &[rec_c]).unwrap();

        let mut found = list_records_for_workspace(data.path(), "jackin").unwrap();
        found.sort_by(|x, y| x.container_name.cmp(&y.container_name));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], rec_a);
        assert_eq!(found[1], rec_b);
    }

    #[test]
    fn list_records_for_workspace_returns_empty_when_data_dir_missing() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("nope");
        let result = list_records_for_workspace(&missing, "jackin").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn list_records_for_workspace_ignores_non_jackin_dirs() {
        let data = TempDir::new().unwrap();
        let other = data.path().join("not-a-jackin-container");
        std::fs::create_dir_all(&other).unwrap();
        let mut rec = sample_record();
        rec.container_name = "not-a-jackin-container".into();
        write_records(&other, &[rec]).unwrap();
        let result = list_records_for_workspace(data.path(), "jackin").unwrap();
        assert!(result.is_empty());
    }
}
