//! Thin adapter shell — editor-stage input dispatch lives in jackin-console.

pub(super) use jackin_console::tui::input::editor::{
    EditorModalOutcome, handle_editor_key, handle_editor_modal,
};
pub(in crate::console) use jackin_console::tui::input::editor::apply_file_browser_to_editor;

#[cfg(test)]
pub(super) use jackin_console::tui::input::editor::{
    apply_text_input_to_pending, env_key_input_state,
};
#[cfg(test)]
pub(super) use jackin_console::tui::screens::editor::view::{
    role_load_input_state, secret_new_key_label,
};

#[cfg(test)]
fn poll_role_load(
    editor: &mut crate::console::tui::state::EditorState<'_>,
    config: &mut jackin_config::AppConfig,
    paths: &crate::paths::JackinPaths,
) -> bool {
    use crate::console::tui::state::PendingRoleLoad;
    use jackin_console::tui::app::ConsolePendingRoleLoad as _;
    let Some((load, result)): Option<(PendingRoleLoad, anyhow::Result<()>)> =
        editor.poll_pending_role_load()
    else {
        return false;
    };
    crate::console::effects::apply_role_load_completion(editor, config, paths, load, result);
    true
}

#[cfg(test)]
pub(super) mod tests;
