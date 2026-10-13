// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn right_on_non_expandable_overflowing_sidebar_scrolls_horizontally() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "chainargos-blockchain-nodes-with-a-very-long-name".into(),
        WorkspaceConfig::default(),
    );
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 1;
    state.cached_term_size = Rect::new(0, 0, 70, 24);
    state.set_list_names_focused(true);

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.list_names_scroll.offset_x() > 0,
        "→ should scroll the focused overflowing sidebar when the row has no expand action"
    );
}

#[test]
fn left_on_non_expandable_overflowing_sidebar_scrolls_horizontally() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "chainargos-blockchain-nodes-with-a-very-long-name".into(),
        WorkspaceConfig::default(),
    );
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 1;
    state.cached_term_size = Rect::new(0, 0, 70, 24);
    state.set_list_names_focused(true);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_names_scroll, 8);

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.list_names_scroll.offset_x() < 8,
        "← should scroll the focused overflowing sidebar when the row has no collapse action"
    );
}

#[test]
fn current_directory_row_silently_ignores_edit_and_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "some-ws".into(),
        WorkspaceConfig {
            workdir: "/unrelated".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    let mut state = ManagerState::from_config(&config, cwd);
    assert_eq!(state.selected, 0);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('e')),
    )
    .unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::List),
        "e on row 0 must not open the Editor; got {:?}",
        state.stage
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )
    .unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::List),
        "d on row 0 must not open ConfirmDelete; got {:?}",
        state.stage
    );
}

#[test]
fn enter_on_current_directory_returns_launch_current_dir() {
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
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    assert!(
        matches!(outcome, InputOutcome::LaunchCurrentDir),
        "row 0 Enter must produce LaunchCurrentDir"
    );

    state.selected = 1;
    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    match outcome {
        InputOutcome::LaunchNamed(name) => assert_eq!(name, "alpha"),
        other => panic!("row 1 Enter must produce LaunchNamed(\"alpha\"); got {other:?}"),
    }
}

#[test]
fn w_on_saved_workspace_returns_prewarm_named() {
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
    let mut state = ManagerState::from_config(&config, cwd);

    state.selected = 0;
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('w')),
    )
    .unwrap();
    assert!(matches!(outcome, InputOutcome::Continue));

    state.selected = 1;
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('w')),
    )
    .unwrap();
    match outcome {
        InputOutcome::PrewarmNamed(name) => assert_eq!(name, "alpha"),
        other => panic!("row 1 W must produce PrewarmNamed(\"alpha\"); got {other:?}"),
    }
}

#[test]
fn s_opens_settings_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.mounts.pending.is_empty())
    );
}

#[test]
fn instance_shortcuts_return_selected_workspace_actions() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::RestoreAvailable,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('r')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Reconnect);
        }
        other => panic!("expected reconnect instance action; got {other:?}"),
    }

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('i')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Inspect);
        }
        other => panic!("expected inspect instance action; got {other:?}"),
    }

    // P now stages a confirm modal instead of dispatching Purge
    // directly — the action destroys role + DinD + volume + network
    // + local state in one stroke, so an unconditional confirmation
    // step keeps mis-keyed `P` from destroying running work.
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('p')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "P should stage the confirm modal and return Continue; got {outcome:?}"
    );
    assert!(
        matches!(state.stage, ManagerStage::ConfirmInstancePurge { .. }),
        "P should have set ConfirmInstancePurge stage"
    );

    // Confirm via Y → the staged action fires.
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('y')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Purge);
        }
        other => panic!("expected purge instance action after Y; got {other:?}"),
    }
}

#[test]
fn crashed_instance_is_visible_in_tree_and_enter_restarts_via_ladder() {
    // D15: a failed/stopped instance appears in the console tree, the
    // workspace expands to show it, and selecting + Enter routes it into the
    // restore ladder (the Reconnect action).
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-crashed",
        InstanceStatus::Crashed,
        workdir,
    )];

    assert!(
        state.has_visible_instances(0),
        "a crashed instance must make the workspace expandable"
    );
    state.expand_workspace(0);
    state.selected = state
        .index_of_row(crate::tui::state::ManagerListRow::WorkspaceInstance(0, 0))
        .expect("crashed instance row must be selectable in the tree");

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-crashed");
            assert_eq!(action, ConsoleInstanceAction::Reconnect);
        }
        other => panic!("expected reconnect (restart) instance action; got {other:?}"),
    }
}

#[test]
fn confirm_instance_purge_n_dismisses_without_dispatch() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-cancel",
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
    assert!(matches!(
        state.stage,
        ManagerStage::ConfirmInstancePurge { .. }
    ));
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('n')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "N must return Continue (no dispatch); got {outcome:?}"
    );
    assert!(
        matches!(state.stage, ManagerStage::List),
        "N must reset stage to List"
    );
}
