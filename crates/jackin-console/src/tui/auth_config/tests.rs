// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::auth::AuthMode;
use jackin_core::{ANTHROPIC_API_KEY_ENV_NAME, MINIMAX_API_KEY_ENV_NAME, ZAI_API_KEY_ENV_NAME};

#[test]
fn auth_kind_agent_returns_none_for_github() {
    assert_eq!(auth_kind_agent(AuthKind::Github), None);
    assert_eq!(auth_kind_agent(AuthKind::Claude), Some(Agent::Claude));
    assert_eq!(auth_kind_agent(AuthKind::Codex), Some(Agent::Codex));
    assert_eq!(auth_kind_agent(AuthKind::Amp), Some(Agent::Amp));
    assert_eq!(auth_kind_agent(AuthKind::Kimi), Some(Agent::Kimi));
    assert_eq!(auth_kind_agent(AuthKind::Opencode), Some(Agent::Opencode));
    assert_eq!(auth_kind_agent(AuthKind::Grok), Some(Agent::Grok));
}

#[test]
fn env_display_map_without_auth_credentials_hides_known_secret_keys() {
    let mut values = BTreeMap::new();
    values.insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));
    values.insert(
        ANTHROPIC_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("secret".into()),
    );
    values.insert("PROJECT_ENV".to_owned(), EnvValue::Plain("visible".into()));

    let display = env_display_map_without_auth_credentials(&values);

    assert_eq!(display.len(), 1);
    assert_eq!(display.get("PROJECT_ENV"), Some(&"visible".to_owned()));
    assert!(!display.contains_key("GH_TOKEN"));
    assert!(!display.contains_key(ANTHROPIC_API_KEY_ENV_NAME));
}

#[test]
fn auth_credential_env_keys_includes_settings_mode_credentials() {
    let keys = auth_credential_env_keys();

    assert!(keys.contains("GH_TOKEN"));
    assert!(keys.contains(ANTHROPIC_API_KEY_ENV_NAME));
    assert!(keys.contains(ZAI_API_KEY_ENV_NAME));
    assert!(keys.contains(MINIMAX_API_KEY_ENV_NAME));
}

#[test]
fn provider_only_kinds_have_no_coding_agent() {
    assert_eq!(auth_kind_agent(AuthKind::Zai), None);
    assert_eq!(auth_kind_agent(AuthKind::Minimax), None);
}

#[test]
fn every_account_owned_environment_name_is_hidden_from_general_display() {
    let mut values: BTreeMap<String, EnvValue> = jackin_core::account_env_names()
        .map(|key| (key.to_owned(), EnvValue::Plain("private-value".into())))
        .collect();
    values.insert("PROJECT_ENV".into(), EnvValue::Plain("visible".into()));
    let display = env_display_map_without_auth_credentials(&values);
    assert_eq!(
        display,
        BTreeMap::from([("PROJECT_ENV".into(), "visible".into())])
    );
}

#[test]
fn source_form_traits_keep_profile_and_key_modes_distinct() {
    let mut form = AuthForm::<EnvValue>::new(AuthKind::Claude);
    form.set_mode(AuthMode::Sync);
    assert!(form.shows_auth_source_folder());
    assert_eq!(form.required_credential_env_var(), None);
    form.set_auth_source_folder(PathBuf::from("/profiles/work"));
    assert_eq!(
        form.commit().unwrap().source_folder,
        Some(PathBuf::from("/profiles/work"))
    );
    form.set_mode(AuthMode::ApiKey);
    assert!(!form.shows_auth_source_folder());
    assert_eq!(
        form.required_credential_env_var(),
        Some(ANTHROPIC_API_KEY_ENV_NAME)
    );
    form.set_auth_literal("synthetic-key".into());
    assert_eq!(
        form.commit().unwrap().env_value,
        Some(EnvValue::Plain("synthetic-key".into()))
    );
}

#[test]
fn non_auth_environment_display_preserves_plain_values() {
    let values = BTreeMap::from([("PROJECT_ENV".into(), EnvValue::Plain("visible".into()))]);
    assert_eq!(
        env_display_map(&values),
        BTreeMap::from([("PROJECT_ENV".into(), "visible".into())])
    );
}
