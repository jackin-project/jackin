//! List-pane geometry: thin adapter shell.

pub(crate) use jackin_console::tui::layout::list::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace,
};
#[cfg(test)]
pub(crate) use jackin_console::tui::layout::list::clamp_list_scroll_for_area;
