// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn m_on_agent_key_unmasks_only_that_row_in_that_agent_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();

    ws.env.insert("API_TOKEN".into(), "ws-value".into());
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert("API_TOKEN".into(), "role-value".into());
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
    let role_key_row = editor
        .secrets_flat_rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                SecretsRow::RoleKeyRow { role, key } if role == "smith" && key == "API_TOKEN"
            )
        })
        .expect("role API_TOKEN row");
    editor.active_field = FieldFocus::Row(role_key_row);
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
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Role("smith".into()), "API_TOKEN".into())),
        "role-scope API_TOKEN must be unmasked"
    );
    assert!(
        !e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "API_TOKEN".into())),
        "workspace-scope API_TOKEN with same key name must remain masked"
    );
}

#[test]
fn cursor_skips_section_spacer_on_down_arrow() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert(
        "agent-smith".into(),
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

    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    if let ManagerStage::Editor(e) = &state.stage {
        let rows = e.secrets_flat_rows();
        assert!(matches!(
            rows.first(),
            Some(SecretsRow::WorkspaceAddSentinel)
        ));
        assert!(matches!(rows.get(1), Some(SecretsRow::SectionSpacer)));
        assert!(matches!(rows.get(2), Some(SecretsRow::RoleHeader { .. })));
    }

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(2)),
        "↓ from sentinel(0) must skip spacer(1) and land on header(2); \
             got {:?}",
        e.active_field
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Up),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(0)),
        "↑ from header(2) must skip spacer(1) and land on sentinel(0); \
             got {:?}",
        e.active_field
    );
}

#[test]
fn space_on_general_keep_awake_row_toggles_pending_flag() {
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_field = FieldFocus::Row(2);
    }

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char(' ')),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.keep_awake.enabled,
        "first Space on row 2 must enable keep_awake"
    );
    assert!(
        !e.original.keep_awake.enabled,
        "Space must mutate pending only, not original (so the diff is visible to save)"
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char(' ')),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.keep_awake.enabled,
        "second Space must toggle keep_awake back off",
    );
}

#[test]
fn enter_on_general_toggle_rows_does_not_toggle_flags() {
    for (row, label) in [(2usize, "keep_awake"), (3usize, "git_pull_on_entry")] {
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(row);
            assert!(!e.pending.keep_awake.enabled);
            assert!(!e.pending.git_pull_on_entry);
        }

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
            !e.pending.keep_awake.enabled,
            "Enter on {label} row must not toggle keep_awake",
        );
        assert!(
            !e.pending.git_pull_on_entry,
            "Enter on {label} row must not toggle git_pull_on_entry",
        );
    }
}

#[test]
fn space_on_general_non_toggle_rows_does_not_flip_keep_awake() {
    for row in [0usize, 1usize] {
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(row);
        }
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(' ')),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.keep_awake.enabled,
            "Space on General row {row} must NOT toggle keep_awake",
        );
    }
}

#[test]
fn down_arrow_on_general_can_reach_keep_awake_row() {
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_field = FieldFocus::Row(0);
    }
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(2)),
        "two ↓ presses from row 0 must land on row 2 (Keep awake); got {:?}",
        e.active_field,
    );
}

#[test]
fn tui_text_entry_op_uri_always_commits_as_plain() {
    let mut editor = EditorState::new_edit("CLAUDE_TOKEN_WS".into(), WorkspaceConfig::default());

    let target = TextInputTarget::EnvValue {
        scope: SecretsScopeTag::Workspace,
        key: "CLAUDE_TOKEN".into(),
    };

    apply_text_input(&target, &mut editor, "op://Vault/Item/Field");

    let stored = editor
        .pending
        .env
        .get("CLAUDE_TOKEN")
        .expect("CLAUDE_TOKEN must be present after commit");

    assert_eq!(
        stored,
        &jackin_core::EnvValue::Plain("op://Vault/Item/Field".into()),
        "text-entry commit of op:// string must store EnvValue::Plain, \
             not EnvValue::OpRef — the picker is the only path to OpRef"
    );

    assert!(
        !matches!(stored, jackin_core::EnvValue::OpRef(_)),
        "text entry must never produce EnvValue::OpRef"
    );
}
