// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn env_config() -> SettingsEnvConfig<&'static str> {
    SettingsEnvConfig {
        env: BTreeMap::from([("GLOBAL".to_owned(), "x")]),
        roles: BTreeMap::from([
            (
                "alpha".to_owned(),
                BTreeMap::from([("ROLE_A".to_owned(), "x"), ("ROLE_B".to_owned(), "x")]),
            ),
            ("empty".to_owned(), BTreeMap::new()),
        ]),
    }
}

pub(super) fn scan_profile_account(directory: &str) -> jackin_config::AccountConfig {
    jackin_config::AccountConfig {
        enabled: true,
        name: "Claude default".into(),
        provider: jackin_config::AiProvider::Anthropic,
        credential: jackin_config::AccountCredential::Profile {
            agent: jackin_core::Agent::Claude,
            directory: directory.into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn scan_key_account(variable: &str) -> jackin_config::AccountConfig {
    jackin_config::AccountConfig {
        enabled: true,
        name: "anthropic API key".into(),
        provider: jackin_config::AiProvider::Anthropic,
        credential: jackin_config::AccountCredential::ApiKey {
            value: EnvValue::Plain(format!("${variable}")),
            base_url: None,
            model: None,
        },
    }
}

pub(super) fn scan_test_auth() -> SettingsAuthState<(), (), ()> {
    SettingsAuthState::from_accounts(BTreeMap::from([(
        "keep".to_owned(),
        scan_key_account("KEEP_KEY"),
    )]))
}
