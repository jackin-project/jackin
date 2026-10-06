// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth form opening.

use crate::tui::state::AuthForm;
use crate::tui::state::AuthFormFocus;

use crate::tui::state::AuthFormTarget;

use crate::tui::state::SettingsModal;

pub(crate) fn open_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &crate::tui::state::SettingsEnvState<'_>,
) {
    let _ = env;
    use jackin_config::AccountCredential;
    auth.editing_text = None;
    let existing = auth
        .pending
        .iter()
        .nth(auth.selected)
        .map(|(id, account)| (id.clone(), account.clone()));
    let (kind, mode, credential, folder) = if auth.selected
        == auth.pending.len() + crate::tui::screens::settings::model::ACCOUNT_KINDS.len()
    {
        auth.editing_account = None;
        let mode = match auth.github.auth_forward {
            jackin_config::GithubAuthMode::Sync => crate::tui::auth::AuthMode::Sync,
            jackin_config::GithubAuthMode::Token => crate::tui::auth::AuthMode::Token,
            jackin_config::GithubAuthMode::Ignore => crate::tui::auth::AuthMode::Ignore,
        };
        (
            crate::tui::auth::AuthKind::Github,
            mode,
            auth.github.env.get("GH_TOKEN").cloned(),
            None,
        )
    } else if let Some((id, account)) = existing {
        auth.editing_account = Some(id);
        let kind = crate::tui::screens::settings::model::account_kind(&account);
        match account.credential {
            AccountCredential::Profile { directory, .. } => (
                kind,
                crate::tui::auth::AuthMode::Sync,
                None,
                Some(directory),
            ),
            AccountCredential::ApiKey { value, .. } => {
                (kind, crate::tui::auth::AuthMode::ApiKey, Some(value), None)
            }
            AccountCredential::OAuthToken { value, .. } => (
                kind,
                crate::tui::auth::AuthMode::OAuthToken,
                Some(value),
                None,
            ),
        }
    } else {
        auth.editing_account = None;
        let Some(kind) = crate::tui::screens::settings::model::ACCOUNT_KINDS
            .get(auth.selected.saturating_sub(auth.pending.len()))
            .copied()
        else {
            return;
        };
        let mode = if matches!(
            kind,
            crate::tui::auth::AuthKind::Zai | crate::tui::auth::AuthKind::Minimax
        ) {
            crate::tui::auth::AuthMode::ApiKey
        } else {
            crate::tui::auth::AuthMode::Sync
        };
        (kind, mode, None, None)
    };
    auth.selected_kind = Some(kind);
    let form = AuthForm::from_existing(kind, mode, credential).with_source_folder(
        folder,
        Some(
            crate::tui::components::editor_rows::AuthSourceFolderDisplay {
                kind: crate::tui::components::editor_rows::AuthSourceFolderKind::Explicit,
                path: "Select a profile folder".to_owned(),
            },
        ),
    );
    let literal_buffer = form.literal_buffer();
    auth.set_modal(SettingsModal::AuthForm {
        target: AuthFormTarget::Workspace { kind },
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer,
    });
}

/// Source-folder validation callback used by the settings auth modal.
pub(crate) type SourceFolderValidator =
    dyn Fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;
