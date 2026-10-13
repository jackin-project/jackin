// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn change_count_env_set_counts_as_one() {
    let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
    assert_eq!(e.change_count(), 0);
    e.pending
        .env
        .insert("DB_URL".into(), EnvValue::Plain("postgres://…".into()));
    assert_eq!(e.change_count(), 1);
}

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
            account_bindings: std::collections::BTreeMap::default(),
            github: None,
            default_launch: None,
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
            account_bindings: std::collections::BTreeMap::default(),
            github: None,
            default_launch: None,
        },
    );
    assert!(e2.is_dirty(), "role env set must make state dirty");
}

#[test]
fn create_prelude_starts_at_first_step() {
    let p = CreatePreludeState::new();
    assert_eq!(p.wizard.step(), 0);
}

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
fn settings_save_removed_account_revokes_registered_secret() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = jackin_config::ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "zai-work",
            &jackin_config::AccountConfig {
                enabled: true,
                name: "Zai work".into(),
                provider: jackin_config::AiProvider::Zai,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: EnvValue::Plain("synthetic-zai-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        )
        .unwrap();
    let config = editor.save().unwrap();
    let mut state = SettingsState::from_config(&config);
    assert!(state.auth.pending.remove("zai-work").is_some());
    let saved = crate::console::services::config::save_settings(
        &paths,
        crate::console::services::config::SettingsSaveInput {
            mounts_original: &state.mounts.original,
            mounts_pending: &state.mounts.pending,
            env_original: &state.env.original,
            env_pending: &state.env.pending,
            auth_pending: &state.auth.pending,
            auth_original: &state.auth.original,
            bindings_pending: &state.auth.bindings,
            bindings_original: &state.auth.original_bindings,
            original_github: &state.auth.original_github,
            github: &state.auth.github,
            trust_pending: &state.trust.pending,
            git_coauthor_trailer: state.general.pending_coauthor_trailer,
            git_dco: state.general.pending_dco,
        },
    )
    .unwrap();
    state.mark_saved();
    assert!(!saved.accounts.contains_key("zai-work"));
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("synthetic-zai-key"));
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
