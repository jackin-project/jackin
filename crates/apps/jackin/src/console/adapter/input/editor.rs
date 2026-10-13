// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Thin adapter shell — editor-stage input dispatch lives in jackin-console.

#[cfg(test)]
pub(super) use jackin_console::tui::input::editor::{
    EditorModalOutcome, apply_file_browser_to_editor, apply_text_input_to_pending,
    env_key_input_state, handle_editor_modal,
};
#[cfg(test)]
pub(super) use jackin_console::tui::screens::editor::view::{
    role_load_input_state, secret_new_key_label,
};

#[cfg(test)]
pub(super) fn poll_role_load(
    editor: &mut crate::console::adapter::state::EditorState<'_>,
    config: &mut jackin_config::AppConfig,
    paths: &jackin_core::JackinPaths,
) -> bool {
    use crate::console::adapter::state::PendingRoleLoad;
    use jackin_console::tui::model::ConsolePendingRoleLoad as _;
    let Some((load, result)): Option<(PendingRoleLoad, anyhow::Result<()>)> =
        editor.poll_pending_role_load()
    else {
        return false;
    };
    crate::console::effects::apply_role_load_completion_for_tests(
        editor, config, paths, load, result,
    );
    true
}

#[cfg(test)]
mod tests;
