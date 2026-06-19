//! Pure console product rules.

use jackin_config::AppConfig;
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

pub(crate) type InstanceRefreshSnapshot =
    jackin_console::tui::subscriptions::InstanceRefreshSnapshot<
        crate::instance::InstanceIndexEntry,
        crate::instance::SessionRecord,
        crate::runtime::snapshot::InstanceSnapshot,
    >;

#[cfg(test)]
pub(crate) use jackin_console::services::role_source::resolve_role_input_source;

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
