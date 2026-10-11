// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor and role state helpers.

use super::{ConfirmTarget, EditorState, Modal};

use jackin_config::AppConfig;

/// Filter instances matching a query that are `Active` or `Running`.
pub fn active_instances_matching<'a>(
    instances: &'a [jackin_core::InstanceIndexEntry],
    query: jackin_core::InstanceQuery<'a>,
) -> impl Iterator<Item = &'a jackin_core::InstanceIndexEntry> {
    instances.iter().filter(move |e| {
        e.matches(query)
            && matches!(
                e.status,
                jackin_core::InstanceStatus::Active | jackin_core::InstanceStatus::Running
            )
    })
}

/// Filter instances matching a query that are visible in the console tree —
/// every status except `Purged` (no on-disk state) and `Superseded`
/// (replaced by a newer instance). Live and failed/stopped instances
/// alike appear so the operator can restore, restart, or delete them (D15).
pub fn visible_instances_matching<'a>(
    instances: &'a [jackin_core::InstanceIndexEntry],
    query: jackin_core::InstanceQuery<'a>,
) -> impl Iterator<Item = &'a jackin_core::InstanceIndexEntry> {
    instances.iter().filter(move |e| {
        e.matches(query)
            && !matches!(
                e.status,
                jackin_core::InstanceStatus::Purged | jackin_core::InstanceStatus::Superseded
            )
    })
}

/// Add a role to a workspace editor and select its row.
pub fn add_role_to_workspace_editor(editor: &mut EditorState<'_>, config: &AppConfig, key: &str) {
    if let Some(idx) = crate::tui::screens::editor::update::add_role_to_workspace_editor(
        &mut editor.pending.allowed_roles,
        config.roles.keys(),
        key,
    ) {
        editor.select_row(idx);
    }
}

/// Open the role trust confirm dialog.
pub fn open_role_trust_confirm(
    editor: &mut EditorState<'_>,
    key: String,
    source: jackin_config::RoleSource,
) {
    let state = crate::tui::screens::editor::view::role_trust_confirm_state(
        key.clone(),
        source.git.clone(),
    );
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::TrustRoleSource { key, source },
        state,
    });
}

/// Open an editor action error popup with the given error.
pub fn open_editor_action_error(editor: &mut EditorState<'_>, err: &dyn std::fmt::Display) {
    record_console_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
    editor.open_error_popup(
        crate::tui::components::error_popup::editor_action_error_popup_state(err),
    );
}

/// Open a role-input error popup with the given message.
pub fn open_role_input_error(editor: &mut EditorState<'_>, message: &str) {
    record_console_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
    editor.open_error_popup(
        crate::tui::components::error_popup::role_load_error_popup_state(message),
    );
}

pub(crate) fn record_console_error(error_type: jackin_telemetry::schema::enums::ErrorType) {
    let _recorded = jackin_telemetry::record_error(error_type);
}
