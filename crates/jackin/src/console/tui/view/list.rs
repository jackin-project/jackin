//! List-stage rendering: the left-column workspace list and right-pane
//! details (saved workspace / current-directory / "+ New workspace"
//! sentinel).

pub(super) use crate::console::tui::components::workspace_list::render_list_body;

#[cfg(test)]
mod tests;
