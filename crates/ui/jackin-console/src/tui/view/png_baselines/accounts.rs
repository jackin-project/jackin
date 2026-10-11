// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account-management baseline builders.
#![cfg(test)]

use super::*;
use std::path::PathBuf;

use crate::tui::state::{EditorState, EditorTab, ManagerStage, ManagerState, SettingsState};
use jackin_config::AppConfig;

pub(crate) fn account_config() -> AppConfig {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};
    use jackin_core::{Agent, EnvValue};
    let mut config = populated_config();
    for (id, name, credential) in [
        (
            "anthropic-personal",
            "Personal Claude",
            AccountCredential::Profile {
                agent: Agent::Claude,
                directory: "/profiles/claude-personal".into(),
                xdg_roots: None,
                source_selector: None,
            },
        ),
        (
            "anthropic-work",
            "Work Claude",
            AccountCredential::Profile {
                agent: Agent::Claude,
                directory: "/profiles/claude-work".into(),
                xdg_roots: None,
                source_selector: None,
            },
        ),
        (
            "anthropic-api",
            "Team API",
            AccountCredential::ApiKey {
                value: EnvValue::Plain("synthetic-review-key".into()),
                base_url: Some("https://api.example.invalid".into()),
                model: Some("team-model".into()),
            },
        ),
    ] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: name.into(),
                provider: AiProvider::Anthropic,
                credential,
            },
        );
    }
    config
        .accounts
        .get_mut("anthropic-personal")
        .unwrap()
        .enabled = false;
    config
        .account_bindings
        .insert(Agent::Claude, "anthropic-work".into());
    let workspace = config.workspaces.get_mut("alpha").unwrap();
    workspace.accounts = vec!["anthropic-work".into(), "anthropic-api".into()];
    workspace
        .account_bindings
        .insert(Agent::Claude, "anthropic-work".into());
    config
}

pub(crate) fn settings_accounts_populated() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = account_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    state.stage = ManagerStage::Settings(settings);
    (state, config, cwd)
}

pub(crate) fn workspace_accounts_assigned() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = account_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), config.workspaces["alpha"].clone());
    editor.active_tab = EditorTab::Auth;
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

pub(crate) fn settings_account_form(api: bool) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = settings_accounts_populated();
    let ManagerStage::Settings(settings) = &mut state.stage else {
        unreachable!()
    };
    let kind = crate::tui::auth::AuthKind::Claude;
    let form = if api {
        crate::tui::state::AuthForm::from_existing(
            kind,
            crate::tui::auth::AuthMode::ApiKey,
            Some(jackin_core::EnvValue::Plain("synthetic-review-key".into())),
        )
    } else {
        crate::tui::state::AuthForm::from_existing(kind, crate::tui::auth::AuthMode::Sync, None)
            .with_source_folder(Some("/profiles/claude-work".into()), None)
    };
    settings
        .auth
        .set_modal(crate::tui::state::SettingsModal::AuthForm {
            target: crate::tui::state::AuthFormTarget::Workspace { kind },
            state: Box::new(form),
            focus: crate::tui::state::AuthFormFocus::Save,
            literal_buffer: String::new(),
        });
    (state, config, cwd)
}

pub(crate) fn settings_account_api_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_account_form(true)
}
pub(crate) fn settings_account_profile_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_account_form(false)
}

pub(crate) fn account_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.inline_account_picker = Some(crate::tui::state::AccountPickerState::new(
        "jackin-alpha".into(),
        jackin_core::Agent::Claude,
        vec![crate::services::launch::AccountChoice {
            id: "anthropic-work".into(),
            name: "Work".into(),
            provider: jackin_config::AiProvider::Anthropic,
            agents: vec![jackin_core::Agent::Claude],
            configuration_id: None,
            instance_id: None,
        }],
    ));
    (state, config, cwd)
}

// ── Inventory ──────────────────────────────────────────────────────────────
