// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth form application.

use super::record_missing_auth_return_path;

use crate::tui::state::AuthFormFocus;

use crate::tui::state::SettingsModal;

pub(crate) fn commit_settings_auth_text(
    auth: &mut crate::tui::state::SettingsAuthState,
    value: String,
) -> Result<(), String> {
    use crate::tui::screens::settings::model::AccountTextField;
    let Some(field) = auth.editing_text else {
        apply_plain_text_to_settings_auth_form(auth, &value);
        return Ok(());
    };
    let id = auth
        .editing_account
        .clone()
        .ok_or_else(|| "Account no longer exists".to_owned())?;
    if field == AccountTextField::DefaultAgent {
        let agent = jackin_core::Agent::from_slug(value.trim())
            .ok_or_else(|| "Unknown coding agent".to_owned())?;
        auth.toggle_account_default(&id, agent)?;
        auth.editing_text = None;
        return Ok(());
    }
    let account = auth
        .pending
        .get_mut(&id)
        .ok_or_else(|| "Account no longer exists".to_owned())?;
    match field {
        AccountTextField::DefaultAgent => unreachable!("default handled before metadata"),
        AccountTextField::Name => {
            if value.trim().is_empty() {
                return Err("Account name cannot be empty".into());
            }
            account.name = value;
        }
        AccountTextField::BaseUrl | AccountTextField::Model => {
            let jackin_config::AccountCredential::ApiKey {
                base_url, model, ..
            } = &mut account.credential
            else {
                return Err("Endpoint and model require an API account".into());
            };
            let target = if field == AccountTextField::BaseUrl {
                base_url
            } else {
                model
            };
            *target = (!value.trim().is_empty()).then(|| value.trim().to_owned());
        }
    }
    auth.editing_text = None;
    Ok(())
}

/// Translate a Create-mode `OpPicker` commit into a global
/// [`PendingTokenGenerate`](crate::tui::state::PendingTokenGenerate)
/// request that the `run_console` loop drains to mint the token.
/// `Existing` cannot occur in Create mode; a Cancel (or stray
/// `Existing`) just closes the chain. On `Continue` the picker is still
/// drilling, so the marker stays armed and the modal stays open.
pub(crate) fn restore_settings_auth_form(auth: &mut crate::tui::state::SettingsAuthState) {
    auth.restore_pending_auth_form();
}

/// Restore the account form with the supplied credential staged for save.
pub fn apply_plain_text_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    value: &str,
) {
    let Some(SettingsModal::AuthForm {
        target, mut state, ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_literal(value.to_owned());
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_owned(),
    });
}

pub(crate) fn apply_source_folder_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    path: std::path::PathBuf,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_source_folder(path);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Apply a committed op picker selection to the settings auth form after the
/// 1Password read has already succeeded on the `spawn_blocking` thread. Called
/// from the `run_console` poll loop — the read was verified asynchronously so
/// Touch ID / the 1Password desktop dialog did not freeze the TUI reactor.
///
/// The auth form is on `auth.modal_parents` — pop it, set the `OpRef` without
/// re-reading, and re-mount with focus on Save.
pub fn apply_op_picker_to_settings_auth_form_committed(
    auth: &mut crate::tui::state::SettingsAuthState,
    op_ref: jackin_core::OpRef,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    // The read already succeeded; set the ref directly without re-reading.
    state.set_op_ref(op_ref);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}
