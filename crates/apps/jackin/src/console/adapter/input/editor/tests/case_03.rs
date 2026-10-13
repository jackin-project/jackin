// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn role_input_existing_untrusted_role_can_be_validated_and_trusted() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    config.roles.insert(
        "chainargos/agent-brown".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
            trusted: false,
            ..Default::default()
        },
    );
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
    assert!(matches!(
        editor.modal,
        Some(Modal::Confirm {
            target: ConfirmTarget::TrustRoleSource { .. },
            ..
        })
    ));

    handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| source.trusted),
        "existing untrusted role should become trusted after confirmation"
    );
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "trusted role should be added to the custom allow-list"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("trusted = true"),
        "confirmed role should persist trust:\n{persisted}"
    );
}

#[tokio::test]
async fn role_input_trusted_existing_role_skips_trust_prompt() {
    // When the config already has a trusted role source the editor
    // must register the cached repo and add it to the workspace
    // *without* re-prompting for trust (`Ok(source) if
    // source.trusted` branch in the role-registration effect executor).
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    config.roles.insert(
        "chainargos/agent-brown".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
            trusted: true,
            ..Default::default()
        },
    );
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

    assert!(
        editor.modal.is_none(),
        "trusted existing role must not open the trust-confirm modal: {:?}",
        editor.modal
    );
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "trusted role should be added to the custom allow-list directly: {:?}",
        editor.pending.allowed_roles
    );
}

#[tokio::test]
async fn role_input_clone_failure_reports_candidate_repository_url() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let mut runner = FakeRunner::default();
    runner
        .fail_with
        .push(("git clone".into(), "repository not found".into()));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "the-architect2",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
            assert!(
                state
                    .message
                    .contains("https://github.com/jackin-project/jackin-the-architect2.git"),
                "message should show the repository URL that was tried:\n{}",
                state.message
            );
            assert!(
                state
                    .message
                    .contains("Repository is not available, or you do not have access."),
                "message should explain the repository is unavailable:\n{}",
                state.message
            );
            assert!(
                !state.message.contains("git clone"),
                "user-facing popup should not include raw clone commands:\n{}",
                state.message
            );
            assert!(
                !state.message.contains(paths.roles_dir.to_str().unwrap()),
                "user-facing popup should not expose the final role cache path:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for failed clone; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("the-architect2"),
        "failed clone must not add the role to in-memory config"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        !persisted.contains("the-architect2"),
        "failed clone must not persist the role:\n{persisted}"
    );
}

#[tokio::test]
async fn role_input_invalid_repo_reports_role_contract_error() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || {
            let repo_dir = first_temp_role_repo(&data_dir);
            std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
            std::fs::write(
                repo_dir.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
        }),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(
                state
                    .message
                    .contains("Repository is not a valid jackin❯ role: missing jackin.role.toml."),
                "message should explain the failed role validation:\n{}",
                state.message
            );
            assert!(
                state
                    .message
                    .contains("https://github.com/chainargos/jackin-agent-brown.git"),
                "message should show the repository URL that was tried:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for invalid role repo; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("chainargos/agent-brown"),
        "invalid role repo must not register the role source"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        !persisted.contains("chainargos/agent-brown"),
        "invalid role repo must not persist the role:\n{persisted}"
    );
}

#[test]
fn role_input_rejects_invalid_selector_with_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::Role,
        state: role_load_input_state(Vec::new()),
    });
    if let Some(Modal::TextInput { state, .. }) = editor.modal.as_mut() {
        for ch in "Chain Argus Agent Brown".chars() {
            state.handle_key(key(KeyCode::Char(ch)).into());
        }
    }

    handle_modal_with(&mut editor, key(KeyCode::Enter), &mut config, &paths);

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
        }
        other => panic!("expected ErrorPopup for invalid selector; got {other:?}"),
    }
    assert!(
        config.roles.is_empty(),
        "invalid selector must not mutate config"
    );
}

#[tokio::test]
async fn role_input_panic_in_registration_is_converted_to_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(|| panic!("test panic while cloning role repo")),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "the-architect2",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
            assert!(
                state.message.contains("test panic while cloning role repo"),
                "panic payload should be visible in the error dialog:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for registration panic; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("the-architect2"),
        "panic must not register the role source"
    );
}

#[test]
fn role_text_input_misroute_uses_error_popup_instead_of_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut editor = EditorState::new_edit("ws".into(), empty_ws());

    apply_text_input_to_pending(&TextInputTarget::Role, &mut editor, "agent-smith", false);

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(
                state.message.contains("generic text-input handler"),
                "message should explain the misrouted role input:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for role misroute; got {other:?}"),
    }
    assert!(config.roles.is_empty());
    let _unused = paths;
}

#[test]
fn agents_tab_star_sets_default_on_allowed_agent() {
    // Cursor on row 1 (role "beta"), no default set yet. Workspace
    // starts in "all roles allowed" shorthand, so beta is
    // effectively allowed. Pressing `*` pins it as default while
    // preserving the shorthand (empty allow list).
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.default_role.as_deref(),
        Some("beta"),
        "`*` on row 1 should pin role `beta` as default",
    );
    assert!(
        e.pending.allowed_roles.is_empty(),
        "default-role pick must preserve the all-roles shorthand; \
             got {:?}",
        e.pending.allowed_roles,
    );
}
