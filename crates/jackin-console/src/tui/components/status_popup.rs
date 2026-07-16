// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-local status popup state construction.

use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

pub fn status_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> termrock::components::StatusPopupState {
    termrock::components::StatusPopupState::new(title, message)
}

pub fn role_resolution_status_popup_state(
    role_key: impl std::fmt::Display,
) -> termrock::components::StatusPopupState {
    status_popup_state(
        "Resolving agent role",
        format!("Loading and resolving {role_key}"),
    )
}

pub fn role_loading_status_popup_state(
    role_key: impl std::fmt::Display,
) -> termrock::components::StatusPopupState {
    status_popup_state("Loading role", format!("Loading role {role_key}"))
}

pub fn workspace_save_drift_check_status_popup_state() -> termrock::components::StatusPopupState {
    status_popup_state("Saving", "Checking isolation records...")
}

pub fn workspace_save_isolation_cleanup_status_popup_state()
-> termrock::components::StatusPopupState {
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
mod tests;
