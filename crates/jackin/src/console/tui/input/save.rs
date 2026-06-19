//! Editor save flow: thin adapter shell.

pub(in crate::console) use jackin_console::tui::input::save::{
    begin_editor_save, commit_editor_save, continue_save_after_drift_check,
    continue_save_after_isolation_cleanup, open_save_error_popup,
};
