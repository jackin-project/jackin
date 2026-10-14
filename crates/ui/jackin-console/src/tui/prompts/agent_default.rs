// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDefaultResolution {
    Launch(String),
    NoDefault,
    Invalid(String),
}

#[must_use]
pub fn resolve_agent_default(
    config: &AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: jackin_core::Agent,
) -> AgentDefaultResolution {
    let ws = match workspace {
        Some(name) => match config.workspaces.get(name.as_str()) {
            Some(ws) => Some(ws),
            None => {
                return AgentDefaultResolution::Invalid(format!(
                    "workspace {name} is not configured"
                ));
            }
        },
        None => None,
    };
    let binding = ws
        .and_then(|w| w.roles.get(role))
        .and_then(|r| r.account_bindings.get(&agent))
        .or_else(|| ws.and_then(|w| w.account_bindings.get(&agent)))
        .or_else(|| config.account_bindings.get(&agent));
    let Some(id) = binding else {
        return AgentDefaultResolution::NoDefault;
    };
    if ws.is_some_and(|w| !w.accounts.contains(id)) {
        return AgentDefaultResolution::Invalid(format!(
            "account {id:?} is not assigned to this workspace"
        ));
    }
    let Some(account) = config.accounts.get(id) else {
        return AgentDefaultResolution::Invalid(format!("unknown account {id:?}"));
    };
    if !account.supports_agent(agent) {
        return AgentDefaultResolution::Invalid(format!("account {id:?} does not support {agent}"));
    }
    AgentDefaultResolution::Launch(id.clone())
}
