//! Tests for `state`.
use super::*;
use crate::console::services::instances::load_instance_refresh_snapshot;
use crate::console::services::instances::overlay_running_instances;
use crate::console::tui::state::SettingsState;
use jackin_config::{CURRENT_WORKSPACE_VERSION, KeepAwakeConfig, MountConfig, WorkspaceConfig};
use jackin_console::mount_diff::{MountDiff, classify_mount_diffs};
use jackin_core::{Agent, JackinPaths};
use jackin_runtime::instance::{
    DockerResources, InstanceIndex, InstanceManifest, InstanceStatus, NewInstanceManifest,
};
use std::path::PathBuf;

fn refresh_instances(state: &mut ManagerState<'_>, paths: &JackinPaths) {
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    let now = std::time::Instant::now();
    if let Some(last) = state.instances_last_refresh
        && now.duration_since(last) < REFRESH_INTERVAL
    {
        return;
    }
    state.instances_last_refresh = Some(now);
    match load_instance_refresh_snapshot(paths) {
        Ok(snapshot) => state.apply_instance_refresh_snapshot(snapshot),
        Err(error) => state.apply_instance_refresh_error(&error),
    }
}

fn empty_ws(workdir: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.into(),
        ..Default::default()
    }
}

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

/// Pins that `ms.selected == 0` means "Current directory" regardless
/// of how many saved workspaces are present. The render path
/// (`render_list_body`) and the input path (`handle_list_key`) both
/// depend on this: selected==0 is the synthetic cwd row, 1..=N are
/// saved workspaces, N+1 is the "+ New workspace" sentinel.
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

/// Regression: cycling isolation on an existing mount (same `dst`,
/// same `src`) is one logical change. Pre-fix it counted as 2
/// because the structural-equality classifier treated the new
/// `MountConfig` as added and the old one as removed.
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

#[test]
fn classify_mount_diffs_keeps_genuine_remove_add_separate() {
    let original = vec![MountConfig {
        src: "/host/a".into(),
        dst: "/workspace/a".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }];
    let pending = vec![MountConfig {
        src: "/host/b".into(),
        dst: "/workspace/b".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }];
    let diffs = classify_mount_diffs(&original, &pending);
    assert_eq!(diffs.len(), 2);
    // Order: pending first (Added), then original (Removed).
    assert!(matches!(diffs[0], MountDiff::Added(_)));
    assert!(matches!(diffs[1], MountDiff::Removed(_)));
}

// ── change_count env-diff coverage (Secrets tab) ──

/// Setting a new workspace-level env key on `pending` (with
/// `original.env` empty) contributes exactly +1 to the change count.
#[test]
fn change_count_env_set_counts_as_one() {
    let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
    assert_eq!(e.change_count(), 0);
    e.pending
        .env
        .insert("DB_URL".into(), EnvValue::Plain("postgres://…".into()));
    assert_eq!(e.change_count(), 1);
}

/// Removing an existing workspace-level env key (seeded in
/// `original.env` at construction time) contributes exactly +1.
#[test]
fn change_count_env_remove_counts_as_one() {
    let mut ws = empty_ws("/a");
    ws.env
        .insert("DB_URL".into(), EnvValue::Plain("postgres://…".into()));
    let mut e = EditorState::new_edit("a".into(), ws);
    assert_eq!(e.change_count(), 0);
    e.pending.env.remove("DB_URL");
    assert_eq!(e.change_count(), 1);
}

/// Adding and removing per-role env override keys each contribute +1
/// via the same map-change helper as workspace-level env.
#[test]
fn change_count_agent_env_delta() {
    use jackin_config::WorkspaceRoleOverride;
    // Seed one role with one env key.
    let mut ws = empty_ws("/a");
    let mut role_x_env = std::collections::BTreeMap::new();
    role_x_env.insert("LOG_LEVEL".into(), EnvValue::Plain("info".into()));
    ws.roles.insert(
        "agent-x".into(),
        WorkspaceRoleOverride {
            env: role_x_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let mut e = EditorState::new_edit("a".into(), ws);
    assert_eq!(e.change_count(), 0);

    // Add a new key to pending.
    e.pending
        .roles
        .get_mut("agent-x")
        .unwrap()
        .env
        .insert("DEBUG".into(), EnvValue::Plain("1".into()));
    assert_eq!(e.change_count(), 1);

    // Remove the original key. Net delta: 2 (one add + one remove).
    e.pending
        .roles
        .get_mut("agent-x")
        .unwrap()
        .env
        .remove("LOG_LEVEL");
    assert_eq!(e.change_count(), 2);
}

/// Any env mutation (workspace-level or per-role) flips `is_dirty()`
/// to true because `pending != original` in the underlying
/// `WorkspaceConfig` `PartialEq`.
#[test]
fn is_dirty_from_env_mutation() {
    use jackin_config::WorkspaceRoleOverride;

    // Workspace env path.
    let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
    assert!(!e.is_dirty());
    e.pending
        .env
        .insert("K".into(), EnvValue::Plain("v".into()));
    assert!(e.is_dirty(), "workspace env set must make state dirty");

    // Per-role env path.
    let mut e2 = EditorState::new_edit("a".into(), empty_ws("/a"));
    assert!(!e2.is_dirty());
    e2.pending.roles.insert(
        "agent-x".into(),
        WorkspaceRoleOverride {
            env: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("K".into(), EnvValue::Plain("v".into()));
                m
            },
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    assert!(e2.is_dirty(), "role env set must make state dirty");
}

#[test]
fn create_prelude_starts_at_first_step() {
    let p = CreatePreludeState::new();
    assert!(matches!(p.step, CreateStep::PickFirstMountSrc));
}

// ── completed() helper — keeps name+ws invariants in lockstep ──

#[test]
fn completed_returns_none_when_name_missing() {
    let mut p = CreatePreludeState::new();
    p.accept_mount_src(PathBuf::from("/home/user/proj"));
    p.accept_mount_dst("/home/user/proj".into(), false);
    p.accept_workdir("/home/user/proj".into());
    // No accept_name → completed() must be None.
    assert!(p.completed().is_none());
}

#[test]
fn completed_returns_none_when_mount_src_missing() {
    let mut p = CreatePreludeState::new();
    // Skip accept_mount_src and accept_mount_dst.
    p.pending_workdir = Some("/home/user/proj".into());
    p.pending_name = Some("proj".into());
    // build_workspace fails on missing src → completed() None.
    assert!(p.completed().is_none());
}

#[test]
fn completed_returns_none_when_workdir_missing() {
    let mut p = CreatePreludeState::new();
    p.accept_mount_src(PathBuf::from("/home/user/proj"));
    p.accept_mount_dst("/home/user/proj".into(), false);
    // Skip accept_workdir.
    p.pending_name = Some("proj".into());
    assert!(p.completed().is_none());
}

#[test]
fn completed_returns_some_when_all_fields_present() {
    let mut p = CreatePreludeState::new();
    p.accept_mount_src(PathBuf::from("/home/user/proj"));
    p.accept_mount_dst("/home/user/proj".into(), false);
    p.accept_workdir("/home/user/proj".into());
    p.accept_name("proj".into());
    let (name, ws) = p.completed().expect("all fields present");
    assert_eq!(name, "proj");
    assert_eq!(ws.workdir, "/home/user/proj");
    assert_eq!(ws.mounts.len(), 1);
    assert_eq!(ws.mounts[0].src, "/home/user/proj");
}

/// Pin the enum contract: round-tripping a `ManagerListRow` through
/// `to_screen_index` / `row_at` / `selected_row` must yield the same
/// logical row. Covers the three variants over a non-trivial saved set.
#[test]
fn manager_list_row_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    config.workspaces.insert("a".into(), empty_ws("/a"));
    config.workspaces.insert("b".into(), empty_ws("/b"));
    config.workspaces.insert("c".into(), empty_ws("/c"));
    let mut state = ManagerState::from_config(&config, cwd);

    let saved_count = state.workspaces.len();
    assert_eq!(state.row_count(), saved_count + 2);
    assert_eq!(state.new_workspace_row_index(), saved_count + 1);

    let rows = [
        ManagerListRow::CurrentDirectory,
        ManagerListRow::SavedWorkspace(0),
        ManagerListRow::SavedWorkspace(1),
        ManagerListRow::SavedWorkspace(2),
        ManagerListRow::NewWorkspace,
    ];
    for row in rows {
        let idx = row.to_screen_index(saved_count).unwrap();
        assert_eq!(state.row_at(idx), Some(row), "row_at({idx}) for {row:?}");
        state.selected = idx;
        assert_eq!(state.selected_row(), row, "selected_row for idx={idx}");
    }

    assert_eq!(
        ManagerListRow::NewWorkspace.to_visual_index(saved_count),
        Some(saved_count + 2)
    );
    assert_eq!(state.row_at_visual_index(saved_count + 1), None);
    assert_eq!(
        state.row_at_visual_index(saved_count + 2),
        Some(ManagerListRow::NewWorkspace)
    );

    // Out-of-range index returns None.
    assert_eq!(state.row_at(saved_count + 2), None);
}

/// `selected_workspace_summary` must return `None` for both synthetic
/// rows (cwd + sentinel) and `Some(&WorkspaceSummary)` for a real
/// saved row.
#[test]
fn manager_selected_workspace_summary_is_none_for_synthetic_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    config.workspaces.insert("alpha".into(), empty_ws("/alpha"));
    let mut state = ManagerState::from_config(&config, cwd);

    // Current directory row.
    state.selected = ManagerListRow::CurrentDirectory.to_screen_index(1).unwrap();
    assert!(state.selected_workspace_summary().is_none());
    assert!(state.is_current_dir_selected());

    // Saved workspace row.
    state.selected = ManagerListRow::SavedWorkspace(0)
        .to_screen_index(1)
        .unwrap();
    let summary = state
        .selected_workspace_summary()
        .expect("saved row exposes summary");
    assert_eq!(summary.name, "alpha");

    // "+ New workspace" sentinel.
    state.selected = ManagerListRow::NewWorkspace.to_screen_index(1).unwrap();
    assert!(state.selected_workspace_summary().is_none());
    assert!(state.is_new_workspace_selected());
}

#[test]
fn global_mounts_state_persists_add_edit_remove_rename_scope_readonly() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();
    let source_a = temp.path().join("cache-a");
    let source_b = temp.path().join("cache-b");
    std::fs::create_dir_all(&source_a).unwrap();
    std::fs::create_dir_all(&source_b).unwrap();

    let mut state = SettingsState::from_config(&AppConfig::default()).mounts;
    state.pending.push(jackin_config::GlobalMountRow {
        scope: None,
        name: "gradle".into(),
        mount: MountConfig {
            src: source_a.display().to_string(),
            dst: "/home/agent/.gradle/caches".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
    });
    crate::console::services::config::save_global_mounts(&paths, &state.original, &state.pending)
        .unwrap();
    state.mark_saved();

    state.pending[0].name = "cargo".into();
    state.pending[0].mount.src = source_b.display().to_string();
    state.pending[0].mount.dst = "/home/agent/.cargo/registry".into();
    state.pending[0].mount.readonly = true;
    state.pending[0].scope = Some("chainargos/*".into());
    state.pending.push(jackin_config::GlobalMountRow {
        scope: None,
        name: "remove-me".into(),
        mount: MountConfig {
            src: source_a.display().to_string(),
            dst: "/remove-me".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
    });
    state.pending.retain(|row| row.name != "remove-me");
    let saved = crate::console::services::config::save_global_mounts(
        &paths,
        &state.original,
        &state.pending,
    )
    .unwrap();
    state.mark_saved();

    let rows = saved.list_mount_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "cargo");
    assert_eq!(rows[0].scope.as_deref(), Some("chainargos/*"));
    assert!(rows[0].mount.readonly);
    assert_eq!(rows[0].mount.dst, "/home/agent/.cargo/registry");
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(raw.contains("[docker.mounts.\"chainargos/*\"]"), "{raw}");
    assert!(!raw.contains("remove-me"), "{raw}");
}

#[test]
fn settings_save_zai_ignore_removes_global_key() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
ZAI_API_KEY = "secret"
"#,
    )
    .unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    let mut state = SettingsState::from_config(&config);
    let row = state
        .auth
        .pending
        .iter_mut()
        .find(|row| row.kind == AuthKind::Zai)
        .expect("settings auth rows include Z.AI");
    row.mode = jackin_console::tui::auth::AuthMode::Ignore;

    state.clear_ignored_env_only_auth_keys();
    let saved = crate::console::services::config::save_settings(
        &paths,
        crate::console::services::config::SettingsSaveInput {
            mounts_original: &state.mounts.original,
            mounts_pending: &state.mounts.pending,
            env_original: &state.env.original,
            env_pending: &state.env.pending,
            auth_pending: &state.auth.pending,
            original_github_env: &state.auth.original_github_env,
            github_env: &state.auth.github_env,
            trust_pending: &state.trust.pending,
            git_coauthor_trailer: state.general.pending_coauthor_trailer,
            git_dco: state.general.pending_dco,
        },
    )
    .unwrap();
    state.mark_saved();

    assert!(!saved.env.contains_key("ZAI_API_KEY"));
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("ZAI_API_KEY"), "{raw}");
}

// ── cycle_isolation_for_selected_mount ─────────────────────────────

/// Build an editor sitting on the Mounts tab with a single Shared mount,
/// cursor on row 0. Mirrors the readonly toggle test fixtures so the new
/// I-hotkey tests share the same shape as the R-hotkey ones.
fn editor_with_one_shared_mount() -> EditorState<'static> {
    use std::collections::BTreeMap;
    let ws = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: String::new(),
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        allowed_roles: vec![],
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::default(),
        roles: BTreeMap::default(),
        keep_awake: KeepAwakeConfig::default(),
        docker: None,
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
    };
    let mut e = EditorState::new_edit("ws".into(), ws);
    e.active_tab = EditorTab::Mounts;
    e.active_field = FieldFocus::Row(0);
    e
}

#[test]
fn cycle_isolation_shared_to_worktree() {
    let mut e = editor_with_one_shared_mount();
    e.cycle_isolation_for_selected_mount();
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Worktree,
        "Shared must cycle to Worktree on first I press"
    );
}

#[test]
fn cycle_isolation_worktree_back_to_shared() {
    let mut e = editor_with_one_shared_mount();
    e.cycle_isolation_for_selected_mount();
    e.cycle_isolation_for_selected_mount();
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Clone,
        "two I presses must cycle Worktree to Clone",
    );
    e.cycle_isolation_for_selected_mount();
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Shared,
        "three I presses must net back to Shared",
    );
    assert_eq!(
        e.change_count(),
        0,
        "a full cycle Shared → Worktree → Shared must net zero changes",
    );
}

#[test]
fn cycle_isolation_on_sentinel_is_noop() {
    // Cursor on the `+ Add mount` sentinel (row == mounts.len()) — I must
    // not mutate mounts or trigger a change.
    let mut e = editor_with_one_shared_mount();
    e.active_field = FieldFocus::Row(e.pending.mounts.len());
    let before = e.pending.mounts.clone();
    e.cycle_isolation_for_selected_mount();
    assert_eq!(
        e.pending.mounts, before,
        "I on sentinel row must leave mounts untouched"
    );
    assert_eq!(e.change_count(), 0);
}
