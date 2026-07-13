// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console TUI launch flow: role picker → launch confirmation → resolved pair.
//!
//! Returns a resolved `(RoleSelector, ResolvedWorkspace)` pair for the caller
//! to act on; does not launch or connect to a container.

use jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace};
use jackin_core::RoleSelector;

use crate::services::launch::LaunchDispatchResolution;
use crate::tui::components::error_popup::{
    no_eligible_roles_error_message, no_eligible_roles_error_title,
};
use crate::tui::console::{ConsoleStage, ConsoleState};
use crate::tui::model::{clear_pending_launch_plan, open_launch_role_prompt_plan};
use crate::tui::state::update::{ManagerMessage, update_manager};

pub fn dispatch_launch_for_workspace(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<jackin_core::Agent>)>> {
    let Some(resolution) = crate::services::launch::resolve_launch_dispatch(config, cwd, input)?
    else {
        return Ok(None);
    };

    match resolution {
        LaunchDispatchResolution::NoEligibleRoles { name } => {
            let ConsoleStage::Manager(ms) = &mut state.stage;
            let _unused = update_manager(
                ms,
                ManagerMessage::OpenListErrorPopup {
                    title: no_eligible_roles_error_title().into(),
                    message: no_eligible_roles_error_message(name),
                },
            );
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
