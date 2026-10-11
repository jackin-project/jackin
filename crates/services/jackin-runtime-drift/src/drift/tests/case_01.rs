// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
                id: "container-id".to_owned(),
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

#[tokio::test]
async fn drift_selects_saved_stem_and_excludes_label_and_ad_hoc_records() {
    let data = TempDir::new().unwrap();
    let stem_record = record_for(
        "saved-stem",
        "jk-a1b2c3d4-stem",
        "/workspace/repo",
        "/old/src",
    );
    let label_record = record_for(
        "display-label",
        "jk-b1b2c3d4-label",
        "/workspace/repo",
        "/old/src",
    );
    let mut ad_hoc = record_for(
        "saved-stem",
        "jk-c1b2c3d4-adhoc",
        "/workspace/repo",
        "/old/src",
    );
    ad_hoc.workspace_name = None;
    for record in [&stem_record, &label_record, &ad_hoc] {
        write_records(
            &data.path().join(&record.container_name),
            std::slice::from_ref(record),
        )
        .unwrap();
    }
    let detected = detect_workspace_edit_drift(
        &paths_for(data.path()),
        &wn("saved-stem"),
        &[mount(
            "/new/src",
            "/workspace/repo",
            MountIsolation::Worktree,
        )],
        &FakeDockerClient::default(),
    )
    .await
    .unwrap();
    assert_eq!(detected.stopped_records, vec![stem_record]);
    assert!(detected.running_containers.is_empty());
}
