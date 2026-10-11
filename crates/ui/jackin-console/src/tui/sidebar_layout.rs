// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure sidebar rectangle allocation for the workspace list preview pane.
mod blocks;
mod inputs;
mod layout;
mod scroll;
#[cfg(test)]
mod tests;
mod types;
#[cfg(test)]
pub(crate) use crate::mount_info_cache::MountInfoCache;
pub use blocks::{
    agents_block_agent_count, agents_block_agent_count_for_config, agents_block_content_width,
    agents_block_height, env_block_height, env_block_height_for_config, global_mount_rows_height,
    global_mounts_content_height, mount_block_height, workspace_has_any_env,
};
pub(crate) use blocks::{
    config_global_mount_rows_height, global_mounts_content_height_from_rows,
    global_mounts_content_width_from_rows,
};
pub use inputs::{
    ConfigSidebarInputs, ConfigSidebarSelectionInputs, SidebarInputs, SidebarInstanceFacts,
    SidebarInstanceQuery, config_sidebar_inputs_for_selection, sidebar_active_instance_count,
};
pub use layout::{
    compute_config_sidebar_layout, compute_config_sidebar_scroll_areas, compute_sidebar_layout,
    focused_mount_scroll_area_still_scrollable, focused_scroll_area_axes,
    focused_scroll_area_still_scrollable,
};
#[cfg(test)]
pub(crate) use ratatui::layout::Rect;
pub(crate) use scroll::mount_data_row_count;
pub use scroll::{clamp_scroll_area, scroll_area_axes, scroll_area_scrollable};
#[cfg(test)]
pub(crate) use termrock::scroll::ScrollAxes;
pub use types::{
    COMPACT_INSTANCES_HEIGHT, GlobalMountRowsSelection, SelectedSidebarTarget, SidebarLayout,
    SidebarLayoutMetrics, SidebarScrollArea, SidebarScrollAreas, SidebarScrollFocus,
    global_mount_rows_selection, inline_picker_active, inline_picker_role, selected_sidebar_target,
};
