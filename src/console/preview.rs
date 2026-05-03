use super::state::WorkspaceChoice;
use crate::config::AppConfig;
use crate::selector::RoleSelector;
use crate::workspace::ResolvedWorkspace;

pub(super) fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    role: &RoleSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    crate::workspace::resolve_load_workspace(config, role, cwd, choice.input.clone(), &[])
}
