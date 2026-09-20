// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent and role prompting helpers for the workspace manager event loop.

use jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace};
use jackin_core::RoleSelector;

use crate::tui::components::error_popup::{
    role_resolution_error_message, role_resolution_error_title,
};
use crate::tui::console::{ConsoleOutcome, ConsoleStage, ConsoleState};
pub use crate::tui::message::{AgentPickerChoices, LaunchPromptDispatch, LaunchPromptRequest};
use crate::tui::message::{
    AgentPickerResolution, OnPromptFailure, PromptOutcome, agent_picker_choices_for_workspace,
    launch_agent_prompt_plan,
};
use crate::tui::model::{
    open_launch_account_picker_plan, open_launch_agent_prompt_plan, store_pending_launch_plan,
    take_pending_launch_and_role_plan, take_pending_launch_plan,
};
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::update::{
    apply_status_overlay_plan, dismiss_status_overlay_plan, role_resolution_status_overlay_plan,
};

pub type ConcreteAgentPickerChoices = AgentPickerChoices<jackin_core::Agent>;

pub type ConcreteLaunchPromptDispatch =
    LaunchPromptDispatch<ConsoleOutcome, ConcreteLaunchPromptRequest>;

pub type ConcreteLaunchPromptRequest =
    LaunchPromptRequest<RoleSelector, ResolvedWorkspace, LoadWorkspaceInput>;

pub fn draw_role_resolution_dialog<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: &RoleSelector,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let ConsoleStage::Manager(ms) = &mut state.stage;
    apply_status_overlay_plan(ms, role_resolution_status_overlay_plan(role.key()));
    terminal.draw(|frame| {
        crate::tui::view::render(frame, frame.area(), ms, config, cwd);
    })?;
    apply_status_overlay_plan(ms, dismiss_status_overlay_plan());
    Ok(())
}

pub fn show_role_resolution_error(
    state: &mut ConsoleState,
    role: &RoleSelector,
    error: &anyhow::Error,
) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    update_manager(
        ms,
        ManagerMessage::OpenListErrorPopup {
            title: role_resolution_error_title().into(),
            message: role_resolution_error_message(role.key(), error),
        },
    );
}

fn try_prompt_for_agent(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    choices: ConcreteAgentPickerChoices,
) -> AgentPickerResolution {
    let choices =
        match agent_picker_choices_for_workspace(workspace.default_agent.is_some(), choices) {
            AgentPickerChoices::Choices(choices) => choices,
            AgentPickerChoices::NotNeeded => return AgentPickerResolution::NotNeeded,
            AgentPickerChoices::Failed(error) => return AgentPickerResolution::Failed(error),
        };

    open_launch_agent_prompt_plan(state, role.clone(), choices);
    AgentPickerResolution::Opened
}

pub fn prompt_agent_for_launch(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    input: LoadWorkspaceInput,
    on_failure: OnPromptFailure,
    choices: ConcreteAgentPickerChoices,
) -> PromptOutcome {
    let plan = launch_agent_prompt_plan(
        try_prompt_for_agent(state, role, workspace, choices),
        on_failure,
    );
    if plan.store_pending_launch {
        store_pending_launch_plan(state, input);
    }
    if let Some(error) = plan.error {
        show_role_resolution_error(state, role, &error);
    }
    plan.outcome
}

pub fn dispatch_launch_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<ConcreteLaunchPromptDispatch> {
    let Some((role, workspace, agent)) =
        crate::tui::launch::dispatch_launch_for_workspace(state, config, cwd, input.clone())?
    else {
        return Ok(LaunchPromptDispatch::None);
    };
    if agent.is_some() {
        return Ok(LaunchPromptDispatch::Launch(ConsoleOutcome::Launch(
            role, workspace, agent,
        )));
    }
    Ok(LaunchPromptDispatch::Prompt(LaunchPromptRequest {
        role,
        workspace,
        input,
        on_failure: OnPromptFailure::ClearPending,
    }))
}

pub fn committed_role_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: RoleSelector,
) -> anyhow::Result<ConcreteLaunchPromptDispatch> {
    let Some(input) = take_pending_launch_plan(state) else {
        return Ok(LaunchPromptDispatch::None);
    };
    // Resolution failures surface as an in-TUI error popup (the console
    // stays alive), so the taken plan must be restored — otherwise the
    // still-visible role picker silently stops working.
    let attempt = input.clone();
    let resolved =
        match crate::services::launch::resolve_committed_role_launch(config, cwd, attempt, &role) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => return Ok(LaunchPromptDispatch::None),
            Err(error) => {
                store_pending_launch_plan(state, input);
                return Err(error);
            }
        };
    Ok(LaunchPromptDispatch::Prompt(LaunchPromptRequest {
        role,
        workspace: resolved.workspace,
        input: resolved.input,
        on_failure: OnPromptFailure::RestorePending,
    }))
}

pub fn launch_with_committed_agent(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: jackin_core::Agent,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    let Some((input, role)) = take_pending_launch_and_role_plan(state) else {
        return Ok(None);
    };
    // Same restore contract as `committed_role_prompt`: the agent picker
    // stays visible behind the error popup and must keep working.
    let workspace_name = match &input {
        LoadWorkspaceInput::Saved(name) => match jackin_core::WorkspaceName::parse(name) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                store_pending_launch_plan(state, input);
                state.pending_launch_role = Some(role);
                return Err(error.into());
            }
        },
        LoadWorkspaceInput::CurrentDir | LoadWorkspaceInput::Path { .. } => None,
    };
    let (attempt_input, attempt_role) = (input.clone(), role.clone());
    let resolved = match crate::services::launch::resolve_committed_agent_launch(
        config,
        cwd,
        attempt_input,
        attempt_role,
        agent,
    ) {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return Ok(None),
        Err(error) => {
            store_pending_launch_plan(state, input);
            state.pending_launch_role = Some(role);
            return Err(error);
        }
    };
    match select_launch_account(
        config,
        workspace_name.as_ref(),
        &resolved.role.key(),
        agent,
        resolved.accounts,
    ) {
        Ok(LaunchAccountSelection::Launch(id)) => Ok(Some(ConsoleOutcome::LaunchWithAccount {
            selector: resolved.role,
            workspace: resolved.workspace,
            agent,
            account: Some(id),
            configuration: None,
        })),
        Ok(LaunchAccountSelection::Pick(accounts)) => {
            open_launch_account_picker_plan(state, resolved.input, resolved.role, agent, accounts);
            Ok(None)
        }
        Err(error) => {
            store_pending_launch_plan(state, input);
            state.pending_launch_role = Some(role);
            Err(error)
        }
    }
}

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

/// Committed-launch account decision: launch immediately or show the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAccountSelection {
    /// Launch immediately with this account id: either a valid binding
    /// default or the sole eligible candidate.
    Launch(String),
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
            1 => Ok(LaunchAccountSelection::Launch(admitted.swap_remove(0).id)),
            _ => Ok(LaunchAccountSelection::Pick(admitted)),
        };
    }
    match resolve_agent_default(config, workspace, role, agent) {
        AgentDefaultResolution::Launch(id) => return Ok(LaunchAccountSelection::Launch(id)),
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
        1 => Ok(LaunchAccountSelection::Launch(eligible.swap_remove(0).id)),
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

#[cfg(test)]
mod tests;
