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
async fn detect_drift_flags_all_mode_transitions_for_running_and_stopped_records() {
    let transitions = [
        (MountIsolation::Shared, MountIsolation::Worktree),
        (MountIsolation::Shared, MountIsolation::Clone),
        (MountIsolation::Worktree, MountIsolation::Shared),
        (MountIsolation::Worktree, MountIsolation::Clone),
        (MountIsolation::Clone, MountIsolation::Shared),
        (MountIsolation::Clone, MountIsolation::Worktree),
    ];
    for (recorded_mode, edited_mode) in transitions {
        for is_running in [false, true] {
            let data = TempDir::new().unwrap();
            let container_name = "jk-a1b2c3d4-jackin";
            let mut record = record_for("jackin", container_name, "/workspace/jackin", "/same/src");
            record.isolation = recorded_mode;
            write_records(
                &data.path().join(container_name),
                std::slice::from_ref(&record),
            )
            .unwrap();

            let edited = vec![mount("/same/src", "/workspace/jackin", edited_mode)];
            let docker = if is_running {
                docker_with_running_container(container_name)
            } else {
                FakeDockerClient::default()
            };
            let detected = detect_workspace_edit_drift(
                &paths_for(data.path()),
                &wn("jackin"),
                &edited,
                &docker,
            )
            .await
            .unwrap();

            if is_running {
                assert_eq!(detected.running_containers, vec![container_name.to_owned()]);
                assert!(detected.stopped_records.is_empty());
            } else {
                assert!(detected.running_containers.is_empty());
                assert_eq!(detected.stopped_records, vec![record]);
            }
        }
    }
}

#[tokio::test]
async fn detect_drift_is_quiet_for_unchanged_modes_readonly_changes_and_missing_records() {
    for mode in [
        MountIsolation::Shared,
        MountIsolation::Worktree,
        MountIsolation::Clone,
    ] {
        let data = TempDir::new().unwrap();
        let container_name = "jk-a1b2c3d4-jackin";
        let mut record = record_for("jackin", container_name, "/workspace/jackin", "/same/src");
        record.isolation = mode;
        write_records(
            &data.path().join(container_name),
            std::slice::from_ref(&record),
        )
        .unwrap();

        let edited = vec![mount("/same/src", "/workspace/jackin", mode)];
        let detected = detect_workspace_edit_drift(
            &paths_for(data.path()),
            &wn("jackin"),
            &edited,
            &FakeDockerClient::default(),
        )
        .await
        .unwrap();
        assert!(detected.running_containers.is_empty());
        assert!(detected.stopped_records.is_empty());
    }

    let data = TempDir::new().unwrap();
    let mut readonly_only = mount("/same/src", "/workspace/jackin", MountIsolation::Worktree);
    readonly_only.readonly = true;
    let mut record = record_for(
        "jackin",
        "jk-a1b2c3d4-jackin",
        "/workspace/jackin",
        "/same/src",
    );
    record.isolation = MountIsolation::Worktree;
    write_records(
        &data.path().join(&record.container_name),
        std::slice::from_ref(&record),
    )
    .unwrap();
    let detected = detect_workspace_edit_drift(
        &paths_for(data.path()),
        &wn("jackin"),
        &[readonly_only],
        &FakeDockerClient::default(),
    )
    .await
    .unwrap();
    assert!(detected.stopped_records.is_empty());

    let no_record_data = TempDir::new().unwrap();
    let no_record = detect_workspace_edit_drift(
        &paths_for(no_record_data.path()),
        &wn("jackin"),
        &[mount(
            "/same/src",
            "/workspace/jackin",
            MountIsolation::Worktree,
        )],
        &FakeDockerClient::default(),
    )
    .await
    .unwrap();
    assert!(no_record.running_containers.is_empty());
    assert!(no_record.stopped_records.is_empty());
}

#[tokio::test]
async fn detect_drift_does_not_change_saved_state_or_dirty_paths() {
    for is_running in [false, true] {
        let data = TempDir::new().unwrap();
        let container_name = "jk-a1b2c3d4-jackin";
        let container_dir = data.path().join(container_name);
        let preserved_path = data.path().join("preserved-worktree");
        std::fs::create_dir_all(&preserved_path).unwrap();
        let sentinel = preserved_path.join("dirty-sentinel");
        std::fs::write(&sentinel, b"uncommitted user data").unwrap();

        let mut record = record_for("jackin", container_name, "/workspace/jackin", "/same/src");
        record.worktree_path = preserved_path.to_string_lossy().into_owned();
        write_records(&container_dir, std::slice::from_ref(&record)).unwrap();
        let record_path =
            jackin_runtime_isolation::isolation::state::isolation_file_path(&container_dir);
        let record_before = std::fs::read(&record_path).unwrap();
        let sentinel_before = std::fs::read(&sentinel).unwrap();
        let children_before: Vec<_> = std::fs::read_dir(&preserved_path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();

        let docker = if is_running {
            docker_with_running_container(container_name)
        } else {
            FakeDockerClient::default()
        };
        let detected = detect_workspace_edit_drift(
            &paths_for(data.path()),
            &wn("jackin"),
            &[mount(
                "/same/src",
                "/workspace/jackin",
                MountIsolation::Clone,
            )],
            &docker,
        )
        .await
        .unwrap();
        if is_running {
            assert_eq!(detected.running_containers, vec![container_name.to_owned()]);
            assert!(detected.stopped_records.is_empty());
        } else {
            assert!(detected.running_containers.is_empty());
            assert_eq!(detected.stopped_records, vec![record]);
        }
        let children_after: Vec<_> = std::fs::read_dir(&preserved_path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(std::fs::read(record_path).unwrap(), record_before);
        assert_eq!(std::fs::read(sentinel).unwrap(), sentinel_before);
        assert!(preserved_path.is_dir());
        assert_eq!(children_after, children_before);
    }
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
    for (edited_src, edited_mode) in [
        ("/new/src", MountIsolation::Worktree),
        ("/old/src", MountIsolation::Clone),
    ] {
        let detected = detect_workspace_edit_drift(
            &paths_for(data.path()),
            &wn("saved-stem"),
            &[mount(edited_src, "/workspace/repo", edited_mode)],
            &FakeDockerClient::default(),
        )
        .await
        .unwrap();
        assert_eq!(detected.stopped_records, vec![stem_record.clone()]);
        assert!(detected.running_containers.is_empty());
    }
}

fn docker_with_running_container(container_name: &str) -> FakeDockerClient {
    FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            ContainerRow {
                name: container_name.to_owned(),
                id: "container-id".to_owned(),
                labels: std::collections::HashMap::default(),
            },
        ]])),
        ..Default::default()
    }
}
