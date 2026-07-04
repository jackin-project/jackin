// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `global_mounts`.
use super::super::test_support::key;
use super::*;
use crate::tui::components::auth_panel::CredentialInput;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::state::{
    ManagerStage, ManagerState, SettingsEnvModal, SettingsEnvRow, SettingsEnvTextTarget,
    SettingsState, SettingsTab,
};
use jackin_config::{AppConfig, RoleSource};
use jackin_core::Agent;
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
                original_github_env: &settings.auth.original_github_env,
                github_env: &settings.auth.github_env,
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
                    .open_sub_modal(GlobalMountModal::FileBrowser {
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
        settings.mounts.modal,
        Some(GlobalMountModal::ScopePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modal,
        Some(GlobalMountModal::FileBrowser { .. })
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
        settings.mounts.modal,
        Some(GlobalMountModal::FileBrowser { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

    // The ScopePicker was committed when AllAgents was picked, so Esc
    // on the FileBrowser must close the modal chain entirely rather
    // than resurrect a consumed picker.
    assert!(
        settings.mounts.modal.is_none(),
        "Esc from add-mount FileBrowser should close the chain; got {:?}",
        settings.mounts.modal
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
    settings.mounts.modal = Some(GlobalMountModal::FileBrowser {
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
        settings.mounts.modal,
        Some(GlobalMountModal::FileBrowser { .. })
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
    let Some(GlobalMountModal::ScopePicker { state: picker }) = settings.mounts.modal.as_mut()
    else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modal,
        Some(GlobalMountModal::RolePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modal,
        Some(GlobalMountModal::FileBrowser { .. })
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
    let Some(GlobalMountModal::ScopePicker { state: picker }) = settings.mounts.modal.as_mut()
    else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
    assert!(matches!(
        settings.mounts.modal,
        Some(GlobalMountModal::RolePicker { .. })
    ));

    confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

    assert!(
        settings.mounts.modal.is_none(),
        "Esc from global-mount RolePicker should close the chain; got {:?}",
        settings.mounts.modal
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
fn auth_tab_mode_row_ignores_space_and_enter_opens_form() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    handle_settings_key(&mut state, key(KeyCode::Enter));
    handle_settings_key(&mut state, key(KeyCode::Char(' ')));

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.auth.pending[0].mode,
        crate::tui::auth::AuthMode::Sync
    );
    assert!(!settings.auth.is_dirty());
    assert!(settings.auth.modal.is_none());

    handle_settings_key(&mut state, key(KeyCode::Enter));

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(matches!(
        settings.auth.modal,
        Some(SettingsAuthModal::AuthForm { .. })
    ));
}

/// `g` on the global Claude `oauth_token` auth form opens the
/// shared source picker (plain vs. 1Password) and arms
/// `generating_token`, driving the global token-generate (mint)
/// path. The storage-target choice happens at the source picker.
#[test]
fn settings_auth_generate_opens_source_picker_and_arms_flag() {
    use crate::tui::auth::{AuthKind, AuthMode};

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Claude);
    open_settings_auth_form(&mut settings.auth, &settings.env);
    // Drive the mode to OAuthToken so the generate gate holds.
    let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut() else {
        panic!("auth form must be open");
    };
    form.set_mode(AuthMode::OAuthToken);
    assert!(settings_auth_can_generate_token(&settings.auth));

    let op_cache = std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()));
    let mut pending = None;
    handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Char('g')),
        true,
        op_cache,
        Rect::new(0, 0, 120, 40),
        &|_, _| Ok(()),
    );

    assert!(
        matches!(
            settings.auth.modal,
            Some(SettingsAuthModal::SourcePicker { .. })
        ),
        "generate must open the source picker as the first step"
    );
    assert!(
        !settings.auth.modal_parents.is_empty(),
        "generate must stash the form so the post-mint re-mount can return to it; \
             generate vs. provide is disambiguated by the generate flag, not the stash"
    );
    assert!(
        settings.auth.generating_token,
        "generate must arm the global token-generate flag"
    );
    assert!(
        pending.is_none(),
        "no mint request is built until the source/picker commits"
    );
}

/// After the settings `g`/`G` generate stashes the form, the mint
/// completion re-mounts the global Claude Edit-auth dialog with the
/// minted op credential applied and focus on Save — the shape the
/// `run_console` loop drives via `apply_op_picker_to_settings_auth_form`.
/// Nothing is persisted here; the operator's Save does that. Uses an
/// injected stub `OpRunner` so no real `op` binary runs.
#[test]
fn settings_auth_generate_op_mint_remounts_form_focus_save() {
    use crate::tui::auth::{AuthKind, AuthMode};
    use jackin_core::OpRef;
    use jackin_env::OpRunner;

    struct StubRunner;
    impl OpRunner for StubRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Ok("sk-ant-oat01-MINTED".into())
        }
    }

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Claude);
    open_settings_auth_form(&mut settings.auth, &settings.env);
    let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut() else {
        panic!("auth form must be open");
    };
    form.set_mode(AuthMode::OAuthToken);

    // Press `g` to start generate (stashes the form).
    let op_cache = std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()));
    let mut pending = None;
    handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Char('g')),
        true,
        op_cache,
        Rect::new(0, 0, 120, 40),
        &|_, _| Ok(()),
    );
    assert!(!settings.auth.modal_parents.is_empty());

    // Simulate the loop's post-mint re-mount with the wired OpRef.
    let minted = OpRef {
        op: "op://uuid/claude-vault".into(),
        path: "Personal/Claude/oauth-token".into(),
        account: None,
        on_demand: false,
    };
    apply_op_picker_to_settings_auth_form_with_runner(
        &mut settings.auth,
        minted.clone(),
        &StubRunner,
    );

    let Some(SettingsAuthModal::AuthForm { state, focus, .. }) = &settings.auth.modal else {
        panic!("mint completion must re-mount the settings auth form");
    };
    assert_eq!(
        focus,
        &AuthFormFocus::Save,
        "post-mint re-mount drops the cursor onto Save"
    );
    match &state.credential {
        CredentialInput::OpRef(r) => assert_eq!(r, &minted),
        other => panic!("expected OpRef credential after mint; got {other:?}"),
    }
    assert!(settings.auth.modal_parents.is_empty());
    assert!(
        pending.is_none(),
        "the mint request was already drained by the loop; none re-queued"
    );
}

/// `g` is a no-op on the global Claude form when the mode is not
/// `oauth_token` (here ApiKey): the auth form stays open and the
/// generate flag is not armed.
#[test]
fn settings_auth_generate_is_noop_for_non_oauth_token_mode() {
    use crate::tui::auth::{AuthKind, AuthMode};

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Claude);
    open_settings_auth_form(&mut settings.auth, &settings.env);
    let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut() else {
        panic!("auth form must be open");
    };
    form.set_mode(AuthMode::ApiKey);
    assert!(!settings_auth_can_generate_token(&settings.auth));

    let op_cache = std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()));
    let mut pending = None;
    handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Char('g')),
        true,
        op_cache,
        Rect::new(0, 0, 120, 40),
        &|_, _| Ok(()),
    );

    assert!(matches!(
        settings.auth.modal,
        Some(SettingsAuthModal::AuthForm { .. })
    ));
    assert!(!settings.auth.generating_token);
    assert!(pending.is_none());
}

#[test]
fn settings_auth_dialog_source_folder_stages_and_save_persists_global_kimi() {
    use crate::tui::auth::AuthKind;
    use crate::tui::components::file_browser::FileBrowserState;

    let tmp = tempfile::tempdir().unwrap();
    let source_dir = tmp.path().join("kimi-home");
    std::fs::create_dir(&source_dir).unwrap();
    // Seed a valid Kimi credential structure so source-folder validation
    // accepts the pick (config.toml + a credentials/ directory).
    std::fs::write(source_dir.join("config.toml"), "x = 1\n").unwrap();
    std::fs::create_dir(source_dir.join("credentials")).unwrap();
    // The file browser commits the symlink-resolved path (on macOS the
    // temp root /var is a symlink to /private/var), so compare against the
    // canonical form.
    let expected_dir = std::fs::canonicalize(&source_dir).unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Kimi);
    open_settings_auth_form(&mut settings.auth, &settings.env);
    let Some(SettingsAuthModal::AuthForm { state, .. }) = settings.auth.modal.take() else {
        panic!("auth form must be open");
    };
    assert!(state.shows_source_folder());
    settings
        .auth
        .modal_parents
        .push(SettingsAuthModal::AuthForm {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Kimi,
            },
            state,
            focus: AuthFormFocus::SourceFolder,
            literal_buffer: String::new(),
        });
    settings.auth.modal = Some(SettingsAuthModal::SourceFolderPicker {
        state: FileBrowserState::from_listing(crate::services::file_browser::listing_at(
            tmp.path().to_path_buf(),
            source_dir.clone(),
        )),
    });

    let op_cache = std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()));
    let mut pending = None;
    let outcome = handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Char('s')),
        true,
        std::rc::Rc::clone(&op_cache),
        Rect::new(0, 0, 120, 40),
        &|_, _| Ok(()),
    );
    assert!(matches!(
        outcome,
        SettingsAuthOutcome::ApplyFileBrowserOutcome(
            crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_)
        )
    ));

    let mut state = ManagerState::from_config(&config, tmp.path());
    state.stage = ManagerStage::Settings(settings);
    crate::tui::file_browser::apply_file_browser_commit_result(
        &mut state,
        crate::tui::file_browser::FileBrowserCommitResult::Accepted {
            context: crate::tui::effect::FileBrowserEffectContext::SettingsAuth,
            path: expected_dir.clone(),
        },
    );
    let ManagerStage::Settings(settings) = &mut state.stage else {
        panic!("expected settings stage");
    };

    let Some(SettingsAuthModal::AuthForm { state, focus, .. }) = &settings.auth.modal else {
        panic!("source folder commit must return to auth form");
    };
    assert_eq!(focus, &AuthFormFocus::Save);
    assert_eq!(state.source_folder.as_deref(), Some(expected_dir.as_path()));

    handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Enter),
        true,
        op_cache,
        Rect::new(0, 0, 120, 40),
        &|_, _| Ok(()),
    );
    assert_eq!(
        settings
            .auth
            .pending
            .iter()
            .find(|row| row.kind == AuthKind::Kimi)
            .and_then(|row| row.sync_source_dir.as_deref()),
        Some(expected_dir.as_path())
    );

    let saved = crate::services::config_save::save_settings(
        &paths,
        crate::services::config_save::SettingsSaveInput {
            mounts_original: &settings.mounts.original,
            mounts_pending: &settings.mounts.pending,
            env_original: &settings.env.original,
            env_pending: &settings.env.pending,
            auth_pending: &settings.auth.pending,
            original_github_env: &settings.auth.original_github_env,
            github_env: &settings.auth.github_env,
            trust_pending: &settings.trust.pending,
            git_coauthor_trailer: settings.general.pending_coauthor_trailer,
            git_dco: settings.general.pending_dco,
        },
    )
    .unwrap();
    assert_eq!(
        saved.sync_source_dir_for(Agent::Kimi).as_deref(),
        Some(expected_dir.as_path())
    );
}

/// Committing a folder that lacks the agent's credentials must reject the
/// pick: raise the standard error dialog (via `auth.error`) and keep the
/// source-folder picker open so the operator can choose another folder,
/// rather than staging an invalid source dir.
#[test]
fn settings_auth_dialog_invalid_source_folder_keeps_picker_open_and_sets_error() {
    use crate::tui::auth::AuthKind;
    use crate::tui::components::file_browser::FileBrowserState;

    let tmp = tempfile::tempdir().unwrap();
    // An empty folder: no Kimi `config.toml` + `credentials/`, so validation
    // must reject it.
    let source_dir = tmp.path().join("not-kimi");
    std::fs::create_dir(&source_dir).unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Kimi);
    open_settings_auth_form(&mut settings.auth, &settings.env);
    let Some(SettingsAuthModal::AuthForm { state, .. }) = settings.auth.modal.take() else {
        panic!("auth form must be open");
    };
    settings
        .auth
        .modal_parents
        .push(SettingsAuthModal::AuthForm {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Kimi,
            },
            state,
            focus: AuthFormFocus::SourceFolder,
            literal_buffer: String::new(),
        });
    settings.auth.modal = Some(SettingsAuthModal::SourceFolderPicker {
        state: FileBrowserState::from_listing(crate::services::file_browser::listing_at(
            tmp.path().to_path_buf(),
            source_dir.clone(),
        )),
    });

    let op_cache = std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()));
    let mut pending = None;
    let outcome = handle_settings_auth_modal(
        &mut settings.auth,
        &mut settings.env,
        &mut pending,
        key(KeyCode::Char('s')),
        true,
        op_cache,
        Rect::new(0, 0, 120, 40),
        &|_, _| Err("missing credentials directory".to_owned()),
    );
    assert!(matches!(
        outcome,
        SettingsAuthOutcome::ApplyFileBrowserOutcome(
            crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_)
        )
    ));

    let mut state = ManagerState::from_config(&config, tmp.path());
    state.stage = ManagerStage::Settings(settings);
    crate::tui::file_browser::apply_file_browser_commit_result(
        &mut state,
        crate::tui::file_browser::FileBrowserCommitResult::Rejected {
            context: crate::tui::effect::FileBrowserEffectContext::SettingsAuth,
            reason: "missing credentials directory".to_owned(),
        },
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };

    assert!(
        settings.auth.error.is_some(),
        "rejecting an invalid source folder must surface the error dialog"
    );
    assert!(
        matches!(
            settings.auth.modal,
            Some(SettingsAuthModal::SourceFolderPicker { .. })
        ),
        "the picker must stay open after a rejected folder"
    );
    assert!(pending.is_none());
}

#[test]
fn settings_auth_dialog_source_folder_row_is_generic_for_codex() {
    use crate::tui::auth::AuthKind;

    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_tab_bar_focused(false);
    settings.auth.selected_kind = Some(AuthKind::Codex);
    open_settings_auth_form(&mut settings.auth, &settings.env);

    let Some(SettingsAuthModal::AuthForm { state, .. }) = &settings.auth.modal else {
        panic!("auth form must be open");
    };
    assert!(state.shows_source_folder());
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
        settings.env.modal,
        Some(SettingsEnvModal::ScopePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modal,
        Some(SettingsEnvModal::Text {
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
        settings.env.modal,
        Some(SettingsEnvModal::Text {
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
        settings.env.modal.is_none(),
        "Esc from settings env key input should close the chain; got {:?}",
        settings.env.modal
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
    commit_env_text(&mut settings.env, &target, "API_KEY");
    assert!(matches!(
        settings.env.modal,
        Some(SettingsEnvModal::SourcePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Esc),
        std::rc::Rc::clone(&state.op_cache),
    );

    assert!(
        matches!(
            settings.env.modal,
            Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc from settings env SourcePicker should restore key input; got {:?}",
        settings.env.modal
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
    let Some(SettingsEnvModal::ScopePicker { state: picker }) = settings.env.modal.as_mut() else {
        panic!("expected scope picker");
    };
    picker.focused = crate::tui::components::scope_picker::ScopeChoice::SpecificAgent;
    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        settings.env.modal,
        Some(SettingsEnvModal::RolePicker { .. })
    ));

    handle_settings_env_modal(
        &mut settings.env,
        key(KeyCode::Enter),
        std::rc::Rc::clone(&state.op_cache),
    );
    assert!(matches!(
        &settings.env.modal,
        Some(SettingsEnvModal::Text {
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
