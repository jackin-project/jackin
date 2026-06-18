//! Wire the console TUI launch flow: role picker → launch confirmation → `LoadOptions` construction.
//!
//! Not responsible for: actually launching a container or connecting to a
//! running one — returns a resolved `(RoleSelector, ResolvedWorkspace)` pair
//! for the caller to act on.

use jackin_config::AppConfig;
use jackin_config::{LoadWorkspaceInput, ResolvedWorkspace};
use jackin_console::services::launch::LaunchDispatchResolution;
use jackin_console::tui::app::{clear_pending_launch_plan, open_launch_role_prompt_plan};
use jackin_console::tui::components::error_popup::{
    no_eligible_roles_error_message, no_eligible_roles_error_title,
};
use jackin_core::RoleSelector;

use super::{ConsoleStage, ConsoleState};

/// Open the inline role picker for every eligible role count except zero.
/// `WorkspaceChoice` is built fresh each call so manager edits take effect
/// immediately.
pub fn dispatch_launch_for_workspace(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<jackin_core::Agent>)>> {
    let Some(resolution) =
        jackin_console::services::launch::resolve_launch_dispatch(config, cwd, input)?
    else {
        // Workspace was deleted between keypress and dispatch.
        return Ok(None);
    };

    match resolution {
        // Stay so the operator can fix `allowed_roles`
        // — a single Enter shouldn't terminate the TUI.
        LaunchDispatchResolution::NoEligibleRoles { name } => {
            if let ConsoleStage::Manager(ms) = &mut state.stage {
                let _unused = crate::console::tui::update_manager(
                    ms,
                    crate::console::tui::ManagerMessage::OpenListErrorPopup {
                        title: no_eligible_roles_error_title().into(),
                        message: no_eligible_roles_error_message(name),
                    },
                );
            }
            clear_pending_launch_plan(state);
        }
        LaunchDispatchResolution::SingleRole { role, workspace } => {
            return Ok(Some((role, workspace, None)));
        }
        LaunchDispatchResolution::RolePicker {
            input,
            roles,
            selected,
        } => {
            open_launch_role_prompt_plan(state, input, roles, selected);
        }
    }
    Ok(None)
}
