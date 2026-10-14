// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AgentDefaultResolution, resolve_agent_default};
use jackin_config::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAccountSelection {
    Launch(jackin_core::LaunchSelection),
    Pick(Vec<crate::services::launch::AccountChoice>),
}

pub fn select_launch_account(
    config: &AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: jackin_core::Agent,
    mut eligible: Vec<crate::services::launch::AccountChoice>,
) -> anyhow::Result<LaunchAccountSelection> {
    if let Some(mut admitted) =
        crate::services::launch::admitted_account_choices(config, workspace, role, agent)?
    {
        return match admitted.len() {
            0 => {
                let scope = match workspace {
                    Some(name) => format!("workspace {name}"),
                    None => "this launch".to_owned(),
                };
                Err(anyhow::anyhow!(no_admitted_instance_message(agent, scope)))
            }
            1 => Ok(LaunchAccountSelection::Launch(
                admitted.swap_remove(0).into_launch_selection(),
            )),
            _ => Ok(LaunchAccountSelection::Pick(admitted)),
        };
    }
    match resolve_agent_default(config, workspace, role, agent) {
        AgentDefaultResolution::Launch(id) => {
            return Ok(LaunchAccountSelection::Launch(
                jackin_core::LaunchSelection::Account(id),
            ));
        }
        AgentDefaultResolution::Invalid(message) => return Err(anyhow::anyhow!(message)),
        AgentDefaultResolution::NoDefault => {}
    }
    let allowlist = workspace.and_then(|name| config.workspaces.get(name.as_str()));
    if let Some(ws) = allowlist {
        for id in &ws.accounts {
            if !config.accounts.contains_key(id) {
                return Err(anyhow::anyhow!("unknown account {id:?}"));
            }
        }
    }
    match eligible.len() {
        0 => {
            let scope = match workspace {
                Some(name) => format!("workspace {name}"),
                None => "this launch".to_owned(),
            };
            Err(anyhow::anyhow!(no_eligible_account_message(agent, scope)))
        }
        1 => Ok(LaunchAccountSelection::Launch(
            eligible.swap_remove(0).into_launch_selection(),
        )),
        _ => {
            sort_account_choices_by_id(&mut eligible);
            Ok(LaunchAccountSelection::Pick(eligible))
        }
    }
}

pub fn sort_account_choices_by_id(accounts: &mut [crate::services::launch::AccountChoice]) {
    accounts.sort_by_key(|account| account.id.clone());
}

#[must_use]
pub fn no_eligible_account_message(
    agent: jackin_core::Agent,
    scope: impl std::fmt::Display,
) -> String {
    format!(
        "No account can authenticate {agent} in {scope}.\n\nAdd an account that supports {agent}, or set a default account binding for it."
    )
}

#[must_use]
pub fn no_admitted_instance_message(
    agent: jackin_core::Agent,
    scope: impl std::fmt::Display,
) -> String {
    format!(
        "No launch configuration admits {agent} in {scope}.\n\nAdd a {agent} configuration to default_launch, or clear the default to fall back to account bindings."
    )
}
