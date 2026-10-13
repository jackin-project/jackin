// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `list`.

use crate::tui::components::mount_rows::render_mount_lines;

use crate::tui::components::mount_rows::{MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH};

use crate::tui::mount_display::MountDisplayRow;

use crate::tui::mount_display::format_config_mount_rows as format_mount_rows;

use crate::tui::mount_display::mount_path_width;

use super::{instance_details_pane, render_details_pane, render_list_body};

use crate::tui::layout::list::clamp_list_scroll_for_area;

use crate::tui::layout::list::list_names_content_width;

use crate::tui::screens::workspaces::view::WorkspaceInstancePaneContent;

use crate::tui::state::{ManagerListRow, ManagerState};

use jackin_config::AppConfig;

use jackin_config::WorkspaceConfig;

use ratatui::Terminal;

use ratatui::backend::TestBackend;

use ratatui::layout::Rect;

use termrock::scroll::max_offset;

use crate::tui::components::mount_rows::render_mount_header;

use jackin_config::MountConfig;

use crate::tui::screens::workspaces::view::{
    render_config_mounts_subpanel as render_mounts_subpanel, render_config_roles_subpanel,
    render_environments_subpanel, render_general_subpanel, workspace_env_rows,
};

use crate::tui::state::{MountInfoCache, WorkspaceSummary};

use ratatui::Frame;

use ratatui::buffer::Buffer;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
