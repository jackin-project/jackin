//! Root-console workspace-list display adapters: thin shell.
pub(crate) use jackin_console::tui::screens::workspaces::view::list::render_list_body;

#[cfg(test)]
pub(crate) use jackin_console::tui::screens::workspaces::view::list::render_details_pane;

#[cfg(test)]
mod tests;
