// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn r_key_on_non_mounts_tab_is_noop() {
    // Cursor set to row 0 on General tab with a mount present; pressing R
    // must not mutate the mount list (the handler is gated on
    // `active_tab == EditorTab::Mounts`).
    let mut config = AppConfig::default();
    let ws = ws_with_one_mount(false);
    let before = ws.mounts.clone();
    let mut state = editor_on_mounts_tab(ws, 0);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_tab = EditorTab::General;
    }

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts, before,
        "R on non-Mounts tab must leave mounts untouched"
    );
}

#[test]
fn i_key_cycles_isolation_on_current_mount_row() {
    // Start Shared → one I press should flip to Worktree and register
    // as a change. Mirrors `r_key_toggles_readonly_on_current_mount_row`.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('I')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Worktree,
        "I on a Shared mount must cycle to Worktree",
    );
    assert!(
        e.change_count() > 0,
        "cycling isolation must surface as a change; got change_count={}",
        e.change_count(),
    );
}

#[test]
fn i_key_lowercase_also_cycles_isolation() {
    // Operators often hit `i` without holding shift; both cases must work.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('i')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Worktree,
    );
}

#[test]
fn enter_on_op_workspace_key_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );

    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0); // the only key row
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.modal.is_none(),
        "Enter on an op:// row must not open any modal; got {:?}",
        e.modal
    );
}

#[test]
fn enter_on_op_agent_key_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert(
        "API_TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/api-token".into(),
            path: "Personal/api/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    ws.roles.insert(
        "smith".into(),
        jackin_config::WorkspaceRoleOverride {
            env: ag_env,
            account_bindings: std::collections::BTreeMap::default(),
            github: None,
            default_launch: None,
        },
    );

    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.secrets_expanded.insert("smith".into());
    // Rows: WorkspaceAddSentinel(0), SectionSpacer(1), AgentHeader(2),
    //       AgentKeyRow(3), AgentAddSentinel(4). Focus the key row.
    editor.active_field = FieldFocus::Row(3);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.modal.is_none(),
        "Enter on an role op:// row must not open any modal; got {:?}",
        e.modal
    );
}

#[test]
fn secrets_tab_m_accepts_shift_modifier_for_caps_lock_parity() {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("DB_URL".into(), "literal-value".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0); // the only key row
    state.stage = ManagerStage::Editor(editor);

    let shift_m = KeyEvent {
        code: KeyCode::Char('M'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    handle_key(&mut state, &mut config, &paths, tmp.path(), shift_m).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "DB_URL".into())),
        "M with SHIFT modifier (Caps Lock parity) must add the focused \
             row to unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

#[test]
fn m_on_focused_workspace_key_unmasks_only_that_row() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first-value".into());
    ws.env.insert("BETA".into(), "second-value".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    // Rows are alphabetically ordered: ALPHA(0), BETA(1), Sentinel(2).
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "ALPHA".into())),
        "ALPHA must be unmasked"
    );
    assert!(
        !e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "BETA".into())),
        "BETA must remain masked"
    );
}

#[test]
fn m_on_already_unmasked_row_re_masks_it() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert!(
        e.unmasked_rows.is_empty(),
        "second M must remove the row from unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

#[test]
fn m_on_op_reference_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert!(
        e.unmasked_rows.is_empty(),
        "M on an op:// row must not modify unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

#[test]
fn tab_leave_resets_unmasked_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    // Unmask ALPHA.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();
    // Tab from content → tab bar + advances tab to Auth (leaves Secrets).
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Tab),
    )
    .unwrap();
    // Now on tab bar (Auth). Right × 4: General → Mounts → Roles → Secrets.
    for _ in 0..4 {
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Right),
        )
        .unwrap();
    }

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert_eq!(e.active_tab, EditorTab::Secrets);
    assert!(
        e.unmasked_rows.is_empty(),
        "tab-leave must clear unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}
