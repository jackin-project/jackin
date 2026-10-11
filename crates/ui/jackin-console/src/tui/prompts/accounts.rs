// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch account selection and sorting.

use super::{AgentDefaultResolution, resolve_agent_default};
use jackin_config::AppConfig;

/// Committed-launch account decision: launch immediately or show the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAccountSelection {
    /// Launch immediately with the exact configuration or registered account
    /// admitted by defaults, bindings, or the sole eligible candidate.
    Launch(jackin_core::LaunchSelection),
    /// No default applies and several candidates are eligible: show the
    /// account picker with these choices in [`sort_account_choices_by_id`]
    /// order.
    Pick(Vec<crate::services::launch::AccountChoice>),
}

/// Decide the account for a committed (role + agent) launch.
///
/// Two regimes, gated by whether a `default_launch` is configured at any
/// scope (role → workspace → global, via
/// `crate::services::launch::admitted_account_choices`):
///
/// - Defaults regime: the admitted set resolves through
///   `jackin_config::resolve_launch` — the same resolver the runtime
///   provisions from — filtered to `agent`. A single admitted account
///   launches directly (fast start honors valid defaults even with
///   several accounts); several open the picker in stable id order; none
///   is an actionable error. Any resolver error fails atomically: an
///   explicit default never falls back to bindings or the eligible list,
///   and a binding never overrides the admitted set, so a launch never
///   silently substitutes another account or an ambient login.
/// - Legacy regime (no default anywhere): a valid binding default (see
///   [`resolve_agent_default`]) always wins, so a configured binding with
///   several eligible accounts launches without a picker. Without a
///   binding, a sole eligible candidate launches directly and several open
///   the picker in stable id order. Zero eligible candidates is an
///   actionable error — an agent launch never proceeds with no account.
///
/// Like `resolve_account`, a dangling workspace-allowlist id is a config
/// error even when other candidates exist: it is reported, never skipped.
///
/// # Errors
///
/// Returns an error for an invalid configured default, an agent the
/// defaults admit nothing for, an invalid explicit binding, an unknown
/// workspace, a dangling allowlist id, or zero eligible candidates.
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

/// Stable account-picker order: ascending account id.
///
/// Both the committed-agent launch picker and the new-session account picker
/// present candidates in this order, so the same configuration always renders
/// the same list. A valid binding default suppresses the picker instead of
/// reordering it.
pub fn sort_account_choices_by_id(accounts: &mut [crate::services::launch::AccountChoice]) {
    accounts.sort_by_key(|account| account.id.clone());
}

/// Actionable error text for the zero-eligible-account case: names the agent
/// and the launch scope, and points at both remedies (add an account or set a
/// default binding). `scope` is preformatted by the caller, e.g.
/// `workspace "demo"` or `container "jackin-demo-architect"`.
#[must_use]
pub fn no_eligible_account_message(
    agent: jackin_core::Agent,
    scope: impl std::fmt::Display,
) -> String {
    format!(
        "No account can authenticate {agent} in {scope}.\n\nAdd an account that supports {agent}, or set a default account binding for it."
    )
}

/// Actionable error text for the admitted-but-empty case: a `default_launch`
/// is configured, but the admitted set holds no instance for `agent`.
/// Points at both remedies (admit a configuration for the agent, or clear
/// the default to fall back to account bindings). `scope` is preformatted
/// by the caller, like [`no_eligible_account_message`].
#[must_use]
pub fn no_admitted_instance_message(
    agent: jackin_core::Agent,
    scope: impl std::fmt::Display,
) -> String {
    format!(
        "No launch configuration admits {agent} in {scope}.\n\nAdd a {agent} configuration to default_launch, or clear the default to fall back to account bindings."
    )
}
