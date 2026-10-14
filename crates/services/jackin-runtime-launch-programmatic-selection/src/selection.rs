// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AgentConfiguration, AppConfig};
use jackin_core::Agent;

fn free_ephemeral_config_id(selected: &AppConfig, agent: Agent, id: &str) -> String {
    let base = format!("{id}@{}", agent.slug());
    match selected.agent_configurations.get(&base) {
        Some(existing) if existing.agent == agent && existing.account == id => return base,
        None => return base,
        Some(_) => {}
    }
    let mut counter = 2_u32;
    while selected
        .agent_configurations
        .contains_key(&format!("{base}-{counter}"))
    {
        counter += 1;
    }
    format!("{base}-{counter}")
}

fn ensure_account_allowed(
    config: &AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    account_id: &str,
) -> anyhow::Result<()> {
    let Some(workspace) = workspace else {
        return Ok(());
    };
    let workspace_config = config
        .workspaces
        .get(workspace.as_str())
        .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?;
    anyhow::ensure!(
        workspace_config
            .accounts
            .iter()
            .any(|allowed| allowed == account_id),
        "account {account_id:?} is not assigned to workspace {workspace}"
    );
    Ok(())
}

fn bind_selected_account(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: Agent,
    account_id: &str,
) -> anyhow::Result<()> {
    if let Some(workspace) = workspace {
        let override_config = selected
            .workspaces
            .get_mut(workspace.as_str())
            .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?
            .roles
            .entry(role.to_owned())
            .or_default();
        override_config
            .account_bindings
            .insert(agent, account_id.to_owned());
    } else {
        selected
            .account_bindings
            .insert(agent, account_id.to_owned());
    }
    Ok(())
}

fn write_default_launch(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    configuration_ids: &[String],
) -> anyhow::Result<()> {
    if let Some(workspace) = workspace {
        selected
            .workspaces
            .get_mut(workspace.as_str())
            .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?
            .roles
            .entry(role.to_owned())
            .or_default()
            .default_launch = Some(configuration_ids.to_vec());
    } else {
        selected.default_launch = Some(configuration_ids.to_vec());
    }
    Ok(())
}

fn replace_agent_default_launch(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: Agent,
    chosen_ids: &[String],
) -> anyhow::Result<()> {
    anyhow::ensure!(!chosen_ids.is_empty(), "launch selection cannot be empty");

    let inherited = jackin_config::resolve_launch(selected, workspace, role, None, None)?;
    let mut replaced = false;
    let mut launch = Vec::with_capacity(inherited.len() + chosen_ids.len());
    for instance in inherited {
        if instance.agent == agent {
            if !replaced {
                launch.extend(chosen_ids.iter().cloned());
                replaced = true;
            }
        } else {
            launch.push(instance.config_id);
        }
    }
    if !replaced {
        launch.extend(chosen_ids.iter().cloned());
    }
    write_default_launch(selected, workspace, role, &launch)?;
    Ok(())
}

fn ensure_account_is_selected(
    instances: &[jackin_config::ResolvedInstance],
    agent: Agent,
    account_id: &str,
    exact_configuration: Option<&str>,
) -> anyhow::Result<()> {
    let selected: Vec<_> = instances
        .iter()
        .filter(|instance| instance.agent == agent)
        .collect();
    anyhow::ensure!(
        !selected.is_empty(),
        "account {account_id:?} is not admitted for {agent} by the configured default launch set"
    );
    anyhow::ensure!(
        selected
            .iter()
            .all(|instance| instance.account_id == account_id),
        "account selection admitted a sibling account for {agent}"
    );
    if let Some(configuration) = exact_configuration {
        anyhow::ensure!(
            selected.len() == 1 && selected[0].config_id == configuration,
            "configuration {configuration:?} was not the exact launch identity"
        );
    }
    Ok(())
}

pub fn with_account_selection(
    config: &AppConfig,
    agent: Agent,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    id: &str,
) -> anyhow::Result<AppConfig> {
    let account = config
        .accounts
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("account {id:?} is not registered"))?;
    anyhow::ensure!(
        account.supports_agent(agent),
        "account {id:?} does not support {agent}"
    );
    ensure_account_allowed(config, workspace, id)?;

    let admitted_ids = if config.effective_default_launch(workspace, role).is_some() {
        let instances = jackin_config::resolve_launch(config, workspace, role, None, Some(agent))?;
        let ids: Vec<String> = instances
            .into_iter()
            .filter(|instance| instance.agent == agent && instance.account_id == id)
            .map(|instance| instance.config_id)
            .collect();
        anyhow::ensure!(
            !ids.is_empty(),
            "account {id:?} is not admitted for {agent} by the configured default launch set"
        );
        Some(ids)
    } else {
        None
    };

    let mut selected = config.clone();
    bind_selected_account(&mut selected, workspace, role, agent, id)?;
    if let Some(configuration_ids) = admitted_ids {
        replace_agent_default_launch(&mut selected, workspace, role, agent, &configuration_ids)?;
    } else {
        let config_id = free_ephemeral_config_id(&selected, agent, id);
        selected
            .agent_configurations
            .entry(config_id.clone())
            .or_insert_with(|| AgentConfiguration {
                agent,
                account: id.to_owned(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            });
        write_default_launch(&mut selected, workspace, role, &[config_id])?;
    }
    let instances = jackin_config::resolve_launch(&selected, workspace, role, None, Some(agent))?;
    ensure_account_is_selected(&instances, agent, id, None)?;
    Ok(selected)
}

pub fn with_configuration_selection(
    config: &AppConfig,
    agent: Agent,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    configuration_id: &str,
) -> anyhow::Result<AppConfig> {
    let requested = [configuration_id.to_owned()];
    let instances = jackin_config::resolve_launch(config, workspace, role, Some(&requested), None)?;
    let instance = instances
        .first()
        .ok_or_else(|| anyhow::anyhow!("configuration {configuration_id:?} resolved empty"))?;
    anyhow::ensure!(
        instance.agent == agent,
        "configuration {configuration_id:?} belongs to {}, not {agent}",
        instance.agent
    );
    let account_id = instance.account_id.clone();
    ensure_account_allowed(config, workspace, &account_id)?;

    let mut selected = config.clone();
    bind_selected_account(&mut selected, workspace, role, agent, &account_id)?;
    if selected.effective_default_launch(workspace, role).is_some() {
        replace_agent_default_launch(&mut selected, workspace, role, agent, &requested)?;
    } else {
        write_default_launch(&mut selected, workspace, role, &requested)?;
    }
    let resolved = jackin_config::resolve_launch(&selected, workspace, role, None, Some(agent))?;
    ensure_account_is_selected(&resolved, agent, &account_id, Some(configuration_id))?;
    Ok(selected)
}
