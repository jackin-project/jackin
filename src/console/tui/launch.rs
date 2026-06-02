use crate::config::AppConfig;
use crate::console::domain::LaunchDispatchResolution;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

use super::{ConsoleStage, ConsoleState};

/// Open the inline role picker for every eligible role count except zero.
/// `WorkspaceChoice` is built fresh each call so manager edits take effect
/// immediately.
pub(crate) fn dispatch_launch_for_workspace(
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
                        title: "No eligible roles".into(),
                        message: format!(
                            "Workspace \"{name}\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
                        ),
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
