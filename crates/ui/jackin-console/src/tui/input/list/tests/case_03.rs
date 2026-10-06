// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn confirm_instance_purge_esc_dismisses_without_dispatch() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-esc",
        InstanceStatus::Running,
        workdir,
    )];
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('p')),
    )
    .unwrap();
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Esc),
    )
    .unwrap();
    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(matches!(state.stage, ManagerStage::List));
}

#[test]
fn t_key_dispatches_stop_for_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-stop",
        InstanceStatus::Running,
        workdir,
    )];
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('t')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-stop");
            assert_eq!(action, ConsoleInstanceAction::Stop);
        }
        other => panic!("expected stop instance action; got {other:?}"),
    }
}

#[test]
fn t_key_shows_no_instance_popup_when_no_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    // Only a CleanExited entry — Stop must not accept it.
    state.instances = vec![instance_entry(
        "jackin-demo-architect-stale",
        InstanceStatus::CleanExited,
        workdir,
    )];
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('t')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "T on non-Running must yield Continue (with the no-instance modal); got {outcome:?}"
    );
    assert!(
        matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
        "expected ErrorPopup modal explaining no running instance"
    );
}

#[test]
fn a_key_starts_new_session_in_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::Active,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('a')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::NewSession);
        }
        other => panic!("expected NewSession action; got {other:?}"),
    }
}

#[test]
fn x_key_opens_shell_in_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::Active,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('x')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Shell);
        }
        other => panic!("expected Shell action; got {other:?}"),
    }
}

#[test]
fn a_and_x_return_continue_for_non_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    // RestoreAvailable instance — not active/running, so a/x must return Continue.
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::RestoreAvailable,
        workdir,
    )];

    for key_char in ['a', 'x'] {
        state.list_modal = None;
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(key_char)),
        )
        .unwrap();
        assert!(
            matches!(outcome, InputOutcome::Continue),
            "'{key_char}' on non-running instance must return Continue; got {outcome:?}",
        );
        assert!(
            matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
            "'{key_char}' on non-running instance must open an ErrorPopup; got {:?}",
            state.list_modal,
        );
    }
}

#[test]
fn moving_selection_resets_mount_scroll_state() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    // When no block is focused, Down navigates the workspace list and resets scroll.
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_mounts_scroll, 24);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_global_mounts_scroll, 16);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_role_global_mounts_scroll, 8);
    state.set_list_scroll_focus(None);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

    assert_eq!(state.selected, 1);
    assert_eq!(state.list_mounts_scroll.offset_x(), 0);
    assert_eq!(state.list_global_mounts_scroll.offset_x(), 0);
    assert_eq!(state.list_role_global_mounts_scroll.offset_x(), 0);
    assert_eq!(state.list_scroll_focus(), None);
}

#[test]
fn down_key_with_focused_block_clamps_vertical_scroll_without_selection_move() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    // When a block is focused, Down scrolls that block vertically, not the list.
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

    assert_eq!(
        state.selected, 0,
        "selection must not change while block focused"
    );
    assert_eq!(
        state.list_mounts_scroll.offset_y(),
        0,
        "non-overflowing block stays clamped"
    );
}

#[test]
fn resolve_github_mounts_returns_one_per_github_repo() {
    // A workspace with two github mounts + one folder + one gitlab repo
    // should yield exactly two picker choices.
    let tmp = tempfile::tempdir().unwrap();
    let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
    let repo_b = make_github_repo(tmp.path(), "repo-b", "dev");
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    // Gitlab repo should be skipped.
    let gitlab = tmp.path().join("gl");
    let gl_git = gitlab.join(".git");
    std::fs::create_dir_all(&gl_git).unwrap();
    std::fs::write(gl_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        gl_git.join("config"),
        "[remote \"origin\"]\n    url = git@gitlab.com:owner/repo.git\n",
    )
    .unwrap();

    let ws = WorkspaceConfig {
        mounts: vec![
            mount(repo_a.to_str().unwrap(), "/a"),
            mount(plain.to_str().unwrap(), "/p"),
            mount(repo_b.to_str().unwrap(), "/b"),
            mount(gitlab.to_str().unwrap(), "/g"),
        ],
        ..WorkspaceConfig::default()
    };

    let choices = crate::github_mounts::resolve_for_workspace(&ws);
    assert_eq!(choices.len(), 2);
    // URLs track the HEAD ref per-repo.
    let urls: Vec<&str> = choices.iter().map(|c| c.url.as_str()).collect();
    assert!(urls.contains(&"https://github.com/owner/repo-a/tree/main"));
    assert!(urls.contains(&"https://github.com/owner/repo-b/tree/dev"));
    // Branch label matches Named variant.
    let branches: Vec<&str> = choices.iter().map(|c| c.branch.as_str()).collect();
    assert!(branches.contains(&"main"));
    assert!(branches.contains(&"dev"));
}

#[test]
fn list_o_with_single_github_mount_has_one_resolved_url() {
    // Input queues typed URL-open effect; browser side effects stay in
    // the effect executor.
    let tmp = tempfile::tempdir().unwrap();
    let repo = make_github_repo(tmp.path(), "solo", "trunk");
    let ws = WorkspaceConfig {
        mounts: vec![mount(repo.to_str().unwrap(), "/solo")],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    let effects = state.drain_effects();
    match effects.first() {
        Some(ManagerEffect::OpenUrl(url)) => {
            assert_eq!(url, "https://github.com/owner/solo/tree/trunk");
        }
        other => panic!("expected OpenUrl effect, got {other:?}"),
    }
}

#[test]
fn list_o_with_multiple_github_mounts_opens_picker() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
    let repo_b = make_github_repo(tmp.path(), "repo-b", "main");
    let ws = WorkspaceConfig {
        mounts: vec![
            mount(repo_a.to_str().unwrap(), "/a"),
            mount(repo_b.to_str().unwrap(), "/b"),
        ],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    match &state.list_modal {
        Some(Modal::GithubPicker { state: picker }) => {
            assert_eq!(picker.choices.len(), 2);
        }
        other => panic!("expected GithubPicker modal; got {other:?}"),
    }
}

#[test]
fn list_o_with_zero_github_mounts_is_silent_noop() {
    let tmp_src = tempfile::tempdir().unwrap();
    let plain = tmp_src.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let ws = WorkspaceConfig {
        mounts: vec![mount(plain.to_str().unwrap(), "/p")],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(state.list_modal.is_none(), "no modal when no GitHub URLs");
}
