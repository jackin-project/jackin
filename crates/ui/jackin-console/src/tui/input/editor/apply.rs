// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorModalOutcome` application helpers.

use crate::tui::screens::editor::view::{mount_destination_input_state, mount_dst_choice_state};

use crate::tui::state::{
    ConfirmTarget, EditorState, FileBrowserTarget, Modal, TextInputTarget, open_role_input_error,
};
use crate::tui::update::{MountDstChoicePlan, mount_dst_choice_plan};
use jackin_config::AppConfig;

pub type EditorModalOutcome = crate::tui::message::ConsoleEditorModalOutcome<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

pub(crate) fn apply_role_input(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    value: &str,
) -> EditorModalOutcome {
    match crate::services::role_source::resolve_role_input_source(config, value) {
        Ok(resolved) => EditorModalOutcome::StartRoleRegistration {
            raw: resolved.raw,
            key: resolved.key,
            selector: resolved.selector,
            source: resolved.source,
        },
        Err(e) => {
            let err_text = e.error.to_string();
            if let Some(panic_message) = err_text.strip_prefix("role loader panicked: ") {
                let message = crate::tui::components::error_popup::internal_role_load_error_message(
                    &e.raw,
                    panic_message,
                );
                open_role_input_error(editor, &message);
                return EditorModalOutcome::Continue;
            }
            open_role_resolution_error(editor, &e.raw, e.source_url.as_ref(), &e.error);
            EditorModalOutcome::Continue
        }
    }
}

pub(crate) fn apply_editor_confirm(
    editor: &mut EditorState<'_>,
    target: &ConfirmTarget,
) -> anyhow::Result<EditorModalOutcome> {
    match target {
        ConfirmTarget::DeleteEnvVar { scope, key } => {
            editor.delete_env_var(scope, key)?;
        }
        ConfirmTarget::TrustRoleSource { key, source } => {
            return Ok(EditorModalOutcome::PersistTrustedRoleSource {
                key: key.clone(),
                source: source.clone(),
            });
        }
        // `DeleteIsolatedAndSave` is handled inline at the dispatch
        // site because it consumes `plan` and routes through
        // `EditorSaveFlow::PendingCommit`. No-op here.
        ConfirmTarget::DeleteIsolatedAndSave { .. } => {}
    }
    Ok(EditorModalOutcome::Continue)
}

/// Only `EditAddMountSrc` is meaningful here; the prelude's
/// `CreateFirstMountSrc` target routes through `handle_prelude_modal`.
pub(crate) fn dispatch_editor_mount_dst_choice(
    editor: &mut EditorState<'_>,
    target: FileBrowserTarget,
    src: &str,
    outcome: &jackin_oppicker::ModalOutcome<
        crate::tui::components::mount_dst_choice::MountDstChoice,
    >,
) {
    match mount_dst_choice_plan(outcome.clone()) {
        MountDstChoicePlan::CommitSamePath => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.add_shared_mount(src, src);
            }
            editor.clear_modal_chain();
        }
        MountDstChoicePlan::OpenEditInput => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.add_shared_mount(src, src);
                editor.open_sub_modal(Modal::TextInput {
                    target: TextInputTarget::MountDst,
                    state: mount_destination_input_state(src),
                });
            } else {
                editor.clear_modal_chain();
            }
        }
        MountDstChoicePlan::Dismiss => {
            editor.pop_modal_chain();
        }
        MountDstChoicePlan::Continue => {}
    }
}

pub fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Defer the mount push to the choice modal: in the common case
            // the operator will take "Mount at same path" (dst = src) and we skip the
            // TextInput entirely. Only the `Edit destination` branch pushes
            // a provisional mount and opens the TextInput.
            editor.open_sub_modal(Modal::MountDstChoice {
                target,
                state: mount_dst_choice_state(path.display().to_string()),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — handled by
            // `handle_prelude_modal`.
            drop((editor, path));
        }
        FileBrowserTarget::AuthFormSourceFolder => {
            super::super::auth::apply_source_folder_to_auth_form(editor, path);
        }
    }
}

pub(crate) fn open_role_resolution_error(
    editor: &mut EditorState<'_>,
    raw: &str,
    source_url: Option<&String>,
    _err: &anyhow::Error,
) {
    use crate::tui::components::error_popup::{
        configured_role_load_error_message, generic_role_repository_error_message,
        repository_role_load_error_message,
    };
    crate::tui::state::record_console_error(
        jackin_telemetry::schema::enums::ErrorType::ConfigError,
    );
    let message = source_url.map_or_else(
        || configured_role_load_error_message(raw),
        |source_url| {
            repository_role_load_error_message(
                raw,
                source_url,
                generic_role_repository_error_message(),
            )
        },
    );
    editor.open_error_popup(
        crate::tui::components::error_popup::role_load_error_popup_state(message),
    );
}
