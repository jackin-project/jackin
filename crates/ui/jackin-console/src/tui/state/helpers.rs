// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{ConfirmTarget, EditorState, Modal};

use jackin_config::AppConfig;

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

pub fn add_role_to_workspace_editor(editor: &mut EditorState<'_>, config: &AppConfig, key: &str) {
    if let Some(idx) = crate::tui::screens::editor::update::add_role_to_workspace_editor(
        &mut editor.pending.allowed_roles,
        config.roles.keys(),
        key,
    ) {
        editor.select_row(idx);
    }
}

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

pub fn open_editor_action_error(editor: &mut EditorState<'_>, err: &dyn std::fmt::Display) {
    record_console_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
    editor.open_error_popup(
        crate::tui::components::error_popup::editor_action_error_popup_state(err),
    );
}

pub fn open_role_input_error(editor: &mut EditorState<'_>, message: &str) {
    record_console_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
    editor.open_error_popup(
        crate::tui::components::error_popup::role_load_error_popup_state(message),
    );
}

pub(crate) fn record_console_error(error_type: jackin_telemetry::schema::enums::ErrorType) {
    let _recorded = jackin_telemetry::record_error(error_type);
}
