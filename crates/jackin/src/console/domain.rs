//! Pure console product rules.

use std::collections::{HashMap, HashSet};

use crate::config::{AppConfig, RoleSource};
use crate::workspace::{LoadWorkspaceInput, MountConfig, ResolvedWorkspace, current_dir_workspace};
use jackin_core::RoleSelector;
use jackin_console::tui::auth::AuthKind;
use jackin_console::tui::auth_config::auth_kind_agent;

// WorkspaceMounts impl for WorkspaceConfig now lives in jackin-console (orphan rule).

// Validate a picked source folder against the agent an auth form targets.
// Returns `Ok(())` for non-agent auth kinds. Runtime validation stays
// in the binary adapter because `jackin-console` cannot depend on runtime.
pub(in crate::console) fn validate_auth_source_folder(
    kind: Option<AuthKind>,
    path: &std::path::Path,
) -> Result<(), String> {
    let Some(agent) = kind.and_then(auth_kind_agent) else {
        return Ok(());
    };
    let host_home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    jackin_runtime::instance::validate_sync_source_dir(agent, path, &host_home)
}

#[derive(Debug)]
pub(crate) struct InstanceRefreshSnapshot {
    pub(crate) instances: Vec<crate::instance::InstanceIndexEntry>,
    pub(crate) sessions: HashMap<String, Vec<crate::instance::SessionRecord>>,
    pub(crate) session_errors: HashSet<String>,
    pub(crate) snapshots: HashMap<String, crate::runtime::snapshot::InstanceSnapshot>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_roles: Vec<RoleSelector>,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
    pub global_mounts: Vec<MountConfig>,
    pub input: LoadWorkspaceInput,
}

/// Resolve the role source the console should load for an operator-entered selector.
pub(crate) struct ResolvedRoleInput {
    pub(crate) raw: String,
    pub(crate) key: String,
    pub(crate) selector: RoleSelector,
    pub(crate) source: RoleSource,
}

pub(crate) struct RoleInputResolutionError {
    pub(crate) raw: String,
    pub(crate) source_url: Option<String>,
    pub(crate) error: anyhow::Error,
}

pub(crate) fn resolve_role_input_source(
    config: &AppConfig,
    value: &str,
) -> Result<ResolvedRoleInput, RoleInputResolutionError> {
    let raw = value.trim();
    crate::debug_log!("role", "resolving role loader input: raw={raw:?}");
    let selector = RoleSelector::parse(raw).map_err(|e| {
        crate::debug_log!("role", "role selector parse failed for {raw:?}: {e}");
        RoleInputResolutionError {
            raw: raw.to_owned(),
            source_url: None,
            error: anyhow::Error::new(e),
        }
    })?;
    crate::debug_log!("role", "parsed role selector: {selector}");

    let key = selector.key();
    let source = jackin_console::services::role_source::candidate_role_source(config, &selector)
        .map_err(|error| {
            crate::debug_log!(
                "role",
                "role loader failed for key={key:?} raw={raw:?}: {error:?}"
            );
            let source_url =
                jackin_console::services::role_source::candidate_role_source(config, &selector)
                    .ok()
                    .map(|source| source.git);
            RoleInputResolutionError {
                raw: raw.to_owned(),
                source_url,
                error,
            }
        })?;
    crate::debug_log!(
        "role",
        "resolved candidate role source: key={key:?} git={git:?} trusted={trusted}",
        git = source.git.as_str(),
        trusted = source.trusted
    );
    Ok(ResolvedRoleInput {
        raw: raw.to_owned(),
        key,
        selector,
        source,
    })
}

/// `Ok(None)` when a saved name went missing between keypress and
/// dispatch (concurrent delete via the manager).
pub fn build_workspace_choice(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
) -> anyhow::Result<Option<WorkspaceChoice>> {
    let global_mounts = jackin_console::services::workspace::unscoped_global_mounts(config)?;
    match input {
        LoadWorkspaceInput::CurrentDir => {
            let current = current_dir_workspace(cwd)?;
            Ok(Some(WorkspaceChoice {
                name: "Current directory".to_owned(),
                workspace: ResolvedWorkspace {
                    label: current.workdir.clone(),
                    workdir: current.workdir,
                    mounts: current.mounts,
                    default_agent: None,
                    keep_awake_enabled: false,
                    git_pull_on_entry: false,
                },
                allowed_roles: jackin_console::workspace::configured_roles(config.roles.keys()),
                default_role: None,
                last_role: None,
                global_mounts,
                input: LoadWorkspaceInput::CurrentDir,
            }))
        }
        LoadWorkspaceInput::Saved(name) => {
            let Some(saved) = config.workspaces.get(name) else {
                return Ok(None);
            };
            let allowed_roles =
                jackin_console::workspace::eligible_roles_for_workspace(config.roles.keys(), saved);
            Ok(Some(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                    default_agent: saved.default_agent,
                    keep_awake_enabled: saved.keep_awake.enabled,
                    git_pull_on_entry: saved.git_pull_on_entry,
                },
                allowed_roles,
                default_role: saved.default_role.clone(),
                last_role: saved.last_role.clone(),
                global_mounts,
                input: LoadWorkspaceInput::Saved(name.clone()),
            }))
        }
        // CLI-only shape (`jackin load --path`); console never
        // produces it.
        LoadWorkspaceInput::Path { .. } => Ok(None),
    }
}

#[derive(Debug)]
pub(crate) enum LaunchDispatchResolution {
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

pub(crate) fn resolve_launch_dispatch(
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
        let role = roles.into_iter().next().unwrap();
        let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
        return Ok(Some(LaunchDispatchResolution::SingleRole {
            role,
            workspace,
        }));
    }

    let selected = jackin_console::workspace::preferred_role_index(
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

pub(crate) struct CommittedRoleLaunch {
    pub(crate) input: LoadWorkspaceInput,
    pub(crate) workspace: ResolvedWorkspace,
}

pub(crate) fn resolve_committed_role_launch(
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

pub(crate) struct CommittedAgentLaunch {
    pub(crate) input: LoadWorkspaceInput,
    pub(crate) role: RoleSelector,
    pub(crate) workspace: ResolvedWorkspace,
    pub(crate) providers: Vec<jackin_protocol::Provider>,
}

pub(crate) fn resolve_committed_agent_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: RoleSelector,
    agent: jackin_core::Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
    let providers = providers_for_launch(config, &choice.name, &role.key(), agent);
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        providers,
    }))
}

pub(crate) fn resolve_provider_launch_workspace(
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
    crate::workspace::resolve_load_workspace(config, role, cwd, choice.input.clone(), &[])
}

fn operator_key_present(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    env_var: &str,
) -> bool {
    crate::operator_env::lookup_operator_env_raw(
        config,
        Some(role_selector),
        Some(workspace_name),
        env_var,
    )
    .is_some()
}

pub(in crate::console) fn providers_for_launch(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    agent: jackin_core::Agent,
) -> Vec<jackin_protocol::Provider> {
    // Map each provider to whether the operator configured its key, using the
    // same `key_env_var()` accessor the capsule reads so the two surfaces cannot
    // disagree. `available_for` only consults this for agents that actually need
    // a key (`needs_key_for_agent`), so Anthropic+claude still passes on
    // subscription auth while Anthropic+opencode requires `ANTHROPIC_API_KEY`.
    // `is_none_or` passes any provider that has no key variable at all.
    let key = |env_var: &str| operator_key_present(config, workspace_name, role_selector, env_var);
    jackin_protocol::Provider::available_for(agent.slug(), |provider: jackin_protocol::Provider| {
        provider.key_env_var().is_none_or(&key)
    })
}

#[cfg(test)]
mod tests;
