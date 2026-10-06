// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn global_mount_save_detects_sensitive_sources() {
    let rows = vec![jackin_config::GlobalMountRow {
        scope: None,
        name: "ssh".into(),
        mount: jackin_config::MountConfig {
            src: "/home/user/.ssh".into(),
            dst: "/ssh".into(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        },
    }];

    assert!(crate::services::workspace::global_rows_have_sensitive_mount(&rows));
}

#[test]
fn add_flow_asks_scope_before_workspace_mount_flow() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Mounts;
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Char('a')));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountScopePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountFileBrowser { .. })
    ));
}

#[test]
fn global_mount_add_filebrowser_esc_closes_chain() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Mounts;
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Char('a')));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountFileBrowser { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

    // The ScopePicker was committed when AllAgents was picked, so Esc
    // on the FileBrowser must close the modal chain entirely rather
    // than resurrect a consumed picker.
    assert!(
        !settings.mounts.modals.is_open(),
        "Esc from add-mount FileBrowser should close the chain; got {:?}",
        settings.mounts.modals.current()
    );
    assert!(
        settings.mounts.error.is_none(),
        "normal add-mount cancel must not become Settings error"
    );
}

#[test]
fn global_mount_add_cancel_does_not_open_settings_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Mounts;
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Char('a')));
    {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));
    }

    after_settings_event(&mut state);

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("must stay in Settings stage");
    };
    assert!(settings.error_popup.is_none());
    assert!(settings.mounts.error.is_none());
}

#[test]
fn global_mount_filebrowser_open_git_url_returns_typed_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let mut settings = SettingsState::from_config(&AppConfig::default());
    let mut browser =
        FileBrowserState::from_listing(crate::services::file_browser::listing_from_home().unwrap());
    browser.pending_git_prompt = Some(tmp.path().to_path_buf());
    browser.pending_git_url = Some("file:///tmp/settings-url".into());
    settings
        .mounts
        .modals
        .open(SettingsModal::MountFileBrowser {
            state: Box::new(browser),
        });

    let outcome = handle_settings_confirm_modal(
        &mut settings,
        key(KeyCode::Char('O')),
        Rect::new(0, 0, 120, 40),
    );

    assert!(matches!(
        outcome,
        SettingsModalOutcome::OpenUrl(url) if url == "file:///tmp/settings-url"
    ));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountFileBrowser { .. })
    ));
}

#[test]
fn add_flow_specific_scope_uses_shared_role_picker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
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
    settings.active_tab = SettingsTab::Mounts;
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Char('a')));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    let Some(SettingsModal::MountScopePicker { state: picker }) =
        settings.mounts.modals.current_mut()
    else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountRolePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountFileBrowser { .. })
    ));
    assert_eq!(
        settings
            .mounts
            .add_draft
            .as_ref()
            .and_then(|draft| draft.scope.as_deref()),
        Some("agent-smith")
    );
}

#[test]
fn global_mount_role_picker_esc_returns_scope_picker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
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
    settings.active_tab = SettingsTab::Mounts;
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Char('a')));
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };
    let Some(SettingsModal::MountScopePicker { state: picker }) =
        settings.mounts.modals.current_mut()
    else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountRolePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

    assert!(
        !settings.mounts.modals.is_open(),
        "Esc from global-mount RolePicker should close the chain; got {:?}",
        settings.mounts.modals.current()
    );
    assert!(
        settings.mounts.error.is_none(),
        "normal role-picker cancel must not become Settings error"
    );
}

#[test]
fn settings_tab_navigation_reaches_all_config_tabs() {
    // W3C ARIA Tabs: Right cycles tabs when the tab bar has focus.
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));
    // Settings opens with tab_bar_focused = true; Right cycles forward.
    assert!(
        matches!(&state.stage, ManagerStage::Settings(s) if s.tab_bar_focused()),
        "must start on tab bar"
    );

    // Settings opens on General (first tab); Right cycles: General → Mounts → Environments → Auth → Trust → General
    handle_settings_key(&mut state, key(KeyCode::Right));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Mounts)
    );
    handle_settings_key(&mut state, key(KeyCode::Right));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Environments)
    );
    handle_settings_key(&mut state, key(KeyCode::Right));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Auth)
    );
    handle_settings_key(&mut state, key(KeyCode::Right));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Trust)
    );
    handle_settings_key(&mut state, key(KeyCode::Right));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::General)
    );
}

#[test]
fn settings_tab_bar_follows_aria_focus_pattern() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

    handle_settings_key(&mut state, key(KeyCode::Down));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if !settings.tab_bar_focused()),
        "Down from focused tab bar must enter content",
    );

    handle_settings_key(&mut state, key(KeyCode::BackTab));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.tab_bar_focused()),
        "ShiftTab from content must return to tab bar",
    );

    handle_settings_key(&mut state, key(KeyCode::Tab));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if !settings.tab_bar_focused()),
        "Tab from focused tab bar must enter content",
    );

    handle_settings_key(&mut state, key(KeyCode::Esc));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.tab_bar_focused()),
        "Esc from content must return to tab bar",
    );
}

#[test]
fn settings_focus_owner_exclusivity() {
    // Defect 563 regression: when content owns focus, exactly one "green border"
    // signal exists — tab_bar_focused is false AND the active-tab's scroll_focused
    // is true. The tab bar must not also be green (tab_bar_focused must be false).
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

    // Enter content (General tab by default).
    handle_settings_key(&mut state, key(KeyCode::Down));
    {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("settings stage expected");
        };
        assert!(
            !settings.tab_bar_focused(),
            "tab_bar must yield focus when content gains it"
        );
    }
    // Return to tab bar, switch to Mounts tab, enter content.
    handle_settings_key(&mut state, key(KeyCode::Esc));
    handle_settings_key(&mut state, key(KeyCode::Right));
    handle_settings_key(&mut state, key(KeyCode::Down));
    {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("settings stage expected");
        };
        assert!(
            !settings.tab_bar_focused(),
            "tab bar must not be green while content is focused"
        );
        assert!(
            settings.content_focused(SettingsTab::Mounts),
            "settings focus owner must name mounts content (Defect 18)"
        );
    }
    handle_settings_key(&mut state, key(KeyCode::Esc));
    {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("settings stage expected");
        };
        assert!(settings.tab_bar_focused(), "tab bar regains focus on Esc");
        assert!(
            !settings.content_focused(SettingsTab::Mounts),
            "Esc returns focus ownership to the tab bar"
        );
    }
}

#[test]
fn trust_tab_space_toggles_trusted_state() {
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

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.trust.pending[0].trusted);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(!settings.trust.pending[0].trusted);

    handle_settings_key(&mut state, key(KeyCode::Char(' ')));
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.trust.pending[0].trusted);
}
