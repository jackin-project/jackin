// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Root-console workspace-list display adapters.
mod body;
mod details;
mod panes;
mod sidebar;
#[cfg(test)]
mod tests;
pub use body::{list_name_lines, render_list_body};
pub use details::instance_details_pane;
pub use panes::{
    render_account_picker_sidebar, render_current_dir_details_pane, render_details_pane,
    render_instance_details_pane, render_sidebar_body,
};
pub use sidebar::render_list_sidebar;
