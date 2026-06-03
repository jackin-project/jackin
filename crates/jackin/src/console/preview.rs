//! Console role-preview rendering: show role metadata before launch confirmation.
//!
//! Not responsible for: launch confirmation UI or constructing `LoadOptions` —
//! those live in `src/console/tui/launch.rs`.

use super::domain::WorkspaceChoice;
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
