//! Modal and input-state constructors extracted from the view coordinator.
//! Keep the use of `forbidden_secret_keys` from update as specified.

use crate::tui::screens::editor::model::{EditorMode, SecretsScopeTag};

use crate::tui::screens::editor::update::forbidden_secret_keys;

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn editor_header_title(mode: &EditorMode) -> String {
    match mode {
        EditorMode::Edit { name } => format!("edit workspace · {name}"),
        EditorMode::Create => "create workspace".to_owned(),
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn editor_name_value(
    mode: &EditorMode,
    pending_name: Option<&str>,
    create_fallback: &str,
) -> String {
    match mode {
        EditorMode::Edit { name } => pending_name.unwrap_or(name).to_owned(),
        EditorMode::Create => pending_name.unwrap_or(create_fallback).to_owned(),
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(secret_delete_confirm_prompt(key))
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_value_input_state<'a>(
    key: &str,
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(format!("Edit {key}"), current)
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_new_value_input_state<'a>(
    key: &str,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(
        format!("Value for {key}"),
        String::new(),
    )
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_source_picker_state(
    key: impl Into<String>,
    op_available: bool,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), op_available)
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState
{
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
#[allow(unreachable_pub)]
pub fn secret_new_key_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "New workspace environment key".to_owned(),
        SecretsScopeTag::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_new_key_after_picker_label(scope: &SecretsScopeTag) -> String {
    format!("New environment key for {}", secrets_scope_label(scope))
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn role_trust_confirm_state(
    role: String,
    repository: String,
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![("Role".into(), role), ("Repository".into(), repository)],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    )
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn isolated_state_save_confirm_state(
    affected_containers: &[String],
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Edit affects preserved isolated state for {} stopped container(s):\n  {}\n\n\
         Delete the preserved state and save?",
        affected_containers.len(),
        affected_containers.join("\n  "),
    ))
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secrets_scope_label(scope: &SecretsScopeTag) -> &str {
    match scope {
        SecretsScopeTag::Workspace => "workspace",
        SecretsScopeTag::Role(role) => role.as_str(),
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secrets_forbidden_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_owned(),
        SecretsScopeTag::Role(role) => format!("role {role}"),
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_key_input_state<'a>(
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    forbidden_keys: Vec<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state =
        jackin_tui::components::TextInputState::new_with_forbidden(label, initial, forbidden_keys);
    state.forbidden_label = secrets_forbidden_label(scope);
    state
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn secret_key_input_state_from_pending<'a, R, V>(
    workspace_env: &std::collections::BTreeMap<String, V>,
    roles: &std::collections::BTreeMap<String, R>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    role_env: impl Fn(&R) -> &std::collections::BTreeMap<String, V>,
) -> jackin_tui::components::TextInputState<'a> {
    secret_key_input_state(
        scope,
        label,
        initial,
        forbidden_secret_keys(workspace_env, roles, scope, role_env),
    )
}
