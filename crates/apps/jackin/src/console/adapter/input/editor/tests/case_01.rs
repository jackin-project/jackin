// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn create_mode_enter_on_name_row_opens_rename_modal() {
    // In Create mode, pressing Enter on row 0 (Name) must open the
    // rename TextInput modal pre-filled with the current pending_name
    // — the same flow Edit mode uses. This is the operator's escape
    // hatch from a prelude-captured name they mistyped.
    let (tmp, paths, mut config) = {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let toml = toml::to_string(&config).unwrap();
        std::fs::write(&paths.config_file, toml).unwrap();
        let loaded = AppConfig::load_or_init(&paths).unwrap();
        (tmp, paths, loaded)
    };
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("typo-name".into());
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("still in editor after Enter on name row");
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Name);
            assert_eq!(
                state.value(),
                "typo-name",
                "TextInput must be pre-filled with current pending_name"
            );
        }
        other => panic!("expected TextInput(Name); got {other:?}"),
    }
}

#[test]
fn create_mode_rename_commit_updates_pending_name() {
    // After the TextInput commits a new value, pending_name should
    // reflect the operator's edit. Same code path as Edit mode —
    // apply_text_input_to_pending doesn't distinguish modes.
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("old-name".into());

    apply_text_input(&TextInputTarget::Name, &mut editor, "new-name");

    assert_eq!(editor.pending_name.as_deref(), Some("new-name"));
}

#[test]
fn edit_mode_enter_on_name_row_still_opens_rename_modal() {
    // Regression guard: the Create-mode extension to row 0 Enter must
    // not break the Edit-mode path that already worked.
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert("keep-me".into(), ws.clone());
    let toml = toml::to_string(&config).unwrap();
    std::fs::write(&paths.config_file, toml).unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("keep-me".into(), ws);
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Name);
            assert_eq!(state.value(), "keep-me");
        }
        other => panic!("expected TextInput(Name); got {other:?}"),
    }
}

#[test]
fn account_and_binding_rows_are_all_keyboard_focusable() {
    let rows = vec![
        AuthRow::Account { id: "work".into() },
        AuthRow::Binding {
            agent: jackin_core::Agent::Codex,
            role: None,
        },
        AuthRow::WorkspaceMode {
            kind: AuthKind::Github,
        },
        AuthRow::RoleMode {
            role: "smith".into(),
            kind: AuthKind::Github,
        },
    ];
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert!(skipped.is_empty());
    for index in 0..rows.len() {
        assert_eq!(
            jackin_console::tui::screens::editor::update::step_cursor_down(
                &skipped,
                index,
                rows.len() - 1
            ),
            index
        );
        assert_eq!(
            jackin_console::tui::screens::editor::update::step_cursor_up(&skipped, index),
            index
        );
    }
}

#[test]
fn account_editor_assignment_revokes_dependent_bindings() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::Profile {
                agent: jackin_core::Agent::Codex,
                directory: "/host/codex-work".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    let mut editor = EditorState::new_edit("project".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Auth;
    editor.active_field = FieldFocus::Row(0);
    editor.edit_account_row(&config, false);
    assert_eq!(editor.pending.accounts, ["work"]);
    let binding = editor
        .auth_flat_rows(&config)
        .iter()
        .position(|row| {
            matches!(
                row,
                AuthRow::Binding {
                    agent: jackin_core::Agent::Codex,
                    role: None
                }
            )
        })
        .unwrap();
    editor.active_field = FieldFocus::Row(binding);
    editor.edit_account_row(&config, false);
    assert_eq!(
        editor.pending.account_bindings[&jackin_core::Agent::Codex],
        "work"
    );
    editor.active_field = FieldFocus::Row(0);
    editor.edit_account_row(&config, false);
    assert!(editor.pending.accounts.is_empty());
    assert!(editor.pending.account_bindings.is_empty());
}

#[test]
fn filebrowser_commit_opens_mount_dst_choice_not_text_input() {
    // Pin: the FileBrowser→TextInput chain is replaced by
    // FileBrowser→MountDstChoice. No mount should be pushed yet — the
    // push is deferred to the choice modal's commit handler.
    let editor = editor_with_browser_committed("/host/path");
    assert!(
        matches!(editor.modal, Some(Modal::MountDstChoice { .. })),
        "expected MountDstChoice modal; got {:?}",
        editor.modal
    );
    assert_eq!(
        editor.pending.mounts.len(),
        0,
        "no mount must be pushed until the operator commits in the choice modal"
    );
}

#[test]
fn filebrowser_child_esc_restores_filebrowser_parent() {
    let mut editor = editor_with_file_browser_parent_committed("/host/path");
    assert!(matches!(editor.modal, Some(Modal::MountDstChoice { .. })));
    assert_eq!(editor.modal_parents.len(), 1);

    handle_modal(&mut editor, key(KeyCode::Esc));

    assert!(
        matches!(editor.modal, Some(Modal::FileBrowser { .. })),
        "Esc from MountDstChoice should restore FileBrowser; got {:?}",
        editor.modal
    );
    assert!(editor.modal_parents.is_empty());
}

#[test]
fn filebrowser_open_git_url_returns_typed_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut browser = jackin_console::tui::components::file_browser::FileBrowserState::from_listing(
        jackin_console::services::file_browser::listing_from_home().unwrap(),
    );
    browser.pending_git_prompt = Some(tmp.path().to_path_buf());
    browser.pending_git_url = Some("file:///tmp/editor-url".into());
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: browser,
    });

    let outcome = handle_editor_modal(
        &mut editor,
        key(KeyCode::Char('O')),
        false,
        std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
        &mut config,
        &paths,
        Rect::new(0, 0, 120, 40),
    );

    assert!(matches!(
        outcome,
        EditorModalOutcome::OpenUrl(url) if url == "file:///tmp/editor-url"
    ));
}

#[test]
fn mounts_add_starts_file_browser_listing_worker() {
    let ws = WorkspaceConfig::default();
    let mut state = editor_on_mounts_tab(ws, 0);
    let mut config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('a')),
    )
    .unwrap();
    for effect in state.drain_effects() {
        crate::console::effects::execute_manager_effect(&mut state, &mut config, &paths, effect);
    }

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(
        editor.modal.is_none(),
        "file browser modal should wait for the listing worker; got {:?}",
        editor.modal
    );
    assert!(state.file_browser_listing_in_flight());
}

#[test]
fn o_on_folder_mount_opens_error_popup() {
    // A plain folder mount has no GitHub URL — O must open an ErrorPopup
    // explaining why, not silently do nothing.
    let ws = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/plain-dir".into(),
            dst: "/host/plain-dir".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let mut state = editor_on_mounts_tab(ws, 0);
    let mut config = AppConfig::default();

    press(&mut state, &mut config, KeyCode::Char('o')).unwrap();

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(
        matches!(editor.modal, Some(Modal::ErrorPopup { .. })),
        "O on a folder mount must open an ErrorPopup; got {:?}",
        editor.modal,
    );
}

#[test]
fn added_mount_defaults_to_shared_isolation() {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;

    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from("/host/path"),
    );
    handle_modal(&mut editor, key(KeyCode::Char('m')));

    assert_eq!(editor.pending.mounts.len(), 1);
    assert_eq!(
        editor.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Shared
    );
}

#[test]
fn editor_mount_same_path_commits_mount_with_dst_equal_src() {
    // Mount-at-same-path shortcut on the choice modal → push MountConfig with dst = src
    // and close the modal. No TextInput should appear.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('m')));
    assert!(
        editor.modal.is_none(),
        "Mount at same path must close the modal; got {:?}",
        editor.modal
    );
    assert_eq!(editor.pending.mounts.len(), 1, "exactly one mount pushed");
    let m = &editor.pending.mounts[0];
    assert_eq!(m.src, "/host/path");
    assert_eq!(
        m.dst, "/host/path",
        "Mount-at-same-path fast path sets dst = src"
    );
    assert!(!m.readonly);
}

#[test]
fn editor_edit_opens_textinput_and_pushes_provisional() {
    // Edit destination → push provisional mount (dst = src) + open
    // the TextInput pre-filled with src. Mirrors today's flow so the
    // operator can edit dst in place.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('e')));
    match &editor.modal {
        Some(Modal::TextInput { target, .. }) => {
            assert_eq!(target, &TextInputTarget::MountDst);
        }
        other => panic!("expected TextInput(MountDst); got {other:?}"),
    }
    assert_eq!(
        editor.pending.mounts.len(),
        1,
        "provisional mount pushed for the TextInput to mutate"
    );
    let m = &editor.pending.mounts[0];
    assert_eq!(m.src, "/host/path");
    assert_eq!(m.dst, "/host/path", "provisional dst mirrors src");
}

#[test]
fn editor_mount_destination_esc_walks_back_one_step() {
    let mut editor = editor_with_file_browser_parent_committed("/host/path");

    handle_modal(&mut editor, key(KeyCode::Char('e')));
    assert!(matches!(
        editor.modal,
        Some(Modal::TextInput {
            target: TextInputTarget::MountDst,
            ..
        })
    ));
    assert_eq!(editor.modal_parents.len(), 2);

    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(
        matches!(editor.modal, Some(Modal::MountDstChoice { .. })),
        "Esc from destination input should restore MountDstChoice; got {:?}",
        editor.modal
    );

    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(
        matches!(editor.modal, Some(Modal::FileBrowser { .. })),
        "second Esc should restore FileBrowser; got {:?}",
        editor.modal
    );
}
