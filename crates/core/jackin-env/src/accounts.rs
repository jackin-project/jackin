// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{OpRunner, resolve_env_value};
use jackin_config::{AccountConfig, AccountCredential, AppConfig};
use jackin_core::{Agent, WorkspaceName};
use jackin_protocol::{AgentCredentialEnv, InstanceCredentialEnv};
use std::collections::{BTreeMap, BTreeSet};

const DEFERRED_MODEL: &str = "__jackin_model_deferred__";

fn account_for_credential_resolution(
    account: &AccountConfig,
    agent: Agent,
    model: Option<&str>,
) -> AccountConfig {
    let model =
        model.or_else(|| matches!(agent, Agent::Codex | Agent::Opencode).then_some(DEFERRED_MODEL));
    let Some(model) = model else {
        return account.clone();
    };
    let mut account = account.clone();
    if let AccountCredential::ApiKey {
        model: account_model,
        ..
    } = &mut account.credential
    {
        *account_model = Some(model.to_owned());
    }
    account
}

pub fn credential_env_declarations_for_instance(
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
) -> anyhow::Result<BTreeMap<String, jackin_core::EnvValue>> {
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    Ok(
        account_for_credential_resolution(account, instance.agent, instance.model.as_deref())
            .credential_env_for_instance(instance.agent, instance.base_url.as_deref())?,
    )
}

#[must_use]
pub fn is_account_env(name: &str) -> bool {
    jackin_core::is_account_env(name)
}

pub fn resolve_instance_env_with<R, H>(
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    _workspace: Option<&WorkspaceName>,
    _role: &str,
    runner: &R,
    host_env: H,
) -> anyhow::Result<AgentCredentialEnv>
where
    R: OpRunner + ?Sized,
    H: Fn(&str) -> Result<String, std::env::VarError> + Send + Sync,
{
    let mut resolved_instances = BTreeMap::new();
    let mut config_ids = BTreeSet::new();
    for instance in instances {
        anyhow::ensure!(
            config_ids.insert(instance.config_id.as_str()),
            "duplicate account instance id {:?}",
            instance.config_id
        );
    }

    for instance in instances {
        let declarations = credential_env_declarations_for_instance(config, instance)?;
        if declarations
            .values()
            .any(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
        {
            runner.probe()?;
        }
        let resolved = declarations
            .into_iter()
            .map(|(key, value)| {
                anyhow::ensure!(
                    !value.is_on_demand(),
                    "account credential {key} must resolve at launch"
                );
                let resolved =
                    resolve_env_value("selected account", &key, &value, runner, &host_env)?;
                anyhow::ensure!(
                    !resolved.trim().is_empty(),
                    "selected account credential {key} is empty"
                );
                Ok((key, resolved))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
        if !resolved.is_empty() {
            resolved_instances.insert(
                instance.config_id.clone(),
                InstanceCredentialEnv {
                    agent: instance.agent.slug().to_owned(),
                    account_id: instance.account_id.clone(),
                    env: resolved,
                },
            );
        }
    }
    Ok(AgentCredentialEnv::new(resolved_instances))
}
