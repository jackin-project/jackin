//! Text input state + plan helpers for the Settings sub-views.

use super::*;
use crate::tui::screens::settings::update as settings_update;

#[must_use]
pub const fn settings_header_title() -> &'static str {
    "settings"
}

#[must_use]
pub fn tab_labels(active: SettingsTab) -> Vec<(&'static str, bool)> {
    SettingsTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub const fn global_mount_confirm_prompt(action: GlobalMountConfirm) -> &'static str {
    match action {
        GlobalMountConfirm::Save => "Save settings to ~/.config/jackin/config.toml?",
        GlobalMountConfirm::Sensitive => "Sensitive global mount path detected. Save anyway?",
        GlobalMountConfirm::Remove => "Remove selected global mount?",
        GlobalMountConfirm::Discard => "Discard unsaved global mount changes?",
    }
}

#[must_use]
pub fn global_mount_confirm_state(
    action: GlobalMountConfirm,
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(global_mount_confirm_prompt(action))
}

#[must_use]
pub fn global_mount_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::with_title(
        " Which agent role do you want to add? ",
    )
}

#[must_use]
pub fn global_mount_text_input_state<'a>(
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new(label, initial)
}

#[must_use]
pub fn global_mount_scope_text_value(scope: Option<&str>) -> String {
    scope.unwrap_or_default().to_owned()
}

#[must_use]
pub fn global_mount_edit_text_initial(
    row: &jackin_config::GlobalMountRow,
    target: &GlobalMountTextTarget,
) -> Option<String> {
    match target {
        GlobalMountTextTarget::Rename => Some(row.name.clone()),
        GlobalMountTextTarget::Source => Some(row.mount.src.clone()),
        GlobalMountTextTarget::Destination => Some(row.mount.dst.clone()),
        GlobalMountTextTarget::Scope => Some(global_mount_scope_text_value(row.scope.as_deref())),
        GlobalMountTextTarget::AddScope
        | GlobalMountTextTarget::AddName
        | GlobalMountTextTarget::AddSource
        | GlobalMountTextTarget::AddDestination => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMountEditTextPlan {
    pub target: GlobalMountTextTarget,
    pub label: &'static str,
    pub initial: String,
}

#[must_use]
pub fn global_mount_selected_edit_text_plan(
    rows: &[jackin_config::GlobalMountRow],
    selected: usize,
    target: GlobalMountTextTarget,
) -> Option<GlobalMountEditTextPlan> {
    let row = rows.get(selected)?;
    let initial = global_mount_edit_text_initial(row, &target)?;
    let label = global_mount_text_target_label(&target)?;
    Some(GlobalMountEditTextPlan {
        target,
        label,
        initial,
    })
}

#[must_use]
pub const fn global_mount_text_target_label(
    target: &GlobalMountTextTarget,
) -> Option<&'static str> {
    match target {
        GlobalMountTextTarget::AddScope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::AddName => Some("Mount name"),
        GlobalMountTextTarget::AddSource => Some("Source"),
        GlobalMountTextTarget::AddDestination => Some("Destination"),
        GlobalMountTextTarget::Source => Some("Source"),
        GlobalMountTextTarget::Destination => Some("Destination"),
        GlobalMountTextTarget::Scope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::Rename => Some("Rename mount"),
    }
}

#[must_use]
pub fn settings_env_text_input_state<'a>(
    target: &SettingsEnvTextTarget,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    if matches!(target, SettingsEnvTextTarget::EnvValue { .. }) {
        jackin_tui::components::TextInputState::new_allow_empty(label, initial)
    } else {
        jackin_tui::components::TextInputState::new(label, initial)
    }
}

#[must_use]
pub fn settings_env_value_text_label(key: &str) -> String {
    format!("Edit {key}")
}

#[must_use]
pub fn settings_env_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvValueEditTextPlan {
    pub target: SettingsEnvTextTarget,
    pub label: String,
    pub current: String,
}

#[must_use]
pub fn settings_env_value_edit_text_plan(
    pending: &SettingsEnvConfig<jackin_core::EnvValue>,
    scope: SettingsEnvScope,
    key: String,
) -> SettingsEnvValueEditTextPlan {
    let value = settings_update::settings_env_value(pending, &scope, &key);
    let current =
        settings_env_value_current_text(value.map(jackin_core::EnvValue::as_persisted_str));
    SettingsEnvValueEditTextPlan {
        target: SettingsEnvTextTarget::EnvValue {
            scope,
            key: key.clone(),
        },
        label: settings_env_value_text_label(&key),
        current,
    }
}

#[must_use]
pub fn settings_env_plain_value_text_plan(
    scope: SettingsEnvScope,
    key: String,
) -> SettingsEnvValueEditTextPlan {
    SettingsEnvValueEditTextPlan {
        target: SettingsEnvTextTarget::EnvValue {
            scope,
            key: key.clone(),
        },
        label: settings_env_value_text_label(&key),
        current: String::new(),
    }
}

#[must_use]
pub fn settings_env_source_picker_state(
    key: impl Into<String>,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), true)
}

#[must_use]
pub fn settings_env_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
pub fn settings_env_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
pub fn settings_env_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(settings_env_delete_confirm_prompt(key))
}

#[must_use]
pub fn env_scope_label(scope: &SettingsEnvScope) -> &str {
    match scope {
        SettingsEnvScope::Global => "global",
        SettingsEnvScope::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn settings_env_new_key_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "New global environment key".to_owned(),
        SettingsEnvScope::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
pub fn settings_env_new_key_after_picker_label(scope: &SettingsEnvScope) -> String {
    format!("New environment key for {}", env_scope_label(scope))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvKeyTextPlan {
    pub scope: SettingsEnvScope,
    pub target: SettingsEnvTextTarget,
    pub label: String,
}

#[must_use]
pub fn settings_env_key_text_plan(
    scope: SettingsEnvScope,
    label: impl Into<String>,
) -> SettingsEnvKeyTextPlan {
    SettingsEnvKeyTextPlan {
        target: SettingsEnvTextTarget::EnvKey {
            scope: scope.clone(),
        },
        scope,
        label: label.into(),
    }
}

#[must_use]
pub fn settings_env_new_key_text_plan(scope: SettingsEnvScope) -> SettingsEnvKeyTextPlan {
    let label = settings_env_new_key_label(&scope);
    settings_env_key_text_plan(scope, label)
}

#[must_use]
pub fn settings_env_new_key_after_picker_text_plan(
    scope: SettingsEnvScope,
) -> SettingsEnvKeyTextPlan {
    let label = settings_env_new_key_after_picker_label(&scope);
    settings_env_key_text_plan(scope, label)
}

#[must_use]
pub fn settings_env_empty_key_text_plan(scope: SettingsEnvScope) -> SettingsEnvKeyTextPlan {
    settings_env_key_text_plan(scope, settings_env_empty_key_label())
}

#[must_use]
pub fn settings_env_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
pub fn settings_env_empty_key_error_message() -> &'static str {
    "Env key cannot be empty."
}

#[must_use]
pub fn global_mount_name_empty_message() -> &'static str {
    "Mount name cannot be empty."
}

#[must_use]
pub fn global_mount_gone_message() -> &'static str {
    "Mount no longer exists; selection was cleared."
}

#[must_use]
pub fn global_mount_add_draft_lost_message() -> &'static str {
    "Add-mount draft was lost; press 'a' to start over."
}

#[must_use]
pub fn global_mount_destination_empty_message() -> &'static str {
    "Mount destination cannot be empty."
}

#[must_use]
pub fn global_mount_no_github_url_message() -> &'static str {
    "no GitHub URL for this mount"
}

#[must_use]
pub fn settings_no_registered_roles_error_message() -> &'static str {
    "No registered roles available."
}

#[must_use]
pub fn settings_sensitive_paths_not_confirmed_message() -> &'static str {
    "Save aborted: sensitive paths not confirmed."
}

#[must_use]
pub fn settings_error_popup_title() -> &'static str {
    "Settings error"
}

#[must_use]
pub fn settings_auth_op_read_failed_message(error: impl std::fmt::Display) -> String {
    format!("1Password read failed: {error}")
}

#[must_use]
pub fn env_forbidden_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_owned(),
        SettingsEnvScope::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn settings_env_key_input_state<'a, V>(
    pending: &SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state = jackin_tui::components::TextInputState::new_with_forbidden(
        label,
        initial,
        forbidden_settings_env_keys(pending, scope),
    );
    state.forbidden_label = env_forbidden_label(scope);
    state
}
