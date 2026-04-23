use jackin::config;
use jackin::workspace::{self, WorkspaceConfig, WorkspaceEdit, parse_mount_spec_resolved};

/// Simulates `jackin workspace create jackin --workdir jackin --mount sibling-project`
/// from a parent directory. Both relative workdir and mount must be resolved to
/// absolute paths so that `create_workspace` validation passes.
#[test]
fn workspace_create_resolves_relative_workdir_and_mounts() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("jackin");
    let mount_dir = temp.path().join("sibling-project");
    std::fs::create_dir_all(&workdir_dir).unwrap();
    std::fs::create_dir_all(&mount_dir).unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    let expanded_workdir = workspace::resolve_path("jackin");
    let mount = parse_mount_spec_resolved("sibling-project").unwrap();

    let mut config = config::AppConfig::default();
    let result = config.create_workspace(
        "jackin",
        WorkspaceConfig {
            workdir: expanded_workdir.clone(),
            mounts: vec![
                workspace::MountConfig {
                    src: expanded_workdir.clone(),
                    dst: expanded_workdir.clone(),
                    readonly: false,
                },
                mount,
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        },
    );

    std::env::set_current_dir(original_cwd).unwrap();

    result.unwrap();
    let ws = config.workspaces.get("jackin").unwrap();
    assert!(ws.workdir.starts_with('/'));
    assert!(!ws.workdir.contains(".."));
    assert!(ws.mounts.iter().all(|m| m.src.starts_with('/')));
}

/// Simulates `jackin workspace create jackin --workdir . --mount ../jackin-agent-smith`
/// from inside the project directory. Dot-workdir and parent-relative mount must both
/// resolve to clean absolute paths.
#[test]
fn workspace_create_resolves_dot_workdir_and_dotdot_mount() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("jackin");
    let sibling_dir = temp.path().join("jackin-agent-smith");
    std::fs::create_dir_all(&workdir_dir).unwrap();
    std::fs::create_dir_all(&sibling_dir).unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&workdir_dir).unwrap();

    let expanded_workdir = workspace::resolve_path(".");
    let mount = parse_mount_spec_resolved("../jackin-agent-smith").unwrap();

    let mut config = config::AppConfig::default();
    let result = config.create_workspace(
        "jackin",
        WorkspaceConfig {
            workdir: expanded_workdir.clone(),
            mounts: vec![
                workspace::MountConfig {
                    src: expanded_workdir.clone(),
                    dst: expanded_workdir.clone(),
                    readonly: false,
                },
                mount.clone(),
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        },
    );

    std::env::set_current_dir(original_cwd).unwrap();

    result.unwrap();
    let ws = config.workspaces.get("jackin").unwrap();
    assert!(ws.workdir.starts_with('/'));
    assert!(!ws.workdir.contains(".."));
    assert!(!mount.src.contains(".."), "mount src must not contain '..'");
    assert!(mount.src.ends_with("/jackin-agent-smith"));
}

/// Simulates `jackin workspace create my-app --workdir /tmp/app` (default behavior).
/// The workdir must be auto-mounted as the first mount.
#[test]
fn workspace_create_auto_mounts_workdir_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("my-app");
    std::fs::create_dir_all(&workdir_dir).unwrap();

    let expanded_workdir = workdir_dir.display().to_string();

    // Simulate default behavior: no_workdir_mount = false
    let no_workdir_mount = false;
    let mut all_mounts: Vec<workspace::MountConfig> = vec![];
    if !no_workdir_mount {
        let already_mounted = all_mounts.iter().any(|m| m.dst == expanded_workdir);
        if !already_mounted {
            all_mounts.insert(
                0,
                workspace::MountConfig {
                    src: expanded_workdir.clone(),
                    dst: expanded_workdir.clone(),
                    readonly: false,
                },
            );
        }
    }

    let mut config = config::AppConfig::default();
    config
        .create_workspace(
            "my-app",
            WorkspaceConfig {
                workdir: expanded_workdir.clone(),
                mounts: all_mounts,
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    let ws = config.workspaces.get("my-app").unwrap();
    assert_eq!(ws.mounts.len(), 1);
    assert_eq!(ws.mounts[0].src, expanded_workdir);
    assert_eq!(ws.mounts[0].dst, expanded_workdir);
    assert!(!ws.mounts[0].readonly);
}

/// Simulates `jackin workspace create monorepo --workdir /workspace --no-workdir-mount
/// --mount /tmp/src:/workspace`. The workdir must NOT be auto-mounted.
#[test]
fn workspace_create_no_workdir_mount_skips_auto_mount() {
    let temp = tempfile::tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let src_path = src_dir.display().to_string();

    // Simulate --no-workdir-mount with explicit mount
    let no_workdir_mount = true;
    let mut all_mounts = vec![workspace::MountConfig {
        src: src_path.clone(),
        dst: "/workspace".to_string(),
        readonly: false,
    }];
    if !no_workdir_mount {
        // This block should NOT execute
        all_mounts.insert(
            0,
            workspace::MountConfig {
                src: "/workspace".to_string(),
                dst: "/workspace".to_string(),
                readonly: false,
            },
        );
    }

    let mut config = config::AppConfig::default();
    config
        .create_workspace(
            "monorepo",
            WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: all_mounts,
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    let ws = config.workspaces.get("monorepo").unwrap();
    assert_eq!(ws.mounts.len(), 1, "should only have the explicit mount");
    assert_eq!(ws.mounts[0].src, src_path);
    assert_eq!(ws.mounts[0].dst, "/workspace");
}

/// When the workdir is already covered by an explicit --mount, the auto-mount
/// should be skipped even without --no-workdir-mount.
#[test]
fn workspace_create_skips_auto_mount_when_workdir_already_mounted() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("project");
    std::fs::create_dir_all(&workdir_dir).unwrap();

    let expanded_workdir = workdir_dir.display().to_string();

    // Simulate: user explicitly mounts workdir via --mount
    let no_workdir_mount = false;
    let mut all_mounts = vec![workspace::MountConfig {
        src: expanded_workdir.clone(),
        dst: expanded_workdir.clone(),
        readonly: true, // user chose read-only
    }];
    if !no_workdir_mount {
        let already_mounted = all_mounts.iter().any(|m| m.dst == expanded_workdir);
        if !already_mounted {
            all_mounts.insert(
                0,
                workspace::MountConfig {
                    src: expanded_workdir.clone(),
                    dst: expanded_workdir.clone(),
                    readonly: false,
                },
            );
        }
    }

    let mut config = config::AppConfig::default();
    config
        .create_workspace(
            "project",
            WorkspaceConfig {
                workdir: expanded_workdir.clone(),
                mounts: all_mounts,
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    let ws = config.workspaces.get("project").unwrap();
    assert_eq!(ws.mounts.len(), 1, "no duplicate mount should be added");
    assert!(ws.mounts[0].readonly, "original mount properties preserved");
}

/// Simulates `jackin workspace edit jackin --mount sibling-dev` where the mount
/// is a relative directory name. The resolved mount must pass validation.
#[test]
fn workspace_edit_resolves_relative_mount() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("jackin");
    let dev_dir = temp.path().join("jackin-dev");
    std::fs::create_dir_all(&workdir_dir).unwrap();
    std::fs::create_dir_all(&dev_dir).unwrap();

    let workdir_abs = workdir_dir.display().to_string();

    let mut config = config::AppConfig::default();
    config
        .create_workspace(
            "jackin",
            WorkspaceConfig {
                workdir: workdir_abs.clone(),
                mounts: vec![workspace::MountConfig {
                    src: workdir_abs.clone(),
                    dst: workdir_abs.clone(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    let mount = parse_mount_spec_resolved("jackin-dev").unwrap();

    let result = config.edit_workspace(
        "jackin",
        WorkspaceEdit {
            upsert_mounts: vec![mount.clone()],
            ..WorkspaceEdit::default()
        },
    );

    std::env::set_current_dir(original_cwd).unwrap();

    result.unwrap();
    let ws = config.workspaces.get("jackin").unwrap();
    assert_eq!(ws.mounts.len(), 2);
    assert!(ws.mounts[1].src.starts_with('/'));
    assert!(ws.mounts[1].src.ends_with("/jackin-dev"));
}

/// Simulates `jackin workspace edit my-app --no-workdir-mount` to remove the
/// auto-mounted workdir after workspace creation.
#[test]
fn workspace_edit_no_workdir_mount_removes_auto_mount() {
    let temp = tempfile::tempdir().unwrap();
    let workdir_dir = temp.path().join("my-app");
    let extra_dir = temp.path().join("extra");
    std::fs::create_dir_all(&workdir_dir).unwrap();
    std::fs::create_dir_all(&extra_dir).unwrap();

    let workdir_abs = workdir_dir.display().to_string();
    let extra_abs = extra_dir.display().to_string();

    let mut config = config::AppConfig::default();
    // Create workspace with auto-mounted workdir + an extra mount
    config
        .create_workspace(
            "my-app",
            WorkspaceConfig {
                workdir: workdir_abs.clone(),
                mounts: vec![
                    workspace::MountConfig {
                        src: workdir_abs.clone(),
                        dst: workdir_abs.clone(),
                        readonly: false,
                    },
                    workspace::MountConfig {
                        src: extra_abs.clone(),
                        dst: workdir_abs.clone() + "/extra",
                        readonly: false,
                    },
                ],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    assert_eq!(config.workspaces.get("my-app").unwrap().mounts.len(), 2);

    // Now remove the workdir auto-mount
    config
        .edit_workspace(
            "my-app",
            WorkspaceEdit {
                no_workdir_mount: true,
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();

    let ws = config.workspaces.get("my-app").unwrap();
    assert_eq!(ws.mounts.len(), 1, "auto-mount should be removed");
    assert_eq!(ws.mounts[0].dst, workdir_abs.clone() + "/extra");
}

/// `--no-workdir-mount` on edit should fail if there is no auto-mounted workdir.
#[test]
fn workspace_edit_no_workdir_mount_fails_when_no_auto_mount() {
    let temp = tempfile::tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let src_abs = src_dir.display().to_string();

    let mut config = config::AppConfig::default();
    // Create workspace that was originally made with --no-workdir-mount
    config
        .create_workspace(
            "monorepo",
            WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: src_abs,
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        )
        .unwrap();

    let err = config
        .edit_workspace(
            "monorepo",
            WorkspaceEdit {
                no_workdir_mount: true,
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(
        err.to_string().contains("no auto-mounted workdir found"),
        "expected clear error, got: {err}"
    );
}
