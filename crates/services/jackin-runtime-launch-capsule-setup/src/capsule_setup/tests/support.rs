// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn api_key_account(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "fixture-key".into(),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

pub(super) fn instance(
    config_id: &str,
    agent: Agent,
    account_id: &str,
) -> jackin_config::ResolvedInstance {
    jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: account_id.into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: config_id.into(),
        synthesized: true,
    }
}
