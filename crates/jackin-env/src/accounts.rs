// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Resolve credentials exclusively from assigned accounts.

use crate::{OpRunner, resolve_env_value};
use jackin_config::{AccountConfig, AccountCredential, AppConfig};
use jackin_core::{Agent, WorkspaceName};
use jackin_protocol::{AgentCredentialEnv, InstanceCredentialEnv};
use std::collections::{BTreeMap, BTreeSet};

// Codex/OpenCode consume the selected model from their private configuration,
// not from the credential environment. The CLI model override is applied
// after credentials are resolved, so the environment phase needs a
// non-empty validation marker when no instance-level model exists yet.
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

/// Environment names owned by account selection, including endpoint routing.
#[must_use]
pub fn is_account_env(name: &str) -> bool {
    jackin_core::is_account_env(name)
}

/// Resolve credentials for the admitted launch instances.
///
/// Instances are already authorized by [`jackin_config::resolve_launch`};
/// `_workspace`/`_role` only preserve the launch-stage call shape. Each
/// instance receives its own credential map keyed by its verbatim
/// `config_id`, including when several instances share one agent.
///
/// # Errors
/// Returns an error for unknown accounts, incompatible agent/provider
/// combinations, on-demand or empty credentials, and unavailable secrets.
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
        let account = config
            .accounts
            .get(&instance.account_id)
            .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
        let account =
            account_for_credential_resolution(account, instance.agent, instance.model.as_deref());
        let declarations =
            account.credential_env_for_instance(instance.agent, instance.base_url.as_deref())?;
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

#[cfg(test)]
mod tests;
