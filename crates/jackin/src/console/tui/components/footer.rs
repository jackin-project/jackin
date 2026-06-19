//! Root footer adapter shells — workspace list and screen footer logic lives in jackin-console.

pub(crate) mod editor;
pub(crate) mod settings;

pub(crate) use jackin_console::tui::screens::workspaces::view::footer::workspace_list_footer_items_for_state;
