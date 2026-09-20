// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure launch-resolution helpers for the host console.

use jackin_config::{
    AccountConfig, AppConfig, LoadWorkspaceInput, MountHealReport, ResolvedInstance,
    ResolvedWorkspace, current_dir_workspace, resolve_launch, resolve_load_workspace,
};
use jackin_core::{Agent, RoleSelector, WorkspaceName};

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_roles: Vec<RoleSelector>,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
    pub input: LoadWorkspaceInput,
}

/// `Ok(None)` when a saved name went missing between keypress and
/// dispatch (concurrent delete via the manager).
///
/// Global mounts are intentionally not resolved here: every caller follows
/// with `resolve_load_workspace` (via `resolve_selected_workspace`), which
/// is the single site that merges, heals, and validates the effective
/// mounts. A redundant pre-check here would only fail earlier with a
/// poorer error.
pub fn build_workspace_choice(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
) -> anyhow::Result<Option<WorkspaceChoice>> {
    match input {
        LoadWorkspaceInput::CurrentDir => {
            let current = current_dir_workspace(cwd)?;
            Ok(Some(WorkspaceChoice {
                name: "Current directory".to_owned(),
                workspace: ResolvedWorkspace {
                    name: current.workdir.clone(),
                    label: current.workdir.clone(),
                    workdir: current.workdir,
                    mounts: current.mounts,
                    default_agent: None,
                    keep_awake_enabled: false,
                    git_pull_on_entry: false,
                    mount_heal: MountHealReport::default(),
                },
                allowed_roles: crate::workspace::configured_roles(config.roles.keys()),
                default_role: None,
                last_role: None,
                input: LoadWorkspaceInput::CurrentDir,
            }))
        }
        LoadWorkspaceInput::Saved(name) => {
            let Some(saved) = config.workspaces.get(name) else {
                return Ok(None);
            };
            let allowed_roles =
                crate::workspace::eligible_roles_for_workspace(config.roles.keys(), saved);
            Ok(Some(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    name: name.clone(),
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                    default_agent: saved.default_agent,
                    keep_awake_enabled: saved.keep_awake.enabled,
                    git_pull_on_entry: saved.git_pull_on_entry,
                    mount_heal: MountHealReport::default(),
                },
                allowed_roles,
                default_role: saved.default_role.clone(),
                last_role: saved.last_role.clone(),
                input: LoadWorkspaceInput::Saved(name.clone()),
            }))
        }
        // CLI-only shape (`jackin load --path`); console never produces it.
        LoadWorkspaceInput::Path { .. } => Ok(None),
    }
}

#[derive(Debug)]
pub enum LaunchDispatchResolution {
    NoEligibleRoles {
        name: String,
    },
    SingleRole {
        role: RoleSelector,
        workspace: ResolvedWorkspace,
    },
    RolePicker {
        input: LoadWorkspaceInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    },
}

pub fn resolve_launch_dispatch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<LaunchDispatchResolution>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let roles = choice.allowed_roles.clone();

    if roles.is_empty() {
        return Ok(Some(LaunchDispatchResolution::NoEligibleRoles {
            name: choice.name,
        }));
    }

    if roles.len() == 1 {
        let Some(role) = roles.into_iter().next() else {
            unreachable!("roles length checked above");
        };
        let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
        return Ok(Some(LaunchDispatchResolution::SingleRole {
            role,
            workspace,
        }));
    }

    let selected = crate::workspace::preferred_role_index(
        &roles,
        choice.last_role.as_deref(),
        choice.default_role.as_deref(),
    );
    Ok(Some(LaunchDispatchResolution::RolePicker {
        input,
        roles,
        selected,
    }))
}

#[derive(Debug)]
pub struct CommittedRoleLaunch {
    pub input: LoadWorkspaceInput,
    pub workspace: ResolvedWorkspace,
}

pub fn resolve_committed_role_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: &RoleSelector,
) -> anyhow::Result<Option<CommittedRoleLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = resolve_selected_workspace(config, cwd, &choice, role)?;
    Ok(Some(CommittedRoleLaunch { input, workspace }))
}

pub fn resolve_account_launch_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
    selector: &RoleSelector,
) -> anyhow::Result<Option<ResolvedWorkspace>> {
    let Some(choice) = build_workspace_choice(config, cwd, input)? else {
        return Ok(None);
    };
    resolve_selected_workspace(config, cwd, &choice, selector).map(Some)
}

fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    role: &RoleSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    Ok(resolve_load_workspace(
        config,
        role,
        cwd,
        choice.input.clone(),
        &[],
    )?)
}

/// Resolved committed-agent launch: all inputs needed to either launch
/// immediately or open the account picker.
#[derive(Debug)]
pub struct CommittedAgentLaunch {
    pub input: LoadWorkspaceInput,
    pub role: RoleSelector,
    pub workspace: ResolvedWorkspace,
    pub accounts: Vec<AccountChoice>,
}

/// Resolve a committed (role + agent) launch into a workspace and available
/// providers. Returns `Ok(None)` when the workspace went missing between the
/// operator's keypress and the commit (concurrent delete).
///
/// The returned accounts are admission-aware: when a `default_launch` is
/// configured at any scope they are the admitted rows for `agent` (see
/// [`admitted_account_choices`]); otherwise they are the legacy eligible
/// list. An invalid configured default fails here — never falls back.
pub fn resolve_committed_agent_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: RoleSelector,
    agent: Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
    let workspace_name = match &input {
        LoadWorkspaceInput::Saved(name) => Some(WorkspaceName::parse(name)?),
        LoadWorkspaceInput::CurrentDir | LoadWorkspaceInput::Path { .. } => None,
    };
    let accounts =
        match admitted_account_choices(config, workspace_name.as_ref(), &role.key(), agent)? {
            Some(admitted) => admitted,
            None => accounts_for_launch(config, workspace_name.as_ref(), agent),
        };
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        accounts,
    }))
}

/// Secret-free account row used by launch and session pickers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountChoice {
    pub id: String,
    pub name: String,
    pub provider: jackin_config::AiProvider,
    pub agents: Vec<Agent>,
    /// Exact pre-container launch configuration. `None` for legacy account
    /// rows and live rows, which use their own routing identity.
    pub configuration_id: Option<String>,
    /// Exact live launch instance. `None` for pre-container launch rows.
    pub instance_id: Option<String>,
}

impl AccountChoice {
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

/// Secret-free row for one registered account: id, display name, provider,
/// and every agent the account can authenticate.
fn account_row(id: &str, account: &AccountConfig) -> AccountChoice {
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

/// One exact account/agent binding admitted by a live container manifest.
/// This is deliberately separate from [`ResolvedInstance`]: the console
/// refresh service must not re-resolve mutable host defaults to describe a
/// running container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInstanceAdmission {
    pub instance_id: String,
    pub agent: Agent,
    pub account_id: String,
}

/// List only registered accounts authorized by the saved workspace.
/// Ad-hoc launches have no workspace allowlist; choosing a registered account
/// here is the explicit selection. A missing saved workspace yields no choices.
///
/// This is the authorization-and-compatibility view, not the admission view:
/// it ignores `default_launch`. Defaults-aware callers use
/// [`admitted_account_choices`] instead.
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

/// Filter authorized registered accounts by coding-agent compatibility.
///
/// Like [`account_choices`], this ignores `default_launch`; see
/// [`admitted_account_choices`] for the admission-constrained rows.
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

/// Map admitted launch instances to secret-free picker rows.
///
/// One row per admitted configuration, in ascending account/configuration
/// order. Configurations sharing an account remain distinct: the
/// configuration, not just the account, is the launch identity.
/// Instances naming an unregistered account are skipped:
/// `jackin_config::resolve_launch` never produces them, so only a foreign
/// instance list can hit that.
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

/// Build live-session rows from the manifest's admitted instance set.
/// Duplicate accounts remain distinct because `instance_id` is the routing
/// identity; a later picker commit must never collapse them back to account ID.
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

/// Admitted picker rows for a committed (role + agent) launch.
///
/// Returns `None` when no `default_launch` is configured at any scope —
/// the legacy account-bindings path applies. Otherwise resolves through
/// `jackin_config::resolve_launch`, the same resolver the runtime
/// provisions from, so this pre-check cannot drift from it, and returns
/// the admitted rows for `agent` (possibly empty when the defaults admit
/// nothing for this agent).
///
/// # Errors
///
/// Returns the resolver error verbatim when the configured defaults are
/// invalid (unknown configuration, unauthorized or incompatible account):
/// an explicit default fails atomically and never falls back to the
/// eligible-candidate list.
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

#[cfg(test)]
mod tests;
