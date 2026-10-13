// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_removal_clears_workspace_and_role_bindings() {
    let mut config = jackin_config::AppConfig::default();
    config.accounts.insert(
        "personal".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("test".into()),
                base_url: None,
                model: None,
            },
        },
    );
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_field = FieldFocus::Row(0);
    editor.edit_account_row(&config, false);
    editor
        .pending
        .account_bindings
        .insert(jackin_core::Agent::Claude, "personal".into());
    editor
        .pending
        .roles
        .entry("dev".into())
        .or_default()
        .account_bindings
        .insert(jackin_core::Agent::Claude, "personal".into());
    editor.edit_account_row(&config, false);
    assert!(editor.pending.accounts.is_empty());
    assert!(editor.pending.account_bindings.is_empty());
    assert!(editor.pending.roles["dev"].account_bindings.is_empty());
}

#[test]
fn account_binding_cycles_only_compatible_assigned_accounts() {
    let mut config = jackin_config::AppConfig::default();
    for (id, provider) in [
        ("anthropic", jackin_config::AiProvider::Anthropic),
        ("openai", jackin_config::AiProvider::OpenAi),
    ] {
        config.accounts.insert(
            id.into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: id.into(),
                provider,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: jackin_core::EnvValue::Plain("test".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.pending.accounts = vec!["anthropic".into(), "openai".into()];
    editor.active_field = FieldFocus::Row(2);
    editor.edit_account_row(&config, false);
    assert_eq!(
        editor.pending.account_bindings[&jackin_core::Agent::Claude],
        "anthropic"
    );
    assert!(editor.change_count() > 0);
    editor.edit_account_row(&config, false);
    assert!(editor.pending.account_bindings.is_empty());
}

#[test]
fn github_workspace_form_remains_independent_of_agent_accounts() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let target = crate::tui::state::AuthFormTarget::Workspace {
        kind: crate::tui::auth::AuthKind::Github,
    };
    let form = crate::tui::state::AuthForm::from_existing(
        crate::tui::auth::AuthKind::Github,
        crate::tui::auth::AuthMode::Token,
        Some(jackin_core::EnvValue::Plain("gh-test".into())),
    );
    editor.persist_auth_form(&target, &form);
    assert_eq!(
        editor.pending.github.as_ref().unwrap().auth_forward,
        jackin_config::GithubAuthMode::Token
    );
    assert!(editor.pending.accounts.is_empty());
    editor.clear_auth_form_layer(&target);
    assert!(editor.pending.github.is_none());
}

#[test]
fn role_binding_uses_assigned_account_and_survives_environment_removal() {
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert("dev".into(), RoleSource::default());
    config.accounts.insert(
        "personal".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("test".into()),
                base_url: None,
                model: None,
            },
        },
    );
    let mut workspace = WorkspaceConfig::default();
    workspace.accounts.push("personal".into());
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    let rows = editor.auth_flat_rows(&config);
    let index = rows.iter().position(|row| matches!(row, AuthRow::Binding { agent: jackin_core::Agent::Claude, role: Some(role) } if role == "dev")).unwrap();
    editor.active_field = FieldFocus::Row(index);
    editor.edit_account_row(&config, false);
    editor
        .pending
        .roles
        .get_mut("dev")
        .unwrap()
        .env
        .insert("TOKEN".into(), jackin_core::EnvValue::Plain("value".into()));
    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();
    assert_eq!(
        editor.pending.roles["dev"].account_bindings[&jackin_core::Agent::Claude],
        "personal"
    );
    assert!(editor.pending.account_bindings.is_empty());
}
