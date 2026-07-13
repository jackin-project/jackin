#[cfg(test)]
use super::detect_workspace_edit_drift;
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use crate::isolation::state::{CleanupStatus, IsolationRecord, write_records};
use jackin_core::JackinPaths;
use jackin_core::MountIsolation;
use jackin_docker::docker_client::ContainerRow;
use jackin_test_support::FakeDockerClient;
use tempfile::TempDir;

fn record_for(workspace: &str, container: &str, dst: &str, src: &str) -> IsolationRecord {
    IsolationRecord {
        workspace: workspace.into(),
        mount_dst: dst.into(),
        original_src: src.into(),
        isolation: MountIsolation::Worktree,
        worktree_path: format!("/data/{container}/isolated{dst}"),
        scratch_branch: format!("jackin/scratch/{container}"),
        base_commit: "abc".into(),
        selector_key: container
            .trim_start_matches(jackin_core::constants::CONTAINER_PREFIX_DASH)
            .into(),
        container_name: container.into(),
        cleanup_status: CleanupStatus::Active,
    }
}

fn paths_for(data: &std::path::Path) -> JackinPaths {
    JackinPaths {
        home_dir: data.into(),
        jackin_home: data.into(),
        config_dir: data.into(),
        config_file: data.join("config.toml"),
        workspaces_dir: data.join("workspaces"),
        roles_dir: data.into(),
        data_dir: data.into(),
        cache_dir: data.into(),
    }
}

fn mount(src: &str, dst: &str, iso: MountIsolation) -> jackin_config::MountConfig {
    jackin_config::MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: iso,
    }
}

#[tokio::test]
async fn detect_drift_flags_running_containers() {
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jk-a1b2c3d4-jackin");
    std::fs::create_dir_all(&cdir).unwrap();
    write_records(
        &cdir,
        std::slice::from_ref(&record_for(
            "jackin",
            "jk-a1b2c3d4-jackin",
            "/workspace/jackin",
            "/old/src",
        )),
    )
    .unwrap();

    let paths = paths_for(data.path());
    let edited = vec![mount(
        "/new/src",
        "/workspace/jackin",
        MountIsolation::Worktree,
    )];
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            ContainerRow {
                name: "jk-a1b2c3d4-jackin".to_owned(),
                labels: std::collections::HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let det = detect_workspace_edit_drift(&paths, &wn("jackin"), &edited, &docker)
        .await
        .unwrap();
    assert_eq!(
        det.running_containers,
        vec!["jk-a1b2c3d4-jackin".to_owned()]
    );
    assert!(det.stopped_records.is_empty());
}

#[tokio::test]
async fn detect_drift_flags_stopped_records_when_src_changes() {
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jk-a1b2c3d4-jackin");
    std::fs::create_dir_all(&cdir).unwrap();
    write_records(
        &cdir,
        std::slice::from_ref(&record_for(
            "jackin",
            "jk-a1b2c3d4-jackin",
            "/workspace/jackin",
            "/old/src",
        )),
    )
    .unwrap();

    let paths = paths_for(data.path());
    let edited = vec![mount(
        "/new/src",
        "/workspace/jackin",
        MountIsolation::Worktree,
    )];
    let docker = FakeDockerClient::default();
    let det = detect_workspace_edit_drift(&paths, &wn("jackin"), &edited, &docker)
        .await
        .unwrap();
    assert!(det.running_containers.is_empty());
    assert_eq!(det.stopped_records.len(), 1);
    assert_eq!(det.stopped_records[0].container_name, "jk-a1b2c3d4-jackin");
}

#[tokio::test]
async fn detect_drift_quiet_when_src_unchanged() {
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jk-a1b2c3d4-jackin");
    std::fs::create_dir_all(&cdir).unwrap();
    write_records(
        &cdir,
        std::slice::from_ref(&record_for(
            "jackin",
            "jk-a1b2c3d4-jackin",
            "/workspace/jackin",
            "/same/src",
        )),
    )
    .unwrap();

    let paths = paths_for(data.path());
    let edited = vec![mount(
        "/same/src",
        "/workspace/jackin",
        MountIsolation::Worktree,
    )];
    let docker = FakeDockerClient::default();
    let det = detect_workspace_edit_drift(&paths, &wn("jackin"), &edited, &docker)
        .await
        .unwrap();
    assert!(det.running_containers.is_empty());
    assert!(det.stopped_records.is_empty());
}

/// Documents a known V1 limitation: flipping the isolation mode
/// from `worktree` to `shared` on the same `dst`+`src` does NOT
/// fire drift detection today. The existing isolation.json
/// record + materialized worktree become stranded silently;
/// they're only reclaimed by `jackin purge`. Pinning this here
/// so a future change that extends the drift predicate
/// (proposed in code review of PR #177) updates this test in
/// the same change instead of accidentally regressing on it.
#[tokio::test]
async fn detect_drift_does_not_currently_flag_isolation_mode_flips() {
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jk-a1b2c3d4-jackin");
    std::fs::create_dir_all(&cdir).unwrap();
    write_records(
        &cdir,
        std::slice::from_ref(&record_for(
            "jackin",
            "jk-a1b2c3d4-jackin",
            "/workspace/jackin",
            "/same/src",
        )),
    )
    .unwrap();

    let paths = paths_for(data.path());
    // Same src+dst as the recorded mount, but isolation flipped.
    let edited = vec![mount(
        "/same/src",
        "/workspace/jackin",
        MountIsolation::Shared,
    )];
    let docker = FakeDockerClient::default();
    let det = detect_workspace_edit_drift(&paths, &wn("jackin"), &edited, &docker)
        .await
        .unwrap();
    // Current behavior — known gap. If this test starts failing
    // because drift now correctly flags the flip, update it to
    // assert `det.stopped_records.len() == 1` and remove this
    // explanatory note.
    assert!(
        det.stopped_records.is_empty(),
        "current V1 behavior: isolation-mode flips don't fire drift; \
             update this test when the predicate is extended"
    );
}

/// Operator removes the mount entirely from the workspace edit
/// (or renames its dst). The existing record's dst is no longer
/// in `edited_mounts`, so drift fires — operator must
/// acknowledge with `--delete-isolated-state`.
#[tokio::test]
async fn detect_drift_flags_record_when_dst_removed_from_edit() {
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jk-a1b2c3d4-jackin");
    std::fs::create_dir_all(&cdir).unwrap();
    write_records(
        &cdir,
        std::slice::from_ref(&record_for(
            "jackin",
            "jk-a1b2c3d4-jackin",
            "/workspace/jackin",
            "/old/src",
        )),
    )
    .unwrap();

    let paths = paths_for(data.path());
    // Edited mount list omits /workspace/jackin entirely.
    let edited = vec![mount(
        "/some/other/src",
        "/workspace/other",
        MountIsolation::Shared,
    )];
    let docker = FakeDockerClient::default();
    let det = detect_workspace_edit_drift(&paths, &wn("jackin"), &edited, &docker)
        .await
        .unwrap();
    assert!(det.running_containers.is_empty());
    assert_eq!(
        det.stopped_records.len(),
        1,
        "removing the dst from the workspace must surface the existing record as drift",
    );
    assert_eq!(det.stopped_records[0].mount_dst, "/workspace/jackin");
}
