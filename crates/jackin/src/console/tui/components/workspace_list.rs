//! Root-console workspace-list display adapters: thin shell.

#[cfg(test)]
pub(crate) use jackin_console::tui::screens::workspaces::view::list::{
    render_details_pane, render_list_body,
};

#[cfg(test)]
mod tests;
