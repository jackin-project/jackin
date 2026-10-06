// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn s1_settings_account_draft_cancel_discards_apply_persists() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let config = jackin_config::ConfigEditor::open(&paths)
        .unwrap()
        .save()
        .unwrap();
    assert!(!config.accounts.contains_key("drafted"));
    let mut settings = crate::tui::state::SettingsState::from_config(&config);
    settings.auth.pending.insert(
        "drafted".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Draft".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: EnvValue::Plain("secret-drafted".into()),
                base_url: None,
                model: None,
            },
        },
    );

    // Cancel: discard the pending draft without saving; disk is untouched.
    settings.auth.pending.remove("drafted");
    let reloaded = jackin_config::ConfigEditor::open(&paths)
        .unwrap()
        .save()
        .unwrap();
    assert!(!reloaded.accounts.contains_key("drafted"));
    assert_eq!(reloaded.accounts, config.accounts);

    // Apply: the same pending draft persists through save_settings.
    settings.auth.pending.insert(
        "drafted".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Draft".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: EnvValue::Plain("secret-drafted".into()),
                base_url: None,
                model: None,
            },
        },
    );
    let saved = save_settings(
        &paths,
        SettingsSaveInput {
            mounts_original: &settings.mounts.original,
            mounts_pending: &settings.mounts.pending,
            env_original: &settings.env.original,
            env_pending: &settings.env.pending,
            auth_original: &settings.auth.original,
            auth_pending: &settings.auth.pending,
            github: &settings.auth.github,
            original_github: &settings.auth.original_github,
            bindings_pending: &settings.auth.bindings,
            bindings_original: &settings.auth.original_bindings,
            trust_pending: &settings.trust.pending,
            git_coauthor_trailer: settings.general.pending_coauthor_trailer,
            git_dco: settings.general.pending_dco,
        },
    )
    .unwrap();
    assert_eq!(saved.accounts["drafted"].name, "Draft");
    let persisted = jackin_config::ConfigEditor::open(&paths)
        .unwrap()
        .save()
        .unwrap();
    assert!(persisted.accounts.contains_key("drafted"));
}
