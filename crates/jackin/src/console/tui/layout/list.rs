//! List-pane geometry: thin adapter shell.

pub(crate) use jackin_console::tui::layout::list::{
    SidebarInputs, SidebarLayout, clamp_list_scroll_for_area, compute_sidebar_layout,
    list_names_content_width, selected_sidebar_scroll_areas, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace,
};
