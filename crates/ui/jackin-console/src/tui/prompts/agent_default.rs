// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch agent default resolution.

use jackin_config::AppConfig;

/// Outcome of the per-agent `account_bindings` lookup shared by the
/// committed-agent launch path and the new-session picker.
///
/// Mirrors `jackin_config::resolve_account` precedence — role binding, then
/// workspace binding, then the global binding. Every binding that names an
/// account outside the workspace allowlist is a hard error; an inherited
/// global selection cannot widen workspace access or silently choose another
/// account.
///
/// This is the legacy regime: bindings apply only when no `default_launch`
/// is configured at any scope. A configured default set is authoritative
/// admission and overrides every binding (see [`select_launch_account`]);
/// defaults-aware callers gate on
/// `crate::services::launch::admitted_account_choices` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDefaultResolution {
    /// A binding resolved to a registered, authorized, agent-compatible
    /// account. Launch it directly; never open the picker.
    Launch(String),
    /// No binding applies. Fall back to the eligible-candidate list.
    NoDefault,
    /// An explicit binding exists but is unusable: unknown account id,
    /// unauthorized role/workspace binding, or an agent-incompatible
    /// (including disabled) account. Fail atomically with the message;
    /// never fall back to another candidate.
    Invalid(String),
}

/// Resolve the configured default account for `agent` without consulting the
/// eligible-candidate list.
///
/// `workspace` is the saved workspace name (`None` for ad-hoc launches) and
/// `role` its key (`name` or `namespace/name`, matching the workspace
/// `roles` map). Unknown workspaces report `Invalid`: the launch paths
/// resolve the workspace first, so this only fires on concurrent-delete
/// races.
///
/// Legacy regime only: this lookup deliberately ignores `default_launch`.
/// Defaults-aware callers ([`select_launch_account`]) consult the admitted
/// set first and reach this only when no default is configured.
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
