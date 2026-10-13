// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn summary_counts_mounts_and_readonly() {
    let ws = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/a".into(),
        mounts: vec![
            MountConfig {
                src: "/s1".into(),
                dst: "/a".into(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
            MountConfig {
                src: "/s2".into(),
                dst: "/b".into(),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
        ],
        allowed_roles: vec!["agent-smith".into()],
        ..Default::default()
    };
    let sum = WorkspaceSummary::from_source("big-monorepo", &ws);
    assert_eq!(sum.name, "big-monorepo");
    assert_eq!(sum.mount_count, 2);
    assert_eq!(sum.readonly_mount_count, 1);
    assert_eq!(sum.allowed_role_count, 1);
}

#[test]
fn manager_from_config_lists_all_workspaces() {
    let mut config = AppConfig::default();
    config.workspaces.insert("a".into(), empty_ws("/a"));
    // cwd is unrelated to /a — landing row is the synthetic
    // "Current directory" at index 0.
    let tmp = tempfile::tempdir().unwrap();
    let state = ManagerState::from_config(&config, tmp.path());
    assert_eq!(state.workspaces.len(), 1);
    assert!(matches!(state.stage, ManagerStage::List));
    assert_eq!(state.selected, 0);
}

#[test]
fn refresh_instances_loads_rebuildable_index() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-demo-alpha",
        workspace_name: Some("demo"),
        workspace_label: "demo",
        workdir: "/workspace/demo",
        host_workdir_fingerprint: "sha256:test",
        role_key: "alpha",
        role_display_name: "Alpha",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/alpha.git",
        role_source_ref: None,
        image_tag: "jk_alpha",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-demo-alpha".into(),
            dind_container: Some("jk-k7p9m2xq-demo-alpha-dind".into()),
            network: "jk-k7p9m2xq-demo-alpha-net".into(),
            certs_volume: Some("jk-k7p9m2xq-demo-alpha-dind-certs".into()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(InstanceStatus::RestoreAvailable);
    manifest
        .write(&paths.data_dir.join("jk-k7p9m2xq-demo-alpha"))
        .unwrap();

    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    refresh_instances(&mut state, &paths);

    assert_eq!(state.instances.len(), 1);
    assert_eq!(state.instances[0].instance_id, "k7p9m2xq");
    assert_eq!(state.instances[0].status, InstanceStatus::RestoreAvailable);
}

#[test]
fn live_running_overlay_makes_restore_available_instance_visible() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-demo-alpha",
        workspace_name: Some("demo"),
        workspace_label: "demo",
        workdir: "/workspace/demo",
        host_workdir_fingerprint: "sha256:test",
        role_key: "alpha",
        role_display_name: "Alpha",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/alpha.git",
        role_source_ref: None,
        image_tag: "jk_alpha",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-demo-alpha".into(),
            dind_container: Some("jk-k7p9m2xq-demo-alpha-dind".into()),
            network: "jk-k7p9m2xq-demo-alpha-net".into(),
            certs_volume: Some("jk-k7p9m2xq-demo-alpha-dind-certs".into()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(InstanceStatus::RestoreAvailable);
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

    let mut instances = InstanceIndex::read(&paths.data_dir).unwrap().instances;
    overlay_running_instances(
        &paths,
        &mut instances,
        &["jk-k7p9m2xq-demo-alpha".to_owned()],
    );

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].status, InstanceStatus::Running);
}

#[test]
fn live_running_overlay_backfills_manifest_missing_from_index() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-demo-alpha",
        workspace_name: Some("demo"),
        workspace_label: "demo",
        workdir: "/workspace/demo",
        host_workdir_fingerprint: "sha256:test",
        role_key: "alpha",
        role_display_name: "Alpha",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/alpha.git",
        role_source_ref: None,
        image_tag: "jk_alpha",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-demo-alpha".into(),
            dind_container: Some("jk-k7p9m2xq-demo-alpha-dind".into()),
            network: "jk-k7p9m2xq-demo-alpha-net".into(),
            certs_volume: Some("jk-k7p9m2xq-demo-alpha-dind-certs".into()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(InstanceStatus::RestoreAvailable);
    manifest
        .write(&paths.data_dir.join("jk-k7p9m2xq-demo-alpha"))
        .unwrap();
    let mut instances = Vec::new();

    overlay_running_instances(
        &paths,
        &mut instances,
        &["jk-k7p9m2xq-demo-alpha".to_owned()],
    );

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].container_base, "jk-k7p9m2xq-demo-alpha");
    assert_eq!(instances[0].status, InstanceStatus::Running);
}

#[test]
fn refresh_instances_throttles_within_interval() {
    // 20 Hz render loop must not reparse instances.json on every
    // tick. After the first refresh, a follow-up call inside the
    // throttle window keeps the cached `instances` snapshot even
    // when the on-disk index changes; `force_refresh_instances_for_test`
    // bypasses the gate.
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-demo-alpha",
        workspace_name: Some("demo"),
        workspace_label: "demo",
        workdir: "/workspace/demo",
        host_workdir_fingerprint: "sha256:test",
        role_key: "alpha",
        role_display_name: "Alpha",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/alpha.git",
        role_source_ref: None,
        image_tag: "jk_alpha",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-demo-alpha".into(),
            dind_container: Some("jk-k7p9m2xq-demo-alpha-dind".into()),
            network: "jk-k7p9m2xq-demo-alpha-net".into(),
            certs_volume: Some("jk-k7p9m2xq-demo-alpha-dind-certs".into()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(InstanceStatus::Active);
    manifest
        .write(&paths.data_dir.join("jk-k7p9m2xq-demo-alpha"))
        .unwrap();

    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    refresh_instances(&mut state, &paths);
    assert_eq!(state.instances.len(), 1);
    assert_eq!(state.instances[0].status, InstanceStatus::Active);

    // Mutate the manifest on disk; without the bypass, an
    // immediate refresh must observe the cached value.
    manifest.mark_status(InstanceStatus::Crashed);
    manifest
        .write(&paths.data_dir.join("jackin-demo-alpha-k7p9m2xq"))
        .unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

    state.instances_last_refresh = Some(std::time::Instant::now());
    refresh_instances(&mut state, &paths);
    assert_eq!(
        state.instances[0].status,
        InstanceStatus::Active,
        "throttle window must keep the cached snapshot",
    );

    // Bypass the throttle — disk state is now observable.
    state.force_refresh_instances_for_test();
    refresh_instances(&mut state, &paths);
    assert_eq!(state.instances[0].status, InstanceStatus::Crashed,);
}

#[test]
fn refresh_instances_clears_on_index_error() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::write(paths.data_dir.join("instances.json"), b"not json").unwrap();
    let bogus = paths.data_dir.join("jackin-bogus-k7p9m2xq");
    std::fs::create_dir_all(bogus.join(".jackin")).unwrap();
    std::fs::write(bogus.join(".jackin/instance.json"), b"not json").unwrap();

    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    refresh_instances(&mut state, &paths);

    assert!(state.instances.is_empty());
}

#[test]
fn manager_preselects_saved_workspace_matching_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().canonicalize().unwrap();
    let workdir = project.display().to_string();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "big-monorepo".into(),
        WorkspaceConfig {
            version: CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: workdir.clone(),
            mounts: vec![MountConfig {
                src: workdir.clone(),
                dst: workdir,
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    // Second workspace that does NOT match cwd — used to verify the
    // preselect calculation points at the matching one, not simply
    // "index 1" which works for a single workspace by accident.
    config
        .workspaces
        .insert("z-unrelated".into(), empty_ws("/some/other/path"));

    let state = ManagerState::from_config(&config, &project);
    // Workspaces are ordered by BTreeMap key: ["big-monorepo", "z-unrelated"].
    // "big-monorepo" is at saved_index 0, so selected = 1 + 0 = 1.
    assert_eq!(state.selected, 1);
    assert_eq!(state.workspaces[state.selected - 1].name, "big-monorepo");
}

#[test]
fn manager_current_directory_is_first_row() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();

    // Empty config: only the synthetic "Current directory" + sentinel.
    let config_empty = AppConfig::default();
    let state_empty = ManagerState::from_config(&config_empty, &cwd);
    assert_eq!(state_empty.selected, 0);
    assert_eq!(state_empty.workspaces.len(), 0);

    // Non-empty config with unrelated saved workspaces — preselect
    // still lands on row 0.
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("a".into(), empty_ws("/some/other/path"));
    config
        .workspaces
        .insert("b".into(), empty_ws("/yet/another"));
    let state = ManagerState::from_config(&config, &cwd);
    assert_eq!(
        state.selected, 0,
        "selected==0 must always map to Current directory"
    );
    assert_eq!(state.workspaces.len(), 2);
}

#[test]
fn manager_preselects_current_directory_when_no_saved_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();

    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("unrelated".into(), empty_ws("/some/other/path"));

    let state = ManagerState::from_config(&config, &cwd);
    assert_eq!(
        state.selected, 0,
        "no saved workspace covers cwd → land on Current directory"
    );
}

#[test]
fn new_edit_is_not_dirty() {
    let e = EditorState::new_edit("a".into(), empty_ws("/a"));
    assert!(!e.is_dirty());
    assert_eq!(e.change_count(), 0);
}

#[test]
fn changing_workdir_is_dirty_count_one() {
    let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
    e.pending.workdir = "/b".into();
    assert!(e.is_dirty());
    assert_eq!(e.change_count(), 1);
}

#[test]
fn adding_mount_counts_as_one_change() {
    let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
    e.pending.mounts.push(MountConfig {
        src: "/s".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    });
    assert_eq!(e.change_count(), 1);
}

#[test]
fn isolation_only_change_counts_as_one() {
    let mut ws = empty_ws("/workspace/jackin");
    ws.mounts.push(MountConfig {
        src: "/host/jackin".into(),
        dst: "/workspace/jackin".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    });
    let mut e = EditorState::new_edit("jackin".into(), ws);
    assert_eq!(e.change_count(), 0);
    // Cycle from Shared to Worktree on the only mount row.
    e.active_field = FieldFocus::Row(0);
    e.cycle_isolation_for_selected_mount();
    assert_eq!(e.change_count(), 1);
}

#[test]
fn classify_mount_diffs_distinguishes_modified_from_remove_add() {
    let original = vec![MountConfig {
        src: "/host/jackin".into(),
        dst: "/workspace/jackin".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }];
    let mut pending = original.clone();
    pending[0].isolation = jackin_config::MountIsolation::Worktree;

    let diffs = classify_mount_diffs(&original, &pending);
    assert_eq!(diffs.len(), 1, "same-dst diff is one row, not two");
    assert!(
        matches!(diffs[0], MountDiff::Modified { .. }),
        "got {:?}",
        diffs[0]
    );
}
