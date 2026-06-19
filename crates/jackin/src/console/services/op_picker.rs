//! Non-TUI 1Password picker services — thin re-export shell.

pub(crate) use jackin_console::tui::op_picker::start_load;
pub(in crate::console) use jackin_console::tui::op_picker::invalidate_cache_for_ref;
