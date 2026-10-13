// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sidebar rectangle allocation.

use super::{
    COMPACT_INSTANCES_HEIGHT, ConfigSidebarInputs, SidebarLayout, SidebarLayoutMetrics,
    SidebarScrollArea, SidebarScrollAreas, SidebarScrollFocus, agents_block_agent_count_for_config,
    agents_block_content_width, agents_block_height, config_global_mount_rows_height,
    env_block_height_for_config, global_mounts_content_height_from_rows,
    global_mounts_content_width_from_rows, mount_block_height, scroll_area_axes,
};
use ratatui::layout::Rect;
use termrock::layout::{PanelStackBlock, ShrinkPolicy, panel_stack_with_policy};
use termrock::scroll::ScrollAxes;

use crate::tui::mount_display::{
    workspace_config_mounts_content_height, workspace_config_mounts_content_width_with_cache,
};

#[must_use]
pub fn focused_mount_scroll_area_still_scrollable(
    focus: crate::tui::focus::MountScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    focused_scroll_area_still_scrollable(focus.into(), areas)
}

#[must_use]
pub fn focused_scroll_area_still_scrollable(
    focus: SidebarScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    focused_scroll_area_axes(focus, areas).any()
}

#[must_use]
pub fn focused_scroll_area_axes(
    focus: SidebarScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> ScrollAxes {
    let Some(areas) = areas else {
        return ScrollAxes::none();
    };
    match focus {
        SidebarScrollFocus::Workspace => scroll_area_axes(areas.workspace),
        SidebarScrollFocus::Global => {
            if areas.global.area.height > 0 {
                scroll_area_axes(areas.global)
            } else {
                ScrollAxes::none()
            }
        }
        SidebarScrollFocus::RoleGlobal => areas
            .role_global
            .map_or_else(ScrollAxes::none, scroll_area_axes),
        SidebarScrollFocus::Roles => areas.roles.map_or_else(ScrollAxes::none, scroll_area_axes),
    }
}

#[must_use]
pub fn compute_sidebar_layout(area: Rect, metrics: SidebarLayoutMetrics) -> SidebarLayout {
    // ShrinkPolicy::Equal keeps the previous ratatui `Constraint::Length`
    // solver semantics byte-identical under overflow (equal-strength
    // relaxation), unlike panel_stack's default tail-first shrink.
    let block = |height: u16, visible: bool| PanelStackBlock {
        content_rows: height,
        chrome_rows: 0,
        min: 0,
        max: height,
        visible,
    };
    let blocks = [
        block(COMPACT_INSTANCES_HEIGHT, metrics.instance_count > 0),
        block(3, true),
        block(metrics.workspace_mount_height, true),
        metrics
            .global_mount_height
            .map_or_else(|| block(0, false), |height| block(height, true)),
        metrics
            .role_global_mount_height
            .map_or_else(|| block(0, false), |height| block(height, true)),
        metrics
            .env_height
            .map_or_else(|| block(0, false), |height| block(height, true)),
        block(agents_block_height(metrics.agent_count), metrics.show_roles),
    ];
    let rows = panel_stack_with_policy(area, &blocks, 0, ShrinkPolicy::Equal);
    let mut iter = rows.iter().copied();

    SidebarLayout {
        instances: iter.next().unwrap_or(None),
        general: iter.next().unwrap_or(None).unwrap_or_default(),
        mounts: iter.next().unwrap_or(None).unwrap_or_default(),
        global: iter.next().unwrap_or(None),
        role_global: iter.next().unwrap_or(None),
        env: iter.next().unwrap_or(None),
        roles: iter.next().unwrap_or(None),
    }
}

#[must_use]
pub fn compute_config_sidebar_layout(
    area: Rect,
    inputs: &ConfigSidebarInputs<'_>,
) -> SidebarLayout {
    let (global_rows, role_global_rows) =
        crate::services::workspace::split_global_mount_rows(&inputs.global_rows);
    let show_global_header = !global_rows.is_empty() || role_global_rows.is_empty();
    let show_global = !inputs.global_rows.is_empty() && show_global_header;
    let show_role_global = !role_global_rows.is_empty();
    let show_roles = !inputs.inline_picker_active;

    compute_sidebar_layout(
        area,
        SidebarLayoutMetrics {
            instance_count: inputs.instance_count,
            workspace_mount_height: mount_block_height(
                inputs.mounts.iter().map(|mount| mount.src == mount.dst),
            ),
            global_mount_height: show_global.then(|| config_global_mount_rows_height(&global_rows)),
            role_global_mount_height: show_role_global
                .then(|| config_global_mount_rows_height(&role_global_rows)),
            env_height: inputs
                .show_envs
                .then(|| env_block_height_for_config(inputs.ws_config)),
            show_roles,
            agent_count: inputs.agent_count,
        },
    )
}

#[must_use]
pub fn compute_config_sidebar_scroll_areas(
    area: Rect,
    inputs: &ConfigSidebarInputs<'_>,
    config: &jackin_config::AppConfig,
) -> SidebarScrollAreas {
    let layout = compute_config_sidebar_layout(area, inputs);
    let (global_rows, role_global_rows) =
        crate::services::workspace::split_global_mount_rows(&inputs.global_rows);

    SidebarScrollAreas {
        workspace: SidebarScrollArea {
            area: layout.mounts,
            content_width: workspace_config_mounts_content_width_with_cache(
                inputs.mounts,
                &inputs.mount_info_cache,
            ),
            content_height: workspace_config_mounts_content_height(inputs.mounts),
        },
        global: SidebarScrollArea {
            area: layout.global.unwrap_or(Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 0,
            }),
            content_width: global_mounts_content_width_from_rows(
                &global_rows,
                &inputs.mount_info_cache,
            ),
            content_height: global_mounts_content_height_from_rows(&global_rows),
        },
        role_global: layout.role_global.map(|area| SidebarScrollArea {
            area,
            content_width: global_mounts_content_width_from_rows(
                &role_global_rows,
                &inputs.mount_info_cache,
            ),
            content_height: global_mounts_content_height_from_rows(&role_global_rows),
        }),
        roles: layout.roles.map(|area| SidebarScrollArea {
            area,
            content_width: agents_block_content_width(config.roles.keys().map(String::as_str)),
            content_height: 2 + agents_block_agent_count_for_config(inputs.ws_config, config),
        }),
    }
}
