// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn general_tab_space_toggles_both_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::General;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    // row 0 (coauthor_trailer) — default is false
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.general.selected, 0);
    assert!(!settings.general.pending_coauthor_trailer);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.general.pending_coauthor_trailer);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(!settings.general.pending_coauthor_trailer);

    // navigate to row 1 (dco)
    handle_settings_key(&mut state, key(KeyCode::Down));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.general.selected, 1);
    assert!(!settings.general.pending_dco);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.general.pending_dco);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(!settings.general.pending_dco);

    // navigate back to row 0
    handle_settings_key(&mut state, key(KeyCode::Up));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.general.selected, 0);
}

#[test]
fn general_tab_enter_does_not_toggle_rows() {
    for selected in [0usize, 1usize] {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::General;
        settings.set_tab_bar_focused(false);
        settings.general.selected = selected;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(
            !settings.general.pending_coauthor_trailer,
            "Enter on settings General row {selected} must not toggle co-author trailer",
        );
        assert!(
            !settings.general.pending_dco,
            "Enter on settings General row {selected} must not toggle DCO",
        );
    }
}

#[test]
fn trust_tab_enter_does_not_toggle_trusted_state() {
    let tmp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".into(),
        RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".into(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Trust;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(
        settings.trust.pending[0].trusted,
        "Enter on Trust row must not toggle trusted state",
    );
}

#[test]
fn env_tab_add_flow_asks_scope_before_key() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Environments;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvScopePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvText {
            target: SettingsEnvTextTarget::EnvKey {
                scope: SettingsEnvScope::Global
            },
            ..
        })
    ));
}

#[test]
fn env_tab_key_input_esc_closes_chain() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Environments;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvText {
            target: SettingsEnvTextTarget::EnvKey { .. },
            ..
        })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Esc),
        std::rc::Rc::clone(&state.op_cache),
    );

    // The ScopePicker was committed before the EnvKey input opened,
    // so Esc on the input must close the chain instead of restoring
    // a consumed picker.
    assert!(
        !settings.env.modals.is_open(),
        "Esc from settings env key input should close the chain; got {:?}",
        settings.env.modals.current()
    );
    assert!(
        settings.env.error.is_none(),
        "normal env key cancel must not become Settings error"
    );
}

#[test]
fn env_add_cancel_does_not_open_settings_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Environments;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            std::rc::Rc::clone(&state.op_cache),
        );
        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Esc),
            std::rc::Rc::clone(&state.op_cache),
        );
    }

    after_settings_event(&mut state);

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("must stay in Settings stage");
    };
    assert!(settings.error_popup.is_none());
    assert!(settings.env.error.is_none());
}

#[test]
fn env_tab_source_picker_esc_returns_key_input() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Environments;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    let target = SettingsEnvTextTarget::EnvKey {
        scope: SettingsEnvScope::Global,
    };
    commit_env_text(&mut settings.env, &target, None, "API_KEY");
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvSourcePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Esc),
        std::rc::Rc::clone(&state.op_cache),
    );

    assert!(
        matches!(
            settings.env.modals.current(),
            Some(SettingsModal::EnvText {
                target: SettingsEnvTextTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc from settings env SourcePicker should restore key input; got {:?}",
        settings.env.modals.current()
    );
}

#[test]
fn env_tab_specific_scope_uses_workspace_role_picker() {
    let tmp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.roles.insert(
        "chainargos/agent-brown".into(),
        RoleSource {
            git: "https://example.invalid/brown.git".into(),
            trusted: false,
            env: BTreeMap::new(),
        },
    );
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Environments;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    let Some(SettingsModal::EnvScopePicker { state: picker }) = settings.env.modals.current_mut()
    else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvRolePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modals.current(),
        Some(SettingsModal::EnvText {
            target: SettingsEnvTextTarget::EnvKey {
                scope: SettingsEnvScope::Role(role)
            },
            ..
        }) if role == "chainargos/agent-brown"
    ));
}

#[test]
fn settings_env_rows_hide_roles_without_env_entries() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-empty".into(),
        RoleSource {
            git: "https://example.invalid/empty.git".into(),
            trusted: false,
            env: BTreeMap::new(),
        },
    );
    config.roles.insert(
        "agent-with-env".into(),
        RoleSource {
            git: "https://example.invalid/with-env.git".into(),
            trusted: false,
            env: BTreeMap::from([(
                "ROLE_ALPHA".into(),
                jackin_core::EnvValue::Plain("one".into()),
            )]),
        },
    );
    let settings = SettingsState::from_config(&config);
    let rows = settings.env_flat_rows();

    assert!(
        !rows.iter().any(
            |row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "agent-empty")
        ),
        "empty role env sections should stay hidden: {rows:?}"
    );
    assert!(
        rows.iter().any(
            |row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "agent-with-env")
        ),
        "roles with env entries should remain visible: {rows:?}"
    );
}
