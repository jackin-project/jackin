//! Tests for `state`.
use super::*;
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
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
        container_name: "jk-a1b2c3d4-thearchitect".into(),
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
    let a = data.path().join("jk-a1b2c3d4-thearchitect");
    std::fs::create_dir_all(&a).unwrap();
    let mut rec_a = sample_record();
    rec_a.container_name = "jk-a1b2c3d4-thearchitect".into();
    write_records(&a, std::slice::from_ref(&rec_a)).unwrap();
    // Container B: workspace=jackin
    let b = data.path().join("jk-k7p9m2xq-thebuilder");
    std::fs::create_dir_all(&b).unwrap();
    let mut rec_b = sample_record();
    rec_b.container_name = "jk-k7p9m2xq-thebuilder".into();
    rec_b.scratch_branch = "jackin/scratch/the-builder".into();
    write_records(&b, std::slice::from_ref(&rec_b)).unwrap();
    // Container C: workspace=docs (must be skipped when filtering by jackin)
    let c = data.path().join("jk-b2c3d4e5-docwriter");
    std::fs::create_dir_all(&c).unwrap();
    let mut rec_c = sample_record();
    rec_c.workspace = "docs".into();
    rec_c.container_name = "jk-b2c3d4e5-docwriter".into();
    write_records(&c, &[rec_c]).unwrap();

    let mut found = list_records_for_workspace(data.path(), &wn("jackin")).unwrap();
    found.sort_by(|x, y| x.container_name.cmp(&y.container_name));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0], rec_a);
    assert_eq!(found[1], rec_b);
}

#[test]
fn list_records_for_workspace_returns_empty_when_data_dir_missing() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("nope");
    let result = list_records_for_workspace(&missing, &wn("jackin")).unwrap();
    assert!(result.is_empty());
}

#[test]
fn list_records_for_workspace_ignores_non_jackin_dirs() {
    let data = TempDir::new().unwrap();
    let other = data.path().join("not-a-jackin-capsule");
    std::fs::create_dir_all(&other).unwrap();
    let mut rec = sample_record();
    rec.container_name = "not-a-jackin-capsule".into();
    write_records(&other, &[rec]).unwrap();
    let result = list_records_for_workspace(data.path(), &wn("jackin")).unwrap();
    assert!(result.is_empty());
}
