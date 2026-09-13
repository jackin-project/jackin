// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn modal_key(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    code: KeyCode,
) -> SettingsAuthOutcome {
    handle_settings_auth_modal(
        auth,
        env,
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
        false,
        std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default())),
        ratatui::layout::Rect::new(0, 0, 100, 40),
        &|_, _| Ok(()),
    )
}

#[test]
fn cancelling_profile_folder_picker_restores_account_form() {
    let (mut auth, mut env) = state();
    open_settings_auth_form(&mut auth, &env);
    let browser = crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_from_home().unwrap(),
    );
    auth.push_auth_modal(SettingsModal::AuthSourceFolderPicker { state: browser });
    modal_key(&mut auth, &mut env, KeyCode::Esc);
    assert!(matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthForm { .. })
    ));
    assert!(auth.modals.parents().is_empty());
    assert!(auth.pending.is_empty());
}

#[test]
fn onepassword_commit_preserves_form_until_validation_completes() {
    let (mut auth, mut env) = state();
    open_settings_auth_form(&mut auth, &env);
    let mut picker = crate::tui::op_picker::OpPickerState {
        stage: jackin_oppicker::OpPickerStage::Field,
        load_state: jackin_oppicker::OpLoadState::Ready,
        selected_vault: Some(jackin_env::OpVault {
            id: "vault-id".into(),
            name: "Private".into(),
        }),
        selected_item: Some(jackin_env::OpItem {
            id: "item-id".into(),
            name: "Account".into(),
            subtitle: String::new(),
        }),
        fields: vec![jackin_env::OpField {
            id: "key-id".into(),
            label: "key".into(),
            reference: "op://Private/Account/key".into(),
            field_type: "concealed".into(),
            concealed: true,
        }],
        ..crate::tui::op_picker::OpPickerState::default()
    };
    picker.field_list_state.set_active(Some(0));
    auth.push_auth_modal(SettingsModal::AuthOpPicker {
        state: Box::new(picker),
    });
    let SettingsAuthOutcome::ValidateOpRef(reference) =
        modal_key(&mut auth, &mut env, KeyCode::Enter)
    else {
        panic!("expected credential validation");
    };
    assert!(!auth.has_modal());
    assert_eq!(auth.modals.parents().len(), 1);
    apply_op_picker_to_settings_auth_form_committed(&mut auth, reference);
    assert!(matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthForm {
            focus: AuthFormFocus::Save,
            ..
        })
    ));
    assert!(auth.modals.parents().is_empty());
}

#[test]
fn missing_settings_auth_return_path_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_missing_auth_return_path);

    export.force_flush();
    assert_eq!(export.event_count("error.typed"), 1);
    assert!(export.contains_log_text("telemetry_instrumentation_fault"));
    for private in ["token", "folder", "op ref", "modal", "path"] {
        assert!(!export.contains_log_text(private));
    }
}

fn state() -> (
    crate::tui::state::SettingsAuthState,
    crate::tui::state::SettingsEnvState<'static>,
) {
    let config = jackin_config::AppConfig::default();
    (
        crate::tui::state::SettingsAuthState::from_config(&config),
        crate::tui::state::SettingsEnvState::from_config(&config),
    )
}

#[test]
fn multiple_accounts_for_one_provider_remain_independent() {
    let (mut auth, mut env) = state();
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::auth::AuthMode::ApiKey,
        Some(jackin_core::EnvValue::Plain("first-secret".into())),
    );
    persist_settings_auth_form(&mut auth, &mut env, &form);
    auth.editing_account = None;
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::auth::AuthMode::ApiKey,
        Some(jackin_core::EnvValue::Plain("second-secret".into())),
    );
    persist_settings_auth_form(&mut auth, &mut env, &form);
    assert_eq!(auth.pending.len(), 2);
    assert_ne!(
        auth.pending["anthropic-1"].credential,
        auth.pending["anthropic-2"].credential
    );
    assert!(env.pending.env.is_empty());
    auth.selected = 0;
    auth.delete_selected_account();
    assert!(auth.pending.contains_key("anthropic-2"));
    assert_eq!(auth.pending.len(), 1);
}

#[test]
fn profile_account_records_exact_selected_directory() {
    let (mut auth, mut env) = state();
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Codex,
        crate::tui::auth::AuthMode::Sync,
        None,
    )
    .with_source_folder(Some("/accounts/codex-work".into()), None);
    persist_settings_auth_form(&mut auth, &mut env, &form);
    assert!(
        matches!(&auth.pending["openai-1"].credential, jackin_config::AccountCredential::Profile { agent: jackin_core::Agent::Codex, directory } if directory == std::path::Path::new("/accounts/codex-work"))
    );
}

#[test]
fn editing_api_key_preserves_identity_and_endpoint_settings() {
    let (mut auth, mut env) = state();
    auth.pending.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work account".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("before".into()),
                base_url: Some("https://example.test".into()),
                model: Some("custom-model".into()),
            },
        },
    );
    auth.selected = 0;
    open_settings_auth_form(&mut auth, &env);
    let Some(SettingsModal::AuthForm { mut state, .. }) = auth.take_modal() else {
        panic!("missing account form")
    };
    state.set_literal("after".into());
    persist_settings_auth_form(&mut auth, &mut env, &state);
    assert_eq!(auth.pending.len(), 1);
    assert_eq!(auth.pending["work"].name, "Work account");
    assert!(
        matches!(&auth.pending["work"].credential, jackin_config::AccountCredential::ApiKey { value, base_url, model } if value == &jackin_core::EnvValue::Plain("after".into()) && base_url.as_deref() == Some("https://example.test") && model.as_deref() == Some("custom-model"))
    );
}

#[test]
fn account_list_masks_credential_literals() {
    let (mut auth, mut env) = state();
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::auth::AuthMode::ApiKey,
        Some(jackin_core::EnvValue::Plain(
            "never-render-this-secret".into(),
        )),
    );
    persist_settings_auth_form(&mut auth, &mut env, &form);
    let lines = crate::tui::screens::settings::view::auth_state_lines(&auth, &env, true);
    let text = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<String>();
    assert!(!text.contains("never-render-this-secret"));
    assert!(text.contains("anthropic-1"));
}

#[test]
fn default_account_targets_only_requested_agent_and_disable_clears_default() {
    let (mut auth, mut env) = state();
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::auth::AuthMode::ApiKey,
        Some(jackin_core::EnvValue::Plain("synthetic-key".into())),
    );
    persist_settings_auth_form(&mut auth, &mut env, &form);
    auth.mark_saved();
    auth.toggle_account_default("anthropic-1", jackin_core::Agent::Claude)
        .unwrap();
    assert_eq!(auth.bindings.len(), 1);
    assert_eq!(auth.bindings[&jackin_core::Agent::Claude], "anthropic-1");
    assert!(auth.is_dirty());
    auth.toggle_selected_account_enabled();
    assert!(!auth.pending["anthropic-1"].enabled);
    assert!(auth.bindings.is_empty());
    let error = auth
        .toggle_account_default("anthropic-1", jackin_core::Agent::Claude)
        .unwrap_err();
    assert!(error.contains("enabled account"));
    open_settings_auth_form(&mut auth, &env);
    let Some(SettingsModal::AuthForm { state, .. }) = auth.take_modal() else {
        panic!("account form")
    };
    persist_settings_auth_form(&mut auth, &mut env, &state);
    assert!(
        !auth.pending["anthropic-1"].enabled,
        "credential editing must not enable account"
    );
    auth.discard();
    assert!(auth.pending["anthropic-1"].enabled);
    assert!(auth.bindings.is_empty());
}

#[test]
fn default_input_rejects_incompatible_agent_without_losing_input() {
    let (mut auth, mut env) = state();
    let form = AuthForm::from_existing(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::auth::AuthMode::Sync,
        None,
    )
    .with_source_folder(Some("/profiles/claude".into()), None);
    persist_settings_auth_form(&mut auth, &mut env, &form);
    auth.editing_account = Some("anthropic-1".into());
    auth.editing_text = Some(crate::tui::screens::settings::model::AccountTextField::DefaultAgent);
    auth.set_modal(SettingsModal::AuthTextInput {
        state: Box::new(crate::tui::components::TextInputState::new(
            "Default agent",
            "codex",
        )),
    });
    modal_key(&mut auth, &mut env, KeyCode::Enter);
    assert!(auth.bindings.is_empty());
    assert!(auth.take_error().is_some());
    assert!(matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthTextInput { .. })
    ));
}
