use super::state::WorkspaceChoice;
use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

pub(super) fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    agent: &ClassSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    crate::workspace::resolve_load_workspace(config, agent, cwd, choice.input.clone(), &[])
}
