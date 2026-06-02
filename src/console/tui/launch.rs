//! Wire the console TUI launch flow: role picker → launch confirmation → `LoadOptions` construction.
//!
//! Not responsible for: actually launching a container or connecting to a
//! running one — returns a resolved `(RoleSelector, ResolvedWorkspace)` pair
//! for the caller to act on.

use crate::config::AppConfig;
use crate::console::domain::LaunchDispatchResolution;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};
use jackin_console::tui::components::error_popup::{
    no_eligible_roles_error_message, no_eligible_roles_error_title,
};

use super::{ConsoleStage, ConsoleState};

/// Open the inline role picker for every eligible role count except zero.
/// `WorkspaceChoice` is built fresh each call so manager edits take effect
/// immediately.
pub fn dispatch_launch_for_workspace(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>)>> {
    let Some(resolution) = crate::console::domain::resolve_launch_dispatch(config, cwd, input)?
    else {
        // Workspace was deleted between keypress and dispatch.
        return Ok(None);
    };

    match resolution {
        // Stay so the operator can fix `allowed_roles`
        // — a single Enter shouldn't terminate the TUI.
        LaunchDispatchResolution::NoEligibleRoles { name } => {
            if let ConsoleStage::Manager(ms) = &mut state.stage {
                let _ = crate::console::tui::update_manager(
                    ms,
                    crate::console::tui::ManagerMessage::OpenListErrorPopup {
                        title: no_eligible_roles_error_title().into(),
                        message: no_eligible_roles_error_message(name),
                    },
                );
            }
            state.pending_launch = None;
            state.pending_launch_role = None;
        }
        LaunchDispatchResolution::SingleRole { role, workspace } => {
            return Ok(Some((role, workspace, None)));
        }
        LaunchDispatchResolution::RolePicker {
            input,
            roles,
            selected,
        } => {
            state.pending_launch = Some(input);
            state.pending_launch_role = None;
            if let ConsoleStage::Manager(ms) = &mut state.stage {
                let mut picker = crate::selector::RolePickerState::launch(roles);
                if let Some(selected) = selected {
                    picker.list_state.select(Some(selected));
                }
                ms.inline_role_picker = Some(picker);
            }
        }
    }
    Ok(None)
}
