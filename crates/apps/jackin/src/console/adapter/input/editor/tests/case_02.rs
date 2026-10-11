// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn editor_env_source_picker_esc_restores_key_input() {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    let scope = SecretsScopeTag::Workspace;
    let target = TextInputTarget::EnvKey {
        scope: scope.clone(),
    };
    editor.modal = Some(Modal::TextInput {
        target: target.clone(),
        state: env_key_input_state(&editor, &scope, secret_new_key_label(&scope), "API_KEY"),
    });

    apply_text_input_to_pending(&target, &mut editor, "API_KEY", false);
    assert!(matches!(editor.modal, Some(Modal::SourcePicker { .. })));
    assert_eq!(editor.modal_parents.len(), 1);

    handle_modal(&mut editor, key(KeyCode::Esc));

    assert!(
        matches!(
            editor.modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc from SourcePicker should restore EnvKey input; got {:?}",
        editor.modal
    );
    assert!(editor.modal_parents.is_empty());
}

#[test]
fn editor_cancel_does_not_push_mount() {
    // C / Esc dismisses the choice modal without touching pending.mounts.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(editor.modal.is_none(), "Esc closes the modal");
    assert_eq!(
        editor.pending.mounts.len(),
        0,
        "Cancel must not push a mount"
    );

    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('c')));
    assert!(editor.modal.is_none(), "`c` closes the modal");
    assert_eq!(editor.pending.mounts.len(), 0, "`c` must not push a mount");
}

#[test]
fn editor_right_arrow_is_noop_on_non_header_row() {
    // Right must not cycle tabs — it is an intra-area horizontal key.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Right),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.active_tab,
        EditorTab::General,
        "Right must not advance tab"
    );
}

#[test]
fn editor_left_arrow_is_noop_on_non_header_row() {
    // Left must not cycle tabs — it is an intra-area horizontal key.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::Mounts);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Left),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(e.active_tab, EditorTab::Mounts, "Left must not rewind tab");
}

#[test]
fn roles_tab_enter_on_load_role_row_opens_role_input() {
    let (tmp, paths, mut config) = {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = config_with_agents(&["agent-smith"]);
        (tmp, paths, config)
    };
    let cwd = tmp.path();
    let mut state = editor_on_agents_tab(empty_ws(), config.roles.len());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Role);
            assert_eq!(state.label, "Load role");
        }
        other => panic!("expected TextInput(Role); got {other:?}"),
    }
}

#[tokio::test]
async fn role_input_resolves_then_persists_namespaced_role_after_trust() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let selector = jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap();
    let cached_repo = CachedRepo::new(&paths, &selector);
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;

    assert!(
        runner
            .recorded
            .iter()
            .any(|cmd| cmd
                .contains("git clone https://github.com/chainargos/jackin-agent-brown.git")),
        "role add must clone through the normal repo resolver; got {:?}",
        runner.recorded
    );
    let clone_cmd = runner
        .recorded
        .iter()
        .find(|cmd| cmd.contains("git clone https://github.com/chainargos/jackin-agent-brown.git"))
        .expect("clone command should be recorded");
    assert!(
        clone_cmd.contains(paths.data_dir.to_str().unwrap()),
        "role add should clone into a temp dir under data_dir first: {clone_cmd}"
    );
    assert!(
        !clone_cmd.contains(paths.roles_dir.to_str().unwrap()),
        "role add must not clone directly into the final role cache: {clone_cmd}"
    );
    assert!(
        cached_repo.repo_dir.join("jackin.role.toml").is_file(),
        "validated clone should be moved into the role cache"
    );

    match &editor.modal {
        Some(Modal::Confirm { target, state }) => {
            assert_eq!(state.title(), "Trust role source");
            let jackin_console::tui::components::ConfirmKind::Details { rows, notes, .. } =
                state.kind()
            else {
                panic!("expected Details kind, got {:?}", state.kind());
            };
            assert!(
                rows.iter()
                    .any(|(label, value)| label == "Role" && value == "chainargos/agent-brown")
            );
            assert!(
                rows.iter().any(|(label, value)| label == "Repository"
                    && value == "https://github.com/chainargos/jackin-agent-brown.git"),
                "trust prompt should show the repository URL",
            );
            assert!(
                notes
                    .iter()
                    .any(|note| note == "Dockerfile can run during image builds.")
            );
            match target {
                ConfirmTarget::TrustRoleSource { key, source } => {
                    assert_eq!(key, "chainargos/agent-brown");
                    assert_eq!(
                        source.git,
                        "https://github.com/chainargos/jackin-agent-brown.git"
                    );
                    assert!(
                        !source.trusted,
                        "newly resolved third-party role should require explicit trust first"
                    );
                }
                other => panic!("expected TrustRoleSource target; got {other:?}"),
            }
        }
        other => panic!("expected trust Confirm modal; got {other:?}"),
    }
    assert!(
        !editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "role should not be allowed before trust confirmation"
    );
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "validated role source should be registered untrusted before trust confirmation"
    );
    let before_trust = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        before_trust.contains("[roles.\"chainargos/agent-brown\"]"),
        "validated role source should be persisted before trust confirmation:\n{before_trust}"
    );
    assert!(
        !before_trust.contains("trusted = true"),
        "role source should remain untrusted before trust confirmation:\n{before_trust}"
    );

    handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

    assert!(editor.modal.is_none(), "trust confirmation should close");
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "custom allow-list should include the newly resolved role"
    );
    let source = config
        .roles
        .get("chainargos/agent-brown")
        .expect("role source must be added to config");
    assert_eq!(
        source.git,
        "https://github.com/chainargos/jackin-agent-brown.git"
    );
    assert!(source.trusted, "trusted role should be marked trusted");
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("[roles.\"chainargos/agent-brown\"]"),
        "new role source should be persisted:\n{persisted}"
    );
    assert!(
        persisted.contains("trusted = true"),
        "trusted role should be persisted with trusted = true:\n{persisted}"
    );
}

#[test]
fn role_load_poll_success_replaces_loading_popup_with_trust_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let rx = jackin_console::tui::runtime::ready_blocking_subscription(Ok(()));
    let source = jackin_config::RoleSource {
        git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
        trusted: false,
        ..Default::default()
    };
    editor.pending_role_load = Some(PendingRoleLoad {
        raw: "chainargos/agent-brown".into(),
        key: "chainargos/agent-brown".into(),
        source,
        rx,
    });
    editor.modal = Some(Modal::StatusPopup {
        state: jackin_console::tui::components::status_popup::role_loading_status_popup_state(
            "chainargos/agent-brown",
        ),
    });
    poll_role_load(&mut editor, &mut config, &paths);

    assert!(
        editor.pending_role_load.is_none(),
        "completed role load should clear pending state"
    );
    match &editor.modal {
        Some(Modal::Confirm { target, state }) => {
            assert_eq!(state.title(), "Trust role source");
            match target {
                ConfirmTarget::TrustRoleSource { key, source } => {
                    assert_eq!(key, "chainargos/agent-brown");
                    assert_eq!(
                        source.git,
                        "https://github.com/chainargos/jackin-agent-brown.git"
                    );
                    assert!(!source.trusted);
                }
                other => panic!("expected TrustRoleSource target; got {other:?}"),
            }
        }
        other => panic!("expected trust Confirm modal; got {other:?}"),
    }
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "completed load should register the untrusted role source"
    );
}

#[tokio::test]
async fn role_input_trust_decline_keeps_registered_role_untrusted() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;
    assert!(matches!(editor.modal, Some(Modal::Confirm { .. })));

    handle_modal_with(&mut editor, key(KeyCode::Char('n')), &mut config, &paths);

    assert!(editor.modal.is_none(), "decline should close trust prompt");
    assert!(
        !editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "declined role must not be added to the custom allow-list"
    );
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "declined role should remain registered but untrusted"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("[roles.\"chainargos/agent-brown\"]"),
        "declined role source should remain registered:\n{persisted}"
    );
    assert!(
        !persisted.contains("trusted = true"),
        "declined role must not be persisted as trusted:\n{persisted}"
    );
}
