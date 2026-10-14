// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AccountConfig, AppConfig, ResolvedInstance, resolve_launch};
use jackin_core::{Agent, WorkspaceName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountChoice {
    pub id: String,
    pub name: String,
    pub provider: jackin_config::AiProvider,
    pub agents: Vec<Agent>,
    pub configuration_id: Option<String>,
    pub instance_id: Option<String>,
}

impl AccountChoice {
    #[must_use]
    pub fn into_launch_selection(self) -> jackin_core::LaunchSelection {
        match self.configuration_id {
            Some(id) => jackin_core::LaunchSelection::Configuration(id),
            None => jackin_core::LaunchSelection::Account(self.id),
        }
    }

    pub fn label(&self) -> String {
        if let Some(instance_id) = self.instance_id.as_deref() {
            return format!(
                "{} · {} ({}) · instance {instance_id}",
                self.name, self.provider, self.id
            );
        }
        self.configuration_id.as_deref().map_or_else(
            || format!("{} · {} ({})", self.name, self.provider, self.id),
            |configuration_id| {
                format!(
                    "{} · {} ({}) · configuration {configuration_id}",
                    self.name, self.provider, self.id
                )
            },
        )
    }
}

pub(crate) fn account_row(id: &str, account: &AccountConfig) -> AccountChoice {
    AccountChoice {
        id: id.to_owned(),
        name: account.name.clone(),
        provider: account.provider,
        agents: Agent::ALL
            .iter()
            .copied()
            .filter(|agent| account.supports_agent(*agent))
            .collect(),
        configuration_id: None,
        instance_id: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInstanceAdmission {
    pub instance_id: String,
    pub agent: Agent,
    pub account_id: String,
}

pub fn account_choices(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
) -> Vec<AccountChoice> {
    config
        .accounts
        .iter()
        .filter(|(id, account)| {
            account.enabled
                && workspace.is_none_or(|workspace| {
                    config
                        .workspaces
                        .get(workspace.as_str())
                        .is_some_and(|workspace| workspace.accounts.contains(id))
                })
        })
        .map(|(id, account)| account_row(id, account))
        .collect()
}

pub fn accounts_for_launch(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    agent: Agent,
) -> Vec<AccountChoice> {
    account_choices(config, workspace)
        .into_iter()
        .filter(|account| account.agents.contains(&agent))
        .collect()
}

#[must_use]
pub fn account_choices_for_instances(
    config: &AppConfig,
    instances: &[ResolvedInstance],
) -> Vec<AccountChoice> {
    let mut choices: Vec<AccountChoice> = instances
        .iter()
        .filter_map(|instance| {
            config.accounts.get(&instance.account_id).map(|account| {
                let mut choice = account_row(&instance.account_id, account);
                choice.configuration_id = Some(instance.config_id.clone());
                choice
            })
        })
        .collect();
    choices.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.configuration_id.cmp(&right.configuration_id))
    });
    choices
}

#[must_use]
pub fn account_choices_for_live_instances(
    config: &AppConfig,
    admissions: &[LiveInstanceAdmission],
) -> Vec<AccountChoice> {
    let mut choices: Vec<AccountChoice> = admissions
        .iter()
        .filter_map(|admission| {
            let account = config.accounts.get(&admission.account_id)?;
            if !account.enabled || !account.supports_agent(admission.agent) {
                return None;
            }
            let mut choice = account_row(&admission.account_id, account);
            choice.agents = vec![admission.agent];
            choice.instance_id = Some(admission.instance_id.clone());
            Some(choice)
        })
        .collect();
    choices.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    choices
}

pub fn admitted_account_choices(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
    agent: Agent,
) -> anyhow::Result<Option<Vec<AccountChoice>>> {
    if config.effective_default_launch(workspace, role).is_none() {
        return Ok(None);
    }
    let instances = resolve_launch(config, workspace, role, None, Some(agent))?;
    let mine: Vec<ResolvedInstance> = instances
        .into_iter()
        .filter(|instance| instance.agent == agent)
        .collect();
    Ok(Some(account_choices_for_instances(config, &mine)))
}
