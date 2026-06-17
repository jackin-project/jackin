//! Pure console product rules.

use std::collections::{HashMap, HashSet};

use jackin_config::{AppConfig, RoleSource};
use jackin_console::tui::auth::AuthKind;
use jackin_console::tui::auth_config::auth_kind_agent;
use jackin_core::RoleSelector;

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

pub(crate) struct CommittedAgentLaunch {
    pub(crate) input: jackin_config::LoadWorkspaceInput,
    pub(crate) role: RoleSelector,
    pub(crate) workspace: jackin_config::ResolvedWorkspace,
    pub(crate) providers: Vec<jackin_protocol::Provider>,
}

pub(crate) fn resolve_committed_agent_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: jackin_config::LoadWorkspaceInput,
    role: RoleSelector,
    agent: jackin_core::Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let Some(choice) =
        jackin_console::services::launch::build_workspace_choice(config, cwd, &input)?
    else {
        return Ok(None);
    };
    let workspace = jackin_config::resolve_load_workspace(config, &role, cwd, input.clone(), &[])?;
    let providers = providers_for_launch(config, &choice.name, &role.key(), agent);
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        providers,
    }))
}

fn operator_key_present(
    config: &AppConfig,
    workspace_name: &str,
    role_selector: &str,
    env_var: &str,
) -> bool {
    jackin_env::lookup_operator_env_raw(config, Some(role_selector), Some(workspace_name), env_var)
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
