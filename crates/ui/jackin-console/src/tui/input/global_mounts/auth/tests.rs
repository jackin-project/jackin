// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::SettingsAuthOutcome;
use super::*;
use crate::tui::auth::{AuthKind, AuthMode};
use crate::tui::components::auth_panel::{CredentialInput, auth_credential_input_state};
use crate::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, SettingsAuthState, SettingsModal,
};
use std::collections::BTreeMap;
use std::rc::Rc;

fn test_env() -> crate::tui::state::SettingsEnvState<'static> {
    crate::tui::state::SettingsEnvState::from_pending(crate::tui::state::SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    })
}

fn test_cache() -> Rc<std::cell::RefCell<jackin_env::OpCache>> {
    Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default()))
}

fn validate_ok(_kind: Option<AuthKind>, _path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

fn auth_form_modal(kind: AuthKind, form: AuthForm) -> SettingsModal<'static> {
    SettingsModal::AuthForm {
        target: AuthFormTarget::Workspace { kind },
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer: String::new(),
    }
}

#[test]
fn cancelling_profile_folder_picker_restores_account_form() {
    let mut auth: SettingsAuthState = SettingsAuthState::from_accounts(BTreeMap::new());
    let form = AuthForm::from_existing(AuthKind::Codex, AuthMode::Sync, None);
    let parent = auth_form_modal(AuthKind::Codex, form);
    auth.set_modal(parent);
    let parent = auth.take_modal().expect("parent auth form");
    let root = std::path::PathBuf::from("/tmp/jackin-auth-test");
    auth.open_child_modal(
        parent,
        SettingsModal::AuthSourceFolderPicker {
            state: crate::tui::components::file_browser::FileBrowserState::from_listing(
                crate::tui::components::file_browser::FolderListing {
                    root: root.clone(),
                    cwd: root,
                    entries: Vec::new(),
                },
            ),
        },
    );
    let mut env = test_env();
    let outcome = handle_settings_auth_modal(
        &mut auth,
        &mut env,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::empty(),
        ),
        false,
        test_cache(),
        ratatui::layout::Rect::new(0, 0, 80, 24),
        &validate_ok,
    );
    assert!(matches!(outcome, SettingsAuthOutcome::Continue));
    assert!(matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthForm { .. })
    ));
    assert!(auth.pending.is_empty());
    assert!(!auth.is_dirty());
}

#[test]
fn editing_api_key_preserves_identity_and_endpoint_settings() {
    let mut pending = BTreeMap::new();
    pending.insert(
        "codex-main".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Main".to_owned(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("old-secret".to_owned()),
                base_url: Some("https://api.example.invalid".to_owned()),
                model: Some("gpt-5".to_owned()),
            },
        },
    );
    let mut auth: SettingsAuthState = SettingsAuthState::from_accounts(pending);
    let form = AuthForm::from_existing(
        AuthKind::Codex,
        AuthMode::ApiKey,
        Some(jackin_core::EnvValue::Plain("old-secret".to_owned())),
    );
    auth.set_modal(auth_form_modal(AuthKind::Codex, form));
    auth.push_auth_modal(SettingsModal::AuthTextInput {
        state: Box::new(auth_credential_input_state("typed-secret")),
    });
    commit_settings_auth_text(&mut auth, "typed-secret".to_owned()).unwrap();
    let Some(SettingsModal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    }) = auth.modal_ref()
    else {
        panic!("auth form was not restored after text commit");
    };
    assert!(matches!(
        target,
        AuthFormTarget::Workspace {
            kind: AuthKind::Codex
        }
    ));
    assert_eq!(state.kind, AuthKind::Codex);
    assert_eq!(state.mode, Some(AuthMode::ApiKey));
    assert!(matches!(
        &state.credential,
        CredentialInput::Literal(value) if value == "typed-secret"
    ));
    assert!(matches!(focus, AuthFormFocus::Save));
    assert_eq!(literal_buffer, "typed-secret");
    let Some(account) = auth.pending.get("codex-main") else {
        panic!("edited account is missing");
    };
    assert!(matches!(
        &account.credential,
        jackin_config::AccountCredential::ApiKey {
            base_url: Some(_),
            model: Some(_),
            ..
        }
    ));
    assert!(!auth.is_dirty());
}
