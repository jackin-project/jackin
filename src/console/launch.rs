use crate::app::context::preferred_agent_index;
use crate::config::AppConfig;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

use super::{ConsoleStage, ConsoleState, build_workspace_choice, manager, preview};

impl ConsoleState {
    /// Open the inline role picker for every eligible role count except zero.
    /// `WorkspaceChoice` is built fresh each call so manager edits take effect
    /// immediately.
    pub fn dispatch_launch_for_workspace(
        &mut self,
        config: &AppConfig,
        cwd: &std::path::Path,
        input: LoadWorkspaceInput,
    ) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>)>>
    {
        let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
            // Workspace was deleted between keypress and dispatch.
            return Ok(None);
        };
        let roles = choice.allowed_roles.clone();

        if roles.is_empty() {
            // Stay so the operator can fix `allowed_roles`
            // — a single Enter shouldn't terminate the TUI.
            let name = choice.name;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                let _ = manager::update_manager(
                    ms,
                    manager::ManagerMessage::OpenListErrorPopup {
                        title: "No eligible roles".into(),
                        message: format!(
                            "Workspace \"{name}\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
                        ),
                    },
                );
            }
            self.pending_launch = None;
            self.pending_launch_role = None;
        } else if roles.len() == 1 {
            // Single role — skip picker and proceed directly to agent selection.
            let role = roles.into_iter().next().unwrap();
            return preview::resolve_selected_workspace(config, cwd, &choice, &role)
                .map(|workspace| Some((role, workspace, None)));
        } else {
            let selected = preferred_agent_index(
                &roles,
                choice.last_role.as_deref(),
                choice.default_role.as_deref(),
            );
            self.pending_launch = Some(input);
            self.pending_launch_role = None;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                let mut picker =
                    crate::selector::RolePickerState::with_confirm_label(roles, "launch");
                if let Some(selected) = selected {
                    picker.list_state.select(Some(selected));
                }
                ms.inline_role_picker = Some(picker);
            }
        }
        Ok(None)
    }
}
