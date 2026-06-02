//! Console-local status popup state construction.

use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

pub fn status_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::StatusPopupState {
    jackin_tui::components::StatusPopupState::new(title, message)
}

pub fn role_resolution_status_popup_state(
    role_key: impl std::fmt::Display,
) -> jackin_tui::components::StatusPopupState {
    status_popup_state(
        "Resolving agent role",
        format!("Loading and resolving {role_key}"),
    )
}

pub fn role_loading_status_popup_state(
    role_key: impl std::fmt::Display,
) -> jackin_tui::components::StatusPopupState {
    status_popup_state("Loading role", format!("Loading role {role_key}"))
}

pub fn workspace_save_drift_check_status_popup_state() -> jackin_tui::components::StatusPopupState {
    status_popup_state("Saving", "Checking isolation records...")
}

pub fn workspace_save_isolation_cleanup_status_popup_state()
-> jackin_tui::components::StatusPopupState {
    status_popup_state("Saving", "Deleting isolated state...")
}

pub fn instance_action_busy_title(action: WorkspaceInstanceAction) -> &'static str {
    match action {
        WorkspaceInstanceAction::Stop => "Stopping",
        WorkspaceInstanceAction::Purge => "Purging",
        _ => "Working",
    }
}

pub fn instance_action_busy_message(
    action: WorkspaceInstanceAction,
    container: impl std::fmt::Display,
) -> String {
    format!("{} {container}…", instance_action_busy_title(action))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_resolution_status_popup_names_role() {
        let state = role_resolution_status_popup_state("agent-smith");
        let debug = format!("{state:?}");

        assert!(debug.contains("Resolving agent role"));
        assert!(debug.contains("Loading and resolving agent-smith"));
    }

    #[test]
    fn role_loading_status_popup_names_role() {
        let state = role_loading_status_popup_state("agent-smith");
        let debug = format!("{state:?}");

        assert!(debug.contains("Loading role"));
        assert!(debug.contains("Loading role agent-smith"));
    }

    #[test]
    fn workspace_save_status_popups_name_background_work() {
        let drift = workspace_save_drift_check_status_popup_state();
        let cleanup = workspace_save_isolation_cleanup_status_popup_state();
        let drift_debug = format!("{drift:?}");
        let cleanup_debug = format!("{cleanup:?}");

        assert!(drift_debug.contains("Saving"));
        assert!(drift_debug.contains("Checking isolation records..."));
        assert!(cleanup_debug.contains("Saving"));
        assert!(cleanup_debug.contains("Deleting isolated state..."));
    }

    #[test]
    fn instance_action_busy_wording_names_action_and_container() {
        assert_eq!(
            instance_action_busy_title(WorkspaceInstanceAction::Stop),
            "Stopping"
        );
        assert_eq!(
            instance_action_busy_message(WorkspaceInstanceAction::Stop, "abc123"),
            "Stopping abc123…"
        );
        assert_eq!(
            instance_action_busy_title(WorkspaceInstanceAction::Purge),
            "Purging"
        );
        assert_eq!(
            instance_action_busy_message(WorkspaceInstanceAction::Purge, "abc123"),
            "Purging abc123…"
        );
        assert_eq!(
            instance_action_busy_title(WorkspaceInstanceAction::Inspect),
            "Working"
        );
    }
}
