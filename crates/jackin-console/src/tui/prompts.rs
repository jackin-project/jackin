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
/// workspace binding, then the global binding — including its authorization
/// asymmetry: role/workspace bindings that name an account outside the
/// workspace allowlist are hard errors, while an unauthorized global binding
/// is silently filtered (a global default can never widen workspace access).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDefaultResolution {
    /// A binding resolved to a registered, authorized, agent-compatible
    /// account. Launch it directly; never open the picker.
    Launch(String),
    /// No binding applies (or the global binding was filtered as
    /// unauthorized). Fall back to the eligible-candidate list.
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
        .or_else(|| {
            config
                .account_bindings
                .get(&agent)
                .filter(|id| ws.is_none_or(|w| w.accounts.contains(id)))
        });
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
/// A valid binding default (see [`resolve_agent_default`]) always wins, so a
/// configured default with several eligible accounts launches without a
/// picker. Without a default, a sole eligible candidate launches directly and
/// several open the picker in stable id order. Zero eligible candidates is an
/// actionable error — an agent launch never proceeds with no account.
///
/// Like `resolve_account`, a dangling workspace-allowlist id is a config
/// error even when other candidates exist: it is reported, never skipped.
///
/// # Errors
///
/// Returns an error for an invalid explicit binding, an unknown workspace, a
/// dangling allowlist id, or zero eligible candidates.
pub fn select_launch_account(
    config: &AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: jackin_core::Agent,
    mut eligible: Vec<crate::services::launch::AccountChoice>,
) -> anyhow::Result<LaunchAccountSelection> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::launch::accounts_for_launch;
    use jackin_config::{
        AccountConfig, AccountCredential, AiProvider, WorkspaceConfig, WorkspaceRoleOverride,
    };
    use jackin_core::{Agent, EnvValue, WorkspaceName};
    use std::collections::BTreeMap;

    const ROLE: &str = "the-architect";

    fn api_key_account(name: &str, provider: AiProvider) -> AccountConfig {
        AccountConfig {
            enabled: true,
            name: name.into(),
            provider,
            credential: AccountCredential::ApiKey {
                value: EnvValue::Plain("test-key".into()),
                base_url: None,
                model: None,
            },
        }
    }

    /// Two Claude-capable accounts, one Codex-capable account, one outsider
    /// (registered but outside the `demo` allowlist), all authorized ids
    /// valid. Callers add bindings per case.
    fn test_config() -> (AppConfig, WorkspaceName) {
        let mut config = AppConfig::default();
        config.accounts.insert(
            "a-claude".into(),
            api_key_account("A", AiProvider::Anthropic),
        );
        config.accounts.insert(
            "z-claude".into(),
            api_key_account("Z", AiProvider::Anthropic),
        );
        config
            .accounts
            .insert("o-codex".into(), api_key_account("O", AiProvider::OpenAi));
        config.accounts.insert(
            "outside".into(),
            api_key_account("Outside", AiProvider::Anthropic),
        );
        let ws = WorkspaceName::parse("demo").unwrap();
        config.workspaces.insert(
            ws.as_str().into(),
            WorkspaceConfig {
                workdir: "/demo".into(),
                accounts: vec!["a-claude".into(), "z-claude".into(), "o-codex".into()],
                ..Default::default()
            },
        );
        (config, ws)
    }

    fn set_role_binding(config: &mut AppConfig, ws: &str, role: &str, agent: Agent, id: &str) {
        config.workspaces.get_mut(ws).unwrap().roles.insert(
            role.into(),
            WorkspaceRoleOverride {
                account_bindings: BTreeMap::from([(agent, id.to_owned())]),
                ..Default::default()
            },
        );
    }

    fn eligible_ids(selection: &LaunchAccountSelection) -> Vec<&str> {
        let LaunchAccountSelection::Pick(accounts) = selection else {
            panic!("expected picker selection; got {selection:?}");
        };
        accounts.iter().map(|account| account.id.as_str()).collect()
    }

    #[test]
    fn role_binding_beats_workspace_and_global() {
        let (mut config, ws) = test_config();
        let workspace = config.workspaces.get_mut(ws.as_str()).unwrap();
        workspace
            .account_bindings
            .insert(Agent::Claude, "a-claude".into());
        config
            .account_bindings
            .insert(Agent::Claude, "a-claude".into());
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::Launch("z-claude".into())
        );
    }

    #[test]
    fn workspace_binding_beats_global() {
        let (mut config, ws) = test_config();
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .account_bindings
            .insert(Agent::Claude, "z-claude".into());
        config
            .account_bindings
            .insert(Agent::Claude, "a-claude".into());

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::Launch("z-claude".into())
        );
    }

    #[test]
    fn global_binding_honored_with_several_accounts() {
        let (mut config, ws) = test_config();
        config
            .account_bindings
            .insert(Agent::Claude, "z-claude".into());
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(eligible.len() > 1);

        assert_eq!(
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
            LaunchAccountSelection::Launch("z-claude".into())
        );
    }

    #[test]
    fn role_binding_for_other_role_is_ignored() {
        let (mut config, ws) = test_config();
        set_role_binding(
            &mut config,
            ws.as_str(),
            "other-role",
            Agent::Claude,
            "z-claude",
        );

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::NoDefault
        );
    }

    #[test]
    fn sole_eligible_candidate_launches_without_binding() {
        let (mut config, ws) = test_config();
        config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["z-claude".into()];
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert_eq!(eligible.len(), 1);

        assert_eq!(
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
            LaunchAccountSelection::Launch("z-claude".into())
        );
    }

    #[test]
    fn no_binding_with_several_candidates_opens_picker_in_id_order() {
        let (config, ws) = test_config();
        let mut eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        eligible.reverse();
        assert_eq!(eligible.len(), 2);

        let selection =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
        assert_eq!(eligible_ids(&selection), vec!["a-claude", "z-claude"]);
    }

    #[test]
    fn picker_order_is_stable_regardless_of_input_order() {
        let (config, ws) = test_config();
        let forward = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        let mut backward = forward.clone();
        backward.reverse();

        let from_forward =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, forward).unwrap();
        let from_backward =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, backward).unwrap();
        assert_eq!(eligible_ids(&from_forward), eligible_ids(&from_backward));
    }

    #[test]
    fn zero_eligible_candidates_error_actionably() {
        let (mut config, ws) = test_config();
        config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["o-codex".into()];
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(eligible.is_empty());

        let error =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("claude"),
            "error must name the agent; got {message:?}"
        );
        assert!(
            message.contains("demo"),
            "error must name the workspace; got {message:?}"
        );
        assert!(
            message.contains("binding"),
            "error must point at the binding remedy; got {message:?}"
        );
    }

    #[test]
    fn zero_eligible_candidates_without_workspace_errors() {
        let (config, _) = test_config();
        let error =
            select_launch_account(&config, None, ROLE, Agent::Muse, Vec::new()).unwrap_err();
        assert!(
            error.to_string().contains("muse"),
            "ad-hoc error must name the agent; got {error:?}"
        );
    }

    #[test]
    fn empty_allowlist_errors_despite_global_accounts() {
        // Empty selection: the saved workspace authorizes nothing, so the
        // global default is filtered and zero candidates remain — an error,
        // even though compatible accounts exist globally.
        let (mut config, ws) = test_config();
        config.workspaces.get_mut(ws.as_str()).unwrap().accounts = Vec::new();
        config
            .account_bindings
            .insert(Agent::Claude, "a-claude".into());
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(eligible.is_empty());

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::NoDefault
        );
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn missing_workspace_falls_back_to_global_accounts() {
        // Missing selection: ad-hoc launches have no allowlist, so the
        // global binding applies unfiltered and bare candidates stay
        // eligible.
        let (mut config, _) = test_config();
        config
            .account_bindings
            .insert(Agent::Claude, "z-claude".into());
        let eligible = accounts_for_launch(&config, None, Agent::Claude);
        assert!(eligible.len() > 1);

        assert_eq!(
            select_launch_account(&config, None, ROLE, Agent::Claude, eligible).unwrap(),
            LaunchAccountSelection::Launch("z-claude".into())
        );

        let (config, _) = test_config();
        let eligible = accounts_for_launch(&config, None, Agent::Claude);
        let selection =
            select_launch_account(&config, None, ROLE, Agent::Claude, eligible).unwrap();
        assert_eq!(
            eligible_ids(&selection),
            vec!["a-claude", "outside", "z-claude"]
        );
    }

    #[test]
    fn unknown_workspace_name_errors() {
        let (config, _) = test_config();
        let ghost = WorkspaceName::parse("ghost").unwrap();
        let eligible = accounts_for_launch(&config, None, Agent::Claude);

        assert!(matches!(
            resolve_agent_default(&config, Some(&ghost), ROLE, Agent::Claude),
            AgentDefaultResolution::Invalid(_)
        ));
        select_launch_account(&config, Some(&ghost), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn unauthorized_role_binding_hard_errors_without_fallback() {
        let (mut config, ws) = test_config();
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "outside");
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(!eligible.is_empty());

        let resolution = resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude);
        assert!(
            matches!(resolution, AgentDefaultResolution::Invalid(_)),
            "unauthorized role default must be invalid; got {resolution:?}"
        );
        let error =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
        assert!(error.to_string().contains("not assigned"), "got {error:?}");
    }

    #[test]
    fn unauthorized_workspace_binding_hard_errors_without_fallback() {
        let (mut config, ws) = test_config();
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .account_bindings
            .insert(Agent::Claude, "outside".into());
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(!eligible.is_empty());

        assert!(matches!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::Invalid(_)
        ));
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn unauthorized_global_binding_is_filtered_not_honored() {
        // Global defaults can never widen workspace access: an unauthorized
        // global binding is ignored and the eligible candidates decide.
        let (mut config, ws) = test_config();
        config
            .account_bindings
            .insert(Agent::Claude, "outside".into());
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert_eq!(eligible.len(), 2);

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::NoDefault
        );
        let selection =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
        assert_eq!(eligible_ids(&selection), vec!["a-claude", "z-claude"]);
    }

    #[test]
    fn binding_to_unknown_account_errors_at_every_scope() {
        let (mut config, ws) = test_config();
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "ghost");
        assert!(matches!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::Invalid(_)
        ));
        // Role scope wins, so the error below pins the role binding; clear
        // it to exercise the workspace scope, then the global scope.
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .roles
            .clear();
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .account_bindings
            .insert(Agent::Claude, "ghost".into());
        // The workspace binding names an id outside the allowlist, so the
        // authorization check fires before the unknown-id check — either way
        // the selection fails atomically.
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible.clone())
            .unwrap_err();
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .account_bindings
            .clear();
        config
            .workspaces
            .get_mut(ws.as_str())
            .unwrap()
            .accounts
            .push("ghost".into());
        // The allowlist now references the ghost id, so the dangling-id
        // check fires even without any binding.
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn global_binding_to_unknown_id_is_filtered_like_any_unauthorized_global() {
        // The global scope cannot distinguish "unknown id" from
        // "unauthorized id": both fail the allowlist filter and fall back
        // to the eligible candidates, exactly like `resolve_account`.
        let (mut config, ws) = test_config();
        config
            .account_bindings
            .insert(Agent::Claude, "ghost".into());
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
            AgentDefaultResolution::NoDefault
        );
        let selection =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
        assert_eq!(eligible_ids(&selection), vec!["a-claude", "z-claude"]);
    }

    #[test]
    fn binding_to_incompatible_account_errors() {
        let (mut config, ws) = test_config();
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "o-codex");
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

        let error =
            select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
        assert!(
            error.to_string().contains("does not support"),
            "got {error:?}"
        );
    }

    #[test]
    fn binding_to_disabled_account_errors() {
        let (mut config, ws) = test_config();
        config.accounts.get_mut("z-claude").unwrap().enabled = false;
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn empty_string_binding_fails_atomically() {
        // An explicit empty selection is invalid (unknown id), never a
        // trigger to fall back to the eligible candidates.
        let (mut config, ws) = test_config();
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "");
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
        assert!(!eligible.is_empty());

        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    }

    #[test]
    fn binding_for_other_agent_does_not_leak() {
        let (mut config, ws) = test_config();
        set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");

        assert_eq!(
            resolve_agent_default(&config, Some(&ws), ROLE, Agent::Codex),
            AgentDefaultResolution::NoDefault
        );
        let eligible = accounts_for_launch(&config, Some(&ws), Agent::Codex);
        assert_eq!(
            select_launch_account(&config, Some(&ws), ROLE, Agent::Codex, eligible).unwrap(),
            LaunchAccountSelection::Launch("o-codex".into())
        );
    }
}
