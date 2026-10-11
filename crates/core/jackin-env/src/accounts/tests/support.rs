// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
struct NoSecrets;

impl OpRunner for NoSecrets {
    fn read(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("unexpected secret lookup")
    }
}

fn account_with_model(value: EnvValue, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::ApiKey {
            value,
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

fn account(value: &str) -> AccountConfig {
    account_with_model(EnvValue::from(value), None)
}

fn configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

fn configuration_with_endpoint(
    agent: Agent,
    account: &str,
    model: Option<&str>,
    base_url: &str,
) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: model.map(str::to_owned),
        base_url: Some(base_url.into()),
        display_label: None,
        invoked_via_wrapper: None,
    }
}

fn launch(cfg: &AppConfig, ids: &[&str]) -> Vec<jackin_config::ResolvedInstance> {
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    jackin_config::resolve_launch(cfg, None, "role", Some(&ids), None).unwrap()
}

