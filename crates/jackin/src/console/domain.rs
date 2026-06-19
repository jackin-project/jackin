//! Pure console product rules.

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

#[cfg(test)]
pub(crate) use jackin_console::services::role_source::resolve_role_input_source;

pub(crate) use jackin_console::services::launch::resolve_committed_agent_launch;
