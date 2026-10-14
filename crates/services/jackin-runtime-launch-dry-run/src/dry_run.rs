// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AppConfig, ResolvedInstance};
use jackin_core::{Agent, WorkspaceName};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DryRunIdentity {
    pub account_id: Option<String>,

    pub model: Option<String>,

    pub instances: Vec<ResolvedInstance>,

    pub admitted_instances: Vec<ResolvedInstance>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DryRunModelProjection {
    pub model: Option<String>,

    pub instances: std::collections::BTreeMap<String, String>,
}

pub fn resolve_dry_run_identity(
    plan_config: &AppConfig,
    selected_agent: Agent,
    workspace: Option<&WorkspaceName>,
    role_key: &str,
    explicit_account_pick: bool,
) -> anyhow::Result<DryRunIdentity> {
    if explicit_account_pick {
        let selected =
            jackin_config::resolve_account(plan_config, selected_agent, workspace, role_key)?;
        let account_id = selected.and_then(|account| {
            plan_config
                .accounts
                .iter()
                .find(|(_, known)| std::ptr::eq(*known, account))
                .map(|(id, _)| id.clone())
        });
        let model = account_id
            .as_ref()
            .and_then(|id| plan_config.accounts.get(id).and_then(account_model));
        let admitted_instances = jackin_config::resolve_launch(
            plan_config,
            workspace,
            role_key,
            None,
            Some(selected_agent),
        )?;
        return Ok(DryRunIdentity {
            account_id,
            model,
            instances: Vec::new(),
            admitted_instances,
        });
    }
    let instances = jackin_config::resolve_launch(
        plan_config,
        workspace,
        role_key,
        None,
        Some(selected_agent),
    )?;
    if let [single] = instances.as_slice()
        && single.synthesized
    {
        return Ok(DryRunIdentity {
            account_id: Some(single.account_id.clone()),
            model: single.model.clone(),
            instances: Vec::new(),
            admitted_instances: instances,
        });
    }
    Ok(DryRunIdentity {
        account_id: None,
        model: None,
        admitted_instances: instances.clone(),
        instances,
    })
}

pub fn resolve_dry_run_model_projection(
    config: &AppConfig,
    role_models: &std::collections::BTreeMap<Agent, String>,
    identity: &DryRunIdentity,
    selected_agent: Agent,
    model_override: Option<&str>,
) -> anyhow::Result<DryRunModelProjection> {
    let instances =
        jackin_runtime_launch_capsule_setup::capsule_setup::resolved_instance_models_from_role_models(
            config,
            role_models,
            &identity.admitted_instances,
            selected_agent,
            model_override,
        )?;
    let model = identity.account_id.as_ref().and_then(|_| {
        let selected_models: Vec<Option<&str>> = identity
            .admitted_instances
            .iter()
            .filter(|instance| instance.agent == selected_agent)
            .map(|instance| instances.get(&instance.config_id).map(String::as_str))
            .collect();
        let first = selected_models.first().copied().flatten()?;
        selected_models
            .iter()
            .all(|candidate| *candidate == Some(first))
            .then(|| first.to_owned())
    });
    Ok(DryRunModelProjection { model, instances })
}

fn account_model(account: &jackin_config::AccountConfig) -> Option<String> {
    match &account.credential {
        jackin_config::AccountCredential::ApiKey { model, .. } => model.clone(),
        _ => None,
    }
}
