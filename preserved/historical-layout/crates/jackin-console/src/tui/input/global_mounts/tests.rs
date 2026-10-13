// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `global_mounts`.
use super::super::test_support::key;
use super::*;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::state::{
    ManagerStage, ManagerState, SettingsEnvRow, SettingsEnvTextTarget, SettingsModal,
    SettingsState, SettingsTab,
};
use jackin_config::{AppConfig, RoleSource};
use jackin_core::JackinPaths;
use ratatui::layout::Rect;
use std::collections::BTreeMap;

fn confirm_modal(
    settings: &mut SettingsState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) {
    let outcome = handle_settings_confirm_modal(settings, key, Rect::new(0, 0, 120, 40));
    if matches!(outcome, SettingsModalOutcome::SaveSettings) {
        match crate::services::config_save::save_settings(
            paths,
            crate::services::config_save::SettingsSaveInput {
                mounts_original: &settings.mounts.original,
                mounts_pending: &settings.mounts.pending,
                env_original: &settings.env.original,
                env_pending: &settings.env.pending,
                auth_pending: &settings.auth.pending,
                auth_original: &settings.auth.original,
                original_github: &settings.auth.original_github,
                bindings_pending: &settings.auth.bindings,
                bindings_original: &settings.auth.original_bindings,
                github: &settings.auth.github,
                trust_pending: &settings.trust.pending,
                git_coauthor_trailer: settings.general.pending_coauthor_trailer,
                git_dco: settings.general.pending_dco,
            },
        ) {
            Ok(saved) => {
                *config = saved;
                settings.mark_saved();
                settings.mounts.exit_requested = true;
            }
            Err(err) => settings.mounts.error = Some(err.to_string()),
        }
    }
    if matches!(outcome, SettingsModalOutcome::OpenGlobalMountFileBrowser) {
        match crate::services::file_browser::state_from_home() {
            Ok(file_browser) => {
                settings
                    .mounts
                    .open_sub_modal(SettingsModal::MountFileBrowser {
                        state: Box::new(file_browser),
                    });
            }
            Err(error) => {
                settings.mounts.add_draft = None;
                settings.mounts.error = Some(error.to_string());
            }
        }
    }
    assert!(
        !matches!(outcome, SettingsModalOutcome::OpenUrl(_)),
        "test helper did not expect URL-open"
    );
}

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

#[test]
fn after_settings_event_promotes_subtab_errors_to_error_popup() {
    fn set_mounts_error(settings: &mut SettingsState<'_>) {
        settings.mounts.error = Some("mounts detail".into());
    }
    fn set_env_error(settings: &mut SettingsState<'_>) {
        settings.env.error = Some("env detail".into());
    }
    fn set_auth_error(settings: &mut SettingsState<'_>) {
        settings.auth.error = Some("auth detail".into());
    }
    fn set_trust_error(settings: &mut SettingsState<'_>) {
        settings.trust.error = Some("trust detail".into());
    }

    type SettingsErrorSetter<'a> = fn(&mut SettingsState<'a>);
    let cases: [(&str, SettingsErrorSetter<'_>); 4] = [
        ("mounts", set_mounts_error),
        ("env", set_env_error),
        ("auth", set_auth_error),
        ("trust", set_trust_error),
    ];

    for (name, set_error) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        set_error(&mut settings);
        state.stage = ManagerStage::Settings(settings);

        after_settings_event(&mut state);

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("must stay in Settings stage");
        };
        let popup = settings
            .error_popup
            .as_ref()
            .unwrap_or_else(|| panic!("{name} error must promote to ErrorPopup"));
        assert_eq!(popup.title, "Settings error");
        assert!(
            popup.message.contains(name),
            "{name} error detail must survive promotion: {:?}",
            popup.message,
        );
        assert!(settings.mounts.error.is_none());
        assert!(settings.env.error.is_none());
        assert!(settings.auth.error.is_none());
        assert!(settings.trust.error.is_none());
    }
}

#[test]
fn after_settings_event_exit_requested_pops_to_list() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.mounts.exit_requested = true;
    state.stage = ManagerStage::Settings(settings);

    after_settings_event(&mut state);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "exit_requested must pop to List; got {:?}",
        state.stage,
    );
}
