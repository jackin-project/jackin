//! Tests for `workspaces`.
use super::*;
use crate::workspace::MountConfig;
use tempfile::tempdir;

#[test]
fn edit_workspace_leaves_original_value_when_validation_fails() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
        workdir: "/workspace/project".to_string(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/project".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        allowed_roles: vec!["agent-smith".to_string()],
        default_role: Some("agent-smith".to_string()),
        default_agent: None,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: crate::workspace::KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        github: None,
        git_pull_on_entry: false,
    };
    config
        .create_workspace("big-monorepo", original.clone())
        .unwrap();

    let err = config
        .edit_workspace(
            "big-monorepo",
            WorkspaceEdit {
                workdir: Some("/workspace/elsewhere".to_string()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_toggles_keep_awake_when_set() {
    // Round-trip: enable, disable, no-change. The Option<bool> shape
    // distinguishes "user touched the field" from "user said nothing
    // about it", which is the whole point of the field type.
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    config
        .create_workspace(
            "my-app",
            WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace/proj".to_string(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/proj".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);

    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                keep_awake_enabled: Some(true),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(config.workspaces.get("my-app").unwrap().keep_awake.enabled);

    // Subsequent edit with no keep_awake change must leave the
    // field alone — this is the contract that lets `workspace edit
    // --workdir` not silently flip power-management state.
    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                workdir: Some("/workspace/proj".to_string()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(
        config.workspaces.get("my-app").unwrap().keep_awake.enabled,
        "unrelated edits must not flip keep_awake",
    );

    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                keep_awake_enabled: Some(false),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);
}

#[test]
fn edit_workspace_sets_and_clears_agent() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    config
        .create_workspace(
            "my-app",
            WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace/proj".to_string(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/proj".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                default_agent: Some(Some(crate::agent::Agent::Codex)),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        config.workspaces.get("my-app").unwrap().default_agent,
        Some(crate::agent::Agent::Codex)
    );

    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                workdir: Some("/workspace/proj".to_string()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(
        config.workspaces.get("my-app").unwrap().default_agent,
        Some(crate::agent::Agent::Codex),
        "unrelated edits must not clear default_agent"
    );

    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                default_agent: Some(None),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();
    assert_eq!(config.workspaces.get("my-app").unwrap().default_agent, None);
}

#[test]
fn create_workspace_rejects_duplicate_name_and_preserves_existing_value() {
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
        workdir: "/workspace/project".to_string(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/project".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace("big-monorepo", original.clone())
        .unwrap();

    let err = config
        .create_workspace(
            "big-monorepo",
            WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace/other".to_string(),
                mounts: vec![MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace/other".to_string(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: Some("agent-smith".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("already exists"));
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_rejects_duplicate_upsert_destinations() {
    let temp = tempdir().unwrap();
    let original_src = temp.path().join("project");
    let first_upsert = temp.path().join("cache-a");
    let second_upsert = temp.path().join("cache-b");
    std::fs::create_dir_all(&original_src).unwrap();
    std::fs::create_dir_all(&first_upsert).unwrap();
    std::fs::create_dir_all(&second_upsert).unwrap();

    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
        workdir: "/workspace/project".to_string(),
        mounts: vec![MountConfig {
            src: original_src.display().to_string(),
            dst: "/workspace/project".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace("big-monorepo", original.clone())
        .unwrap();

    let err = config
        .edit_workspace(
            "big-monorepo",
            WorkspaceEdit {
                upsert_mounts: vec![
                    MountConfig {
                        src: first_upsert.display().to_string(),
                        dst: "/workspace/cache".to_string(),
                        readonly: false,
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: second_upsert.display().to_string(),
                        dst: "/workspace/cache".to_string(),
                        readonly: true,
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                ],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("duplicate workspace edit mount destination")
    );
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn edit_workspace_rejects_missing_remove_destination() {
    let temp = tempdir().unwrap();
    let original_src = temp.path().join("project");
    std::fs::create_dir_all(&original_src).unwrap();

    let mut config = AppConfig::default();
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
        workdir: "/workspace/project".to_string(),
        mounts: vec![MountConfig {
            src: original_src.display().to_string(),
            dst: "/workspace/project".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    config
        .create_workspace("big-monorepo", original.clone())
        .unwrap();

    let err = config
        .edit_workspace(
            "big-monorepo",
            WorkspaceEdit {
                remove_destinations: vec!["/workspace/missing".to_string()],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("unknown workspace mount destination")
    );
    assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
}

#[test]
fn remove_workspace_errors_when_missing() {
    let mut config = AppConfig::default();

    let err = config.remove_workspace("missing").unwrap_err();

    assert!(err.to_string().contains("unknown workspace missing"));
}

mod drift_detection {
    use super::super::*;
    use crate::docker_client::{ContainerRow, FakeDockerClient};
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, IsolationRecord, write_records};
    use crate::paths::JackinPaths;
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

    fn mount(src: &str, dst: &str, iso: MountIsolation) -> crate::workspace::MountConfig {
        crate::workspace::MountConfig {
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
            list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                vec![ContainerRow {
                    name: "jk-a1b2c3d4-jackin".to_string(),
                    labels: std::collections::HashMap::default(),
                }],
            ])),
            ..Default::default()
        };
        let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &docker)
            .await
            .unwrap();
        assert_eq!(
            det.running_containers,
            vec!["jk-a1b2c3d4-jackin".to_string()]
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
        let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &docker)
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
        let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &docker)
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
        let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &docker)
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
        let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &docker)
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
}
