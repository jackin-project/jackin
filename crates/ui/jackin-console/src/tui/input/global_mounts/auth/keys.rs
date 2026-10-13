// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth tab key handling.

use super::open_settings_auth_form;

use crate::tui::screens::settings::update::SettingsAuthKeyPlan;
use crate::tui::state::GlobalMountConfirm;
use crate::tui::state::ManagerStage;
use crate::tui::state::ManagerState;
use crate::tui::state::update::ManagerMessage;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;

use crate::tui::state::SettingsModal;

use super::super::confirm_modal;
use super::super::dispatch_manager;
use super::super::open_settings_save_preview;
use crate::tui::screens::settings::update as settings_update;

pub(crate) fn record_missing_auth_return_path() {
    let _recorded = jackin_telemetry::record_error(
        jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
    );
}

pub(crate) fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    if matches!(key.code, KeyCode::Delete | KeyCode::Char('d' | 'D')) {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings.auth.delete_selected_account();
        }
        return;
    }
    if matches!(key.code, KeyCode::Char('e' | 'E')) {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings.auth.toggle_selected_account_enabled();
        }
        return;
    }
    use crate::tui::screens::settings::model::AccountTextField;
    let text_field = match key.code {
        KeyCode::Char('f' | 'F') => Some(AccountTextField::DefaultAgent),
        KeyCode::Char('r' | 'R') => Some(AccountTextField::Name),
        KeyCode::Char('b' | 'B') => Some(AccountTextField::BaseUrl),
        KeyCode::Char('m' | 'M') => Some(AccountTextField::Model),
        _ => None,
    };
    if let Some(field) = text_field {
        if let ManagerStage::Settings(settings) = &mut state.stage
            && let Some((id, account)) = settings.auth.pending.iter().nth(settings.auth.selected)
        {
            let (label, value) = match (field, &account.credential) {
                (AccountTextField::DefaultAgent, _) if account.enabled => (
                    "Toggle default for agent (claude/codex/amp/kimi/opencode/grok)",
                    jackin_core::Agent::ALL
                        .iter()
                        .copied()
                        .find(|agent| account.supports_agent(*agent))
                        .map_or_else(String::new, |agent| agent.slug().to_owned()),
                ),
                (AccountTextField::Name, _) => ("Account name", account.name.clone()),
                (
                    AccountTextField::BaseUrl,
                    jackin_config::AccountCredential::ApiKey { base_url, .. },
                ) => (
                    "API base URL (empty for default)",
                    base_url.clone().unwrap_or_default(),
                ),
                (
                    AccountTextField::Model,
                    jackin_config::AccountCredential::ApiKey { model, .. },
                ) => (
                    "Model (empty for agent default)",
                    model.clone().unwrap_or_default(),
                ),
                _ => return,
            };
            settings.auth.editing_account = Some(id.clone());
            settings.auth.editing_text = Some(field);
            settings.auth.set_modal(SettingsModal::AuthTextInput {
                state: Box::new(crate::tui::components::TextInputState::new(label, value)),
            });
        }
        return;
    }
    if matches!(key.code, KeyCode::Enter)
        && settings_update::settings_auth_scan_row_selected(
            settings.auth.pending.len(),
            settings.auth.selected,
        )
    {
        dispatch_manager(
            state,
            ManagerMessage::Settings(
                crate::tui::screens::settings::message::SettingsMessage::RequestAccountScan,
            ),
        );
        return;
    }
    let plan = settings_update::settings_auth_key_plan(key.code, settings.is_dirty(), false, true);
    match plan {
        SettingsAuthKeyPlan::ClearKind => {
            dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
        }
        SettingsAuthKeyPlan::MoveSelection { delta } => {
            dispatch_manager(state, ManagerMessage::MoveSettingsAuthSelection { delta });
        }
        SettingsAuthKeyPlan::EnterKind => {
            if let ManagerStage::Settings(settings) = &mut state.stage {
                open_settings_auth_form(&mut settings.auth, &settings.env);
            }
        }
        SettingsAuthKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            if settings.is_dirty() {
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            }
        }
        SettingsAuthKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsAuthKeyPlan::OpenForm => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_auth_form(&mut settings.auth, &settings.env);
        }
        SettingsAuthKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsAuthKeyPlan::Noop => {}
    }
}
