// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `error_popup`.
use super::*;

#[test]
fn role_load_error_popup_uses_standard_title() {
    let state = role_load_error_popup_state("bad role");

    assert_eq!(state.title, "Load role failed");
    assert_eq!(state.message, "bad role");
}

#[test]
fn role_load_error_messages_name_configured_or_repository_source() {
    assert_eq!(
        configured_role_load_error_message("bad-role"),
        "Could not load role \"bad-role\".\n\nUse a configured role such as \"agent-smith\" or a GitHub selector like \"owner/agent-name\"."
    );
    assert_eq!(
        repository_role_load_error_message(
            "bad-role",
            "https://example.test/repo.git",
            "not valid"
        ),
        "Could not load role \"bad-role\".\n\nLooked for repository:\nhttps://example.test/repo.git\n\nnot valid"
    );
    assert_eq!(
        internal_role_load_error_message("bad-role", "panic payload"),
        "Could not load role \"bad-role\".\n\nThe role loader hit an internal error while registering the repository.\n\npanic payload"
    );
    assert_eq!(
        role_repository_unavailable_message(),
        "Repository is not available, or you do not have access.",
    );
    assert_eq!(
        role_repository_remote_mismatch_message(),
        "A cached copy already exists for this role, but it points at a different repository.",
    );
    assert_eq!(
        invalid_role_repository_message("missing jackin.role.toml"),
        "Repository is not a valid jackin❯ role: missing jackin.role.toml.",
    );
    assert_eq!(
        generic_role_repository_error_message(),
        "Repository could not be used as a jackin❯ role.",
    );
    assert_eq!(
        missing_role_repository_file_message("jackin.role.toml"),
        "missing jackin.role.toml",
    );
    assert_eq!(
        role_input_misroute_error_message(),
        "Role input was routed through the generic text-input handler."
    );
}

#[test]
fn editor_action_error_popup_names_failed_change() {
    let state = editor_action_error_popup_state("disk full");

    assert_eq!(state.title, "Could not apply change");
    assert!(state.message.contains("disk full"));
}

#[test]
fn no_github_url_error_popup_explains_missing_remote() {
    let state = no_github_url_error_popup_state();

    assert_eq!(state.title, "No GitHub URL");
    assert!(state.message.contains("GitHub origin"));
}

#[test]
fn save_failed_error_popup_uses_standard_title() {
    let state = save_failed_error_popup_state("bad config");

    assert_eq!(state.title, "Save failed");
    assert_eq!(state.message, "bad config");
}

#[test]
fn effect_error_popup_helpers_use_standard_titles() {
    let token = token_generation_failed_error_popup_state("op failed");
    let url = failed_to_open_url_error_popup_state("browser failed");

    assert_eq!(token.title, token_generation_failed_error_title());
    assert_eq!(token.message, "op failed");
    assert_eq!(url.title, failed_to_open_url_error_title());
    assert_eq!(url.message, "browser failed");
    assert_eq!(delete_failed_error_title(), "Delete failed");
    assert_eq!(file_browser_failed_error_title(), "File browser failed");
}

#[test]
fn op_read_failed_error_popup_uses_standard_title() {
    let state = op_read_failed_error_popup_state("Touch ID rejected");

    assert_eq!(state.title, "1Password read failed");
    assert_eq!(state.message, "Touch ID rejected");
}

#[test]
fn role_resolution_error_names_role() {
    assert_eq!(role_resolution_error_title(), "Role resolution failed");
    assert_eq!(
        role_resolution_error_message("agent-smith", "not found"),
        "Could not resolve agent-smith.\n\nnot found"
    );
}

#[test]
fn no_eligible_roles_error_names_workspace() {
    assert_eq!(no_eligible_roles_error_title(), "No eligible roles");
    assert_eq!(
        no_eligible_roles_error_message("demo"),
        "Workspace \"demo\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
    );
}

#[test]
fn list_instance_error_helpers_use_standard_wording() {
    assert_eq!(instance_unavailable_error_title(), "Instance unavailable");
    assert_eq!(
        instance_unavailable_error_message(),
        "Instance no longer active; list refreshes automatically."
    );
    assert_eq!(instance_index_error_title(), "Instance index error");
    assert_eq!(
        instance_index_error_message("missing record"),
        "instance index error: missing record"
    );
    assert_eq!(no_instance_error_title(), "No instance");
    assert_eq!(
        no_recoverable_instance_selected_message(),
        "No recoverable instance selected."
    );
    assert_eq!(
        no_recoverable_instance_for_workspace_message(),
        "No recoverable instance for this workspace."
    );
    assert_eq!(
        no_running_instance_for_workspace_message(),
        "No running instance for this workspace."
    );
    assert_eq!(
        no_instance_state_for_workspace_message(),
        "No instance state for this workspace."
    );
    assert_eq!(
        no_running_instance_to_stop_message(),
        "No running instance to stop."
    );
    assert_eq!(
        no_purgeable_instance_for_workspace_message(),
        "No purgeable instance for this workspace."
    );
}

#[test]
fn instance_action_failed_title_names_in_place_actions() {
    assert_eq!(
        instance_action_failed_error_title(WorkspaceInstanceAction::Stop),
        "Stop failed"
    );
    assert_eq!(
        instance_action_failed_error_title(WorkspaceInstanceAction::Purge),
        "Purge failed"
    );
    assert_eq!(
        instance_action_failed_error_title(WorkspaceInstanceAction::Inspect),
        "Action failed"
    );
    assert_eq!(
        instance_action_failed_error_message(anyhow::anyhow!("docker failed")),
        "docker failed"
    );
}
