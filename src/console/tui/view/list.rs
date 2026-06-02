//! List-stage rendering: the left-column workspace list and right-pane
//! details (saved workspace / current-directory / "+ New workspace"
//! sentinel).
#![expect(
    clippy::redundant_pub_crate,
    reason = "manager update code uses selected render geometry helpers through the moved tui facade"
)]
#![expect(
    clippy::too_many_lines,
    reason = "Phase 9 only moves render files; later component splits own shortening these helpers"
)]
#![expect(
    clippy::too_many_arguments,
    reason = "existing row-builder shape is preserved during the directory migration"
)]
#![expect(
    clippy::fn_params_excessive_bools,
    reason = "existing row-builder shape is preserved during the directory migration"
)]

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::config::AppConfig;
use crate::console::tui::layout::list::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace, split_global_mount_rows,
};
#[cfg(test)]
pub(super) use crate::console::tui::layout::list::{
    global_mounts_content_height, mount_block_height,
};
#[cfg(test)]
pub(super) use crate::console::tui::components::mount_display::format_mount_rows;
pub(super) use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
#[cfg(test)]
pub(super) use crate::console::tui::components::mount_display::mount_path_width;
use crate::console::tui::components::workspace_list::{
    instance_details_pane, list_name_lines, render_agent_picker_sidebar,
    render_agents_subpanel_scrollable, render_provider_picker_sidebar, render_role_picker_sidebar,
    workspace_env_rows,
};
use crate::console::tui::state::{
    ManagerListRow, ManagerState, MountInfoCache, MountScrollFocus, WorkspaceSummary,
};
#[cfg(test)]
pub(super) use jackin_console::tui::components::mount_rows::render_mount_lines;
#[cfg(test)]
pub(super) use jackin_console::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH,
};
#[cfg(test)]
pub(super) use jackin_console::mount_display::MountDisplayRow;
use jackin_console::tui::screens::workspaces::view::{
    render_compact_instances_summary, render_list_names_block,
    render_environments_subpanel, render_general_subpanel, render_global_mounts_subpanel,
    render_mounts_subpanel as render_workspace_mounts_panel,
    render_instance_details_pane as render_workspace_instance_details_pane,
    render_sentinel_description_pane,
};

#[allow(clippy::too_many_lines)]
pub(super) fn render_list_body(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // See ManagerListRow docs for row layout.
    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   CurrentDirectory  → current-dir details
    //   SavedWorkspace(i) → saved-workspace details
    //   NewWorkspace      → description-of-what-a-workspace-is pane
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let list_area = columns[0];

    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            render_current_dir_details_pane(frame, columns[1], cwd, config, state);
        }
        ManagerListRow::NewWorkspace => {
            render_sentinel_description_pane(frame, columns[1]);
        }
        ManagerListRow::SavedWorkspace(i) => {
            if let Some(ws) = state.workspaces.get(i).cloned() {
                render_details_pane(frame, columns[1], i, &ws, config, state);
            }
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
            let instances = state.workspace_active_instances(ws_idx);
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            let instances = state.current_dir_active_instances();
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
    }

    if let Some(picker) = state.inline_provider_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(&picker.context)
            .unwrap_or(picker.context.as_str());
        render_provider_picker_sidebar(
            frame,
            list_area,
            Some(short_id),
            picker.providers(),
            picker.selected(),
        );
    } else if let Some(picker) = state.launch_provider_picker.as_ref() {
        render_provider_picker_sidebar(
            frame,
            list_area,
            None,
            picker.providers(),
            picker.selected(),
        );
    } else if let Some((container, picker, _providers)) = state.inline_new_session_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(container)
            .unwrap_or(container);
        render_agent_picker_sidebar(frame, list_area, short_id, picker, state.list_names_focused);
    } else if let Some((role, picker)) = state.inline_agent_picker.as_ref() {
        render_agent_picker_sidebar(
            frame,
            list_area,
            &role.key(),
            picker,
            state.list_names_focused,
        );
    } else if let Some(picker) = state.inline_role_picker.as_ref() {
        let title = state
            .selected_workspace_summary()
            .map_or("Current directory", |summary| summary.name.as_str());
        render_role_picker_sidebar(frame, list_area, title, picker, state.list_names_focused);
    } else {
        let (list_lines, content_width) =
            list_name_lines(state, super::scroll_viewport_width(list_area));
        render_list_names_block(
            frame,
            list_area,
            list_lines,
            content_width,
            state.list_names_focused,
            state.list_names_scroll_x,
        );
    }
}

fn render_sidebar_body(
    frame: &mut Frame,
    layout: &SidebarLayout,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    if let Some(area) = layout.instances {
        render_compact_instances_summary(
            frame,
            area,
            inputs.instance_count,
            inputs.instance_expanded,
        );
    }
    render_general_subpanel(
        frame,
        layout.general,
        &crate::tui::shorten_home(inputs.workdir),
    );
    let ws_focused = state.list_scroll_focus == Some(MountScrollFocus::Workspace);
    render_mounts_subpanel(
        frame,
        layout.mounts,
        inputs.mounts,
        &inputs.mount_info_cache,
        state.list_mounts_scroll_x,
        state.list_mounts_scroll_y,
        ws_focused,
    );
    if layout.global.is_some() || layout.role_global.is_some() {
        let global_focused = state.list_scroll_focus;
        let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);
        if let Some(area) = layout.global {
            render_global_mount_rows_section(
                frame,
                area,
                " Global mounts ",
                &global_rows,
                &inputs.mount_info_cache,
                state.list_global_mounts_scroll_x,
                state.list_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::Global),
            );
        }
        if let Some(area) = layout.role_global {
            let title = format!(" Role global mounts · {} ", inputs.picker_role_label);
            render_global_mount_rows_section(
                frame,
                area,
                &title,
                &role_global_rows,
                &inputs.mount_info_cache,
                state.list_role_global_mounts_scroll_x,
                state.list_role_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::RoleGlobal),
            );
        }
    }
    if let Some(area) = layout.env {
        render_environments_subpanel(frame, area, workspace_env_rows(inputs.ws_config));
    }
    if let Some(area) = layout.roles {
        let roles_focused = state.list_scroll_focus == Some(MountScrollFocus::Roles);
        render_agents_subpanel_scrollable(
            frame,
            area,
            inputs.ws_config,
            config,
            state.list_roles_scroll_x,
            state.list_roles_scroll_y,
            roles_focused,
        );
    }
}

fn render_details_pane(
    frame: &mut Frame,
    area: Rect,
    _ws_idx: usize,
    ws: &WorkspaceSummary,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let inputs = sidebar_inputs_for_workspace(ws, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

/// Cursor on the synthetic "Current directory" row — mirrors
/// `workspace::current_dir_workspace`: src=dst=cwd, rw, any role.
fn render_current_dir_details_pane(
    frame: &mut Frame,
    area: Rect,
    cwd: &std::path::Path,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let cwd_str = cwd.display().to_string();
    let mounts = [crate::console::domain::current_dir_mount_config(&cwd_str)];
    let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

/// Right-panel shown when operator selects an instance row in the tree.
/// When the daemon's bind-mounted socket gives us a live snapshot we
/// render the tab/pane tree (active tab marked, focused pane marked,
/// per-pane agent + state); otherwise we fall back to the on-disk
/// manifest sessions, and finally to a "no sessions recorded" hint
/// when neither is available.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn render_instance_details_pane(
    frame: &mut Frame,
    area: Rect,
    entry: &crate::instance::InstanceIndexEntry,
    sessions: &[crate::instance::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&crate::runtime::snapshot::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) {
    let pane = instance_details_pane(
        entry,
        sessions,
        session_load_error,
        snapshot,
        selected_pane,
        preview_focused,
    );
    render_workspace_instance_details_pane(frame, area, &pane);
}

/// Number of leading spaces every content row in the General / Mounts /
/// Environments / Roles sub-panels is prefixed with, so the first visible
/// character lines up across all blocks (at
/// `border_col + SUBPANEL_CONTENT_INDENT`). Pinned by
/// `subpanel_content_column_alignment` in the visual regression tests.
#[cfg(test)]
const SUBPANEL_CONTENT_INDENT: usize = 2;

fn render_mounts_subpanel(
    frame: &mut Frame,
    area: Rect,
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let rows = format_mount_rows_with_cache(mounts, cache);
    render_workspace_mounts_panel(frame, area, &rows, scroll_x, scroll_y, focused);
}

fn render_global_mount_rows_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: &[&crate::config::GlobalMountRow],
    cache: &MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mounts: Vec<crate::workspace::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    render_global_mounts_subpanel(frame, area, title, &display_rows, scroll_x, scroll_y, focused);
}

#[cfg(test)]
mod list_name_scroll_tests {
    use super::render_list_body;
    use jackin_tui::theme::{PHOSPHOR_GREEN, TAB_BG_INACTIVE_HOVER};
    use crate::config::AppConfig;
    use crate::console::tui::layout::list::{
        clamp_list_scroll_for_area, list_names_content_width,
    };
    use crate::console::tui::state::{ManagerListRow, ManagerState};
    use crate::workspace::WorkspaceConfig;
    use jackin_tui::components::scrollable_panel::max_offset;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn config_with_long_workspace_name() -> AppConfig {
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "chainargos-blockchain-nodes".into(),
            WorkspaceConfig::default(),
        );
        config
    }

    fn config_with_short_selected_and_long_sibling() -> AppConfig {
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("jackin".into(), WorkspaceConfig::default());
        config.workspaces.insert(
            "chainargos-blockchain-nodes".into(),
            WorkspaceConfig::default(),
        );
        config
    }

    fn config_with_sidebar_names_that_fit_wide_pane() -> AppConfig {
        let mut config = AppConfig::default();
        for name in [
            "chainargos",
            "chainargos-blockchain-nodes",
            "jackin",
            "parallax",
            "scentbird",
        ] {
            config
                .workspaces
                .insert(name.into(), WorkspaceConfig::default());
        }
        config
    }

    #[test]
    fn list_names_content_width_includes_trailing_scroll_padding() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let state = ManagerState::from_config(&config, tmp.path());

        // Rows without active instances: cursor(1) + 2 spaces + name(27) = 30 cols.
        // The selected highlight adds a trailing-padding span: 30 + 3 = 33.
        let width = list_names_content_width(&state, 19);

        assert_eq!(width, 33);
        assert_eq!(max_offset(width, 19), 14);
    }

    #[test]
    fn list_name_render_clamps_scroll_to_rendered_width() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.list_names_scroll_x = u16::MAX;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        clamp_list_scroll_for_area(Rect::new(0, 0, 70, 24), &mut state, &config, tmp.path());

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        assert_eq!(state.list_names_scroll_x, 14);
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_selected_prefix_visible() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1;
        state.list_names_scroll_x = 8;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 2)].symbol(), "▸");
        assert_eq!(buffer[(1, 2)].bg, PHOSPHOR_GREEN);
        assert_eq!(buffer[(2, 2)].bg, PHOSPHOR_GREEN);
        assert_eq!(buffer[(3, 2)].bg, PHOSPHOR_GREEN);
        for x in 1..20 {
            assert_eq!(buffer[(x, 2)].bg, PHOSPHOR_GREEN, "x={x}");
        }
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_hover_background_full_width() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 0;
        state.hovered_list_row = Some(ManagerListRow::SavedWorkspace(0));
        state.list_names_scroll_x = 8;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for x in 1..20 {
            assert_eq!(buffer[(x, 2)].bg, TAB_BG_INACTIVE_HOVER, "x={x}");
        }
    }

    #[test]
    fn hovered_fitting_list_name_does_not_make_sidebar_horizontally_scrollable() {
        let config = config_with_sidebar_names_that_fit_wide_pane();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.hovered_list_row = Some(ManagerListRow::SavedWorkspace(0));

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 120, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for x in 1..35 {
            assert!(
                !["━", "·"].contains(&buffer[(x, 23)].symbol()),
                "unexpected horizontal scrollbar at x={x}"
            );
        }
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_short_selected_background_full_width() {
        let config = config_with_short_selected_and_long_sibling();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 2;
        state.list_names_scroll_x = 12;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 3)].symbol(), "▸");
        for x in 1..20 {
            assert_eq!(buffer[(x, 3)].bg, PHOSPHOR_GREEN, "x={x}");
        }
    }
}

#[cfg(test)]
mod mount_table_tests {
    use super::{
        MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, MountDisplayRow, format_mount_rows,
        mount_path_width, render_mount_lines,
    };
    use crate::workspace::MountConfig;
    use jackin_console::tui::components::mount_rows::render_mount_header;

    /// Collapse a `Line` into a single plain string (concat of all span contents).
    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    /// Return the character index of the start of the `Mode` column (i.e. the
    /// "M" in "Mode" for the header, or the first char of "ro"/"rw" for a data
    /// row). Both are found at: `"  " + path_w + "  "` — so the index equals
    /// `2 + path_w + 2` for a header and for data rows that have no selection
    /// prefix (and the selection prefix is always two chars too — "▸ " or
    /// "  " — so the column boundary is stable).
    fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
        let s = line_text(line);
        // The Mode column is the first two-letter "rw"/"ro" after the gap,
        // or the literal "Mode" for the header. Scan for the first non-space
        // character after the gap-of-two-spaces that follows the path.
        // Simpler: find the offset of the two-space gap before Mode.
        // Header: "  Path<pad>  Mode<pad>Type"
        // Data:   "  path<pad>  rw<pad>type"
        // In both cases the left edge of "Mode"/"rw" is exactly 2 + path_w + 2
        // from the start — we recover it by scanning for the first non-space
        // char at position >= 4 (past the left gutter + at least one path char).
        // Instead, just look for the substring "  M" (Mode header) or "  r"
        // (data row, always "rw"/"ro" starting with r).
        for (i, c) in s.chars().enumerate() {
            if i < 4 {
                continue;
            }
            if c == 'M' || c == 'r' {
                // Make sure this is preceded by the two-space gap — the first
                // such occurrence past the left gutter is the column boundary.
                let prev_two: String = s.chars().skip(i.saturating_sub(2)).take(2).collect();
                if prev_two == "  " {
                    return i;
                }
            }
        }
        panic!("mode column not found in line: {s:?}");
    }

    fn mount_row(
        destination: &str,
        mode: &'static str,
        isolation: &'static str,
        kind: &str,
    ) -> MountDisplayRow {
        MountDisplayRow {
            destination: destination.into(),
            host_source: None,
            mode,
            isolation,
            kind: kind.into(),
        }
    }

    #[test]
    fn header_and_data_rows_share_path_column_width() {
        // Short path + long path forces path_w to be the length of the long one.
        let rows = vec![
            mount_row("~/short", "rw", "shared", "git · main"),
            mount_row(
                "~/Projects/very/deeply/nested/directory",
                "ro",
                "worktree",
                "dir",
            ),
        ];
        let path_w = mount_path_width(&rows);
        assert!(path_w >= "~/Projects/very/deeply/nested/directory".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);

        let header_mode_col = mode_col_start(&header);
        let data0_mode_col = mode_col_start(&data[0]);
        let data1_mode_col = mode_col_start(&data[1]);

        assert_eq!(
            header_mode_col, data0_mode_col,
            "header 'mode' column must align with data row 0"
        );
        assert_eq!(
            header_mode_col, data1_mode_col,
            "header 'mode' column must align with data row 1"
        );
    }

    #[test]
    fn single_row_still_uses_minimum_column_width() {
        // Single short mount — path_w should stay at the floor so the
        // table is still visibly tabular.
        let rows = vec![mount_row(
            "~/Projects/ChainArgos/blockchain-nodes",
            "rw",
            "shared",
            "git · main",
        )];
        let path_w = mount_path_width(&rows);
        assert_eq!(path_w, "~/Projects/ChainArgos/blockchain-nodes".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        assert_eq!(mode_col_start(&header), mode_col_start(&data[0]));
    }

    #[test]
    fn empty_rows_uses_floor_for_header() {
        // Empty case: header should still render with the floor width and
        // include the two-space gap between every column.
        let path_w = mount_path_width(&[]);
        assert_eq!(path_w, "Destination".len());
        let header = render_mount_header(path_w);
        // "  <path padded>  <mode padded>  <iso padded>  Type"
        let expected = format!(
            "  {path:<path_w$}  {mode:<mw$}  {iso:<iw$}  Type",
            path = "Destination",
            mode = "Mode",
            iso = "Isolation",
            path_w = path_w,
            mw = MOUNT_MODE_COL_WIDTH,
            iw = MOUNT_ISOLATION_COL_WIDTH,
        );
        let s = line_text(&header);
        assert_eq!(s, expected);
    }

    #[test]
    fn header_has_two_space_gap_between_columns() {
        // Regression for the "Mode Type" spacing bug, extended to cover the
        // new `Isolation` column: header must emit a literal two-space gap
        // between every column (Mode → Isolation → Type), mirroring the gap
        // data rows emit between `rw`/`ro`, the isolation label, and the
        // kind. Additionally pins the type-column alignment: the `Type`
        // header label must start at the same character offset as the data
        // row's kind label.
        let rows = vec![mount_row("~/p", "rw", "shared", "folder")];
        let path_w = mount_path_width(&rows);
        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        let header_text = line_text(&header);
        let data_text = line_text(&data[0]);
        // Header should have "Mode" followed by gap+padding to the isolation column.
        assert!(
            header_text.contains("Isolation"),
            "expected header to contain 'Isolation'; got {header_text:?}"
        );
        let header_type_offset = header_text.find("Type").expect("header has 'Type'");
        let data_kind_offset = data_text.find("folder").expect("data row has 'folder'");
        assert_eq!(
            header_type_offset, data_kind_offset,
            "Type column misaligned: header at {header_type_offset}, data at {data_kind_offset}"
        );
    }

    /// Worktree mounts must surface an `Iso = worktree` badge in the data
    /// row. Per the per-mount-isolation spec the badge renders the canonical
    /// spelling for every mount (no blank for `shared`).
    #[test]
    fn mount_row_renders_isolation_badge_for_worktree() {
        let m = MountConfig {
            src: "/tmp/x".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Worktree,
        };
        let rows = format_mount_rows(std::slice::from_ref(&m));
        assert_eq!(rows.len(), 1);
        let path_w = mount_path_width(&rows);
        let lines = render_mount_lines(&rows, path_w);
        let text = line_text(&lines[0]);
        assert!(
            text.contains("worktree"),
            "missing worktree badge: {text:?}"
        );
    }

    /// Shared mounts must also surface a literal `shared` badge — the spec's
    /// canonical-spelling rule means `shared` is rendered explicitly rather
    /// than blank, so operators always see which strategy applies.
    #[test]
    fn mount_row_renders_isolation_badge_for_shared() {
        let m = MountConfig {
            src: "/tmp/x".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let rows = format_mount_rows(std::slice::from_ref(&m));
        assert_eq!(rows.len(), 1);
        let path_w = mount_path_width(&rows);
        let lines = render_mount_lines(&rows, path_w);
        let text = line_text(&lines[0]);
        assert!(text.contains("shared"), "missing shared badge: {text:?}");
    }
}

#[cfg(test)]
mod mount_block_height_tests {
    //! Pins the Mounts sub-panel height formula shared by
    //! `render_details_pane` and `render_current_dir_details_pane`. Guards
    //! against the "phantom empty row" regression where a fixed
    //! `Constraint::Length(5)` over-allocated by 1 for a single-mount
    //! current-directory workspace.
    use super::{global_mounts_content_height, mount_block_height};
    use crate::workspace::MountConfig;

    fn mount(path: &str) -> MountConfig {
        MountConfig {
            src: path.into(),
            dst: path.into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }

    #[test]
    fn empty_mounts_reserves_row_for_none_placeholder() {
        // 0 data rows + "(none)" placeholder (1 row) + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[]), 4);
    }

    #[test]
    fn single_mount_fits_in_four_rows() {
        // Regression: the current-dir pane used to hard-code `Length(5)`
        // which left an extra empty line inside the block. Correct total
        // for a 1-mount workspace is 1 data + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[mount("/tmp/a")]), 4);
    }

    #[test]
    fn multiple_mounts_scale_linearly() {
        assert_eq!(mount_block_height(&[mount("/tmp/a"), mount("/tmp/b")]), 5);
        assert_eq!(
            mount_block_height(&[mount("/a"), mount("/b"), mount("/c")]),
            6
        );
    }

    #[test]
    fn many_mounts_clamp_to_twelve() {
        let mounts: Vec<MountConfig> = (0..20).map(|i| mount(&format!("/m/{i}"))).collect();
        assert_eq!(mount_block_height(&mounts), 12);
    }

    #[test]
    fn global_mount_heights_match_rendered_line_count() {
        let same_path = mount("/cache/shared");
        let split_path = MountConfig {
            src: "/host/cache".into(),
            dst: "/container/cache".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };

        assert_eq!(global_mounts_content_height(&[same_path]), 2);
        assert_eq!(global_mounts_content_height(&[split_path]), 3);
        assert_eq!((global_mounts_content_height(&[]) + 2).min(12), 3);
    }
}

#[cfg(test)]
mod subpanel_padding_tests {
    //! Visual regression tests pinning the leading-padding convention shared
    //! by the General / Mounts / Roles sub-panels. All three render content
    //! rows starting at the same column so the first visible character of
    //! the three blocks, giving the right pane a tidy left edge.
    use super::{
        SUBPANEL_CONTENT_INDENT, render_environments_subpanel, render_general_subpanel,
        render_mounts_subpanel,
    };
    use crate::config::AppConfig;
    use crate::console::tui::components::workspace_list::{
        render_agents_subpanel, workspace_env_rows,
    };
    use crate::console::tui::state::{MountInfoCache, WorkspaceSummary};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    /// Scan the first content row inside a sub-panel block (y = 1, skipping
    /// the top border at y = 0) for the first cell holding a printable
    /// non-space character, skipping the left vertical border. Returns the
    /// offset of that character *from the left border* — i.e. the indent —
    /// so values can be compared against `SUBPANEL_CONTENT_INDENT` directly.
    fn first_content_indent(terminal: &Terminal<TestBackend>) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        // Locate the left border column first so the returned value is the
        // relative indent, not the absolute column.
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, 1)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            let sym = buf[(x, 1)].symbol();
            if sym.is_empty() || sym == " " {
                continue;
            }
            return Some((x - border_x - 1) as usize);
        }
        None
    }

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        joined
    }

    fn summary() -> WorkspaceSummary {
        WorkspaceSummary {
            name: "demo".into(),
            workdir: "/tmp/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_role_count: 0,
            default_role: None,
            last_role: None,
        }
    }

    fn ws_config_with_allowed(names: &[&str], default: Option<&str>) -> WorkspaceConfig {
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/tmp/demo".into(),
            mounts: vec![],
            allowed_roles: names.iter().map(|s| (*s).into()).collect(),
            default_role: default.map(String::from),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        }
    }

    /// The first visible character of row 0 inside each sub-panel block
    /// must sit at the shared `SUBPANEL_CONTENT_INDENT`. Without the General
    /// block's two-space prefix the `w` of `workdir` rendered at column 1
    /// (flush with the border) while Mounts/Roles rendered at column 2.
    #[test]
    fn subpanel_content_column_alignment() {
        // General
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 40, 4), &summary().workdir);
        })
        .unwrap();
        let general_col = first_content_indent(&term).expect("general has content");

        // Mounts
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let cache = MountInfoCache::default();
            render_mounts_subpanel(f, Rect::new(0, 0, 40, 4), &[], &cache, 0, 0, false);
        })
        .unwrap();
        let mounts_col = first_content_indent(&term).expect("mounts has content");

        // Roles, "any role" branch (no allowed list)
        let cfg = AppConfig::default();
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 4), None, &cfg);
        })
        .unwrap();
        let agents_any_col = first_content_indent(&term).expect("roles 'any' has content");

        assert_eq!(
            general_col, SUBPANEL_CONTENT_INDENT,
            "General first char at col {general_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            mounts_col, SUBPANEL_CONTENT_INDENT,
            "Mounts first char at col {mounts_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            agents_any_col, SUBPANEL_CONTENT_INDENT,
            "Roles (any) first char at col {agents_any_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
    }

    /// Scan row `y` inside a sub-panel block for the first cell whose
    /// symbol equals `needle`, returning the offset from the left border.
    /// Used to locate the trailing star glyph on a default-role row.
    fn find_symbol_indent(terminal: &Terminal<TestBackend>, y: u16, needle: &str) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            if buf[(x, y)].symbol() == needle {
                return Some((x - border_x - 1) as usize);
            }
        }
        None
    }

    /// Scan row `y` for the last printable non-space/border cell and
    /// return its relative offset from the left border. Used to confirm
    /// a non-default row has no trailing suffix past the name.
    fn last_printable_indent(terminal: &Terminal<TestBackend>, y: u16) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let right_border_x = ((border_x + 1)..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let mut last: Option<usize> = None;
        for x in (border_x + 1)..right_border_x {
            let sym = buf[(x, y)].symbol();
            if !sym.is_empty() && sym != " " {
                last = Some((x - border_x - 1) as usize);
            }
        }
        last
    }

    /// Non-default role rows render the name starting at
    /// `SUBPANEL_CONTENT_INDENT` (col 2 from the border). With the
    /// trailing-star convention no glyph precedes the name.
    ///
    /// With the lean Roles block (env detail moved to the
    /// Environments block), the sub-panel lays out for two allowed
    /// roles (alpha default, beta non-default):
    ///   y=0 top border
    ///   y=1 `  Default <name>`
    ///   y=2 blank spacer
    ///   y=3 alpha row (default)
    ///   y=4 beta row (non-default)
    #[test]
    fn agents_subpanel_non_default_agent_name_starts_at_col_2() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 7);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 7), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the beta row (y=4).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on beta row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("beta row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "non-default role name should start at col {SUBPANEL_CONTENT_INDENT}, got {name_col}"
        );

        // And there must be no trailing star on the non-default row.
        let last_col = last_printable_indent(&term, 4).expect("beta row has content");
        // `beta` is 4 chars starting at col 2 ⇒ last printable at col 5.
        // A trailing star would push last_col to col 7 (space + star).
        assert_eq!(
            last_col,
            SUBPANEL_CONTENT_INDENT + "beta".len() - 1,
            "non-default role row must have no trailing suffix past the name",
        );
    }

    /// Default role row carries a trailing star glyph positioned after
    /// the role name (separated by a space), not a leading star.
    ///
    /// Roles sub-panel layout: top border at y=0, `Default <name>` at
    /// y=1, blank at y=2, first role row at y=3. For a single-allowed
    /// workspace that role IS the default.
    #[test]
    fn agents_subpanel_default_agent_has_trailing_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        let star_col = find_symbol_indent(&term, 3, "\u{2605}")
            .expect("default role row should contain a star glyph");
        let expected = SUBPANEL_CONTENT_INDENT + "alpha".len() + 1;
        assert_eq!(
            star_col, expected,
            "default role star should trail the name at col {expected}, got {star_col}"
        );
    }

    /// Default role row's name column matches non-default rows (and the
    /// `SUBPANEL_CONTENT_INDENT` convention). The trailing star must not
    /// shift the name right.
    ///
    /// y=1 is the `Default <role>` row, whose label also starts at
    /// `SUBPANEL_CONTENT_INDENT`. The invariant the test pins (every
    /// content row starts at col 2) still holds — what we're confirming
    /// is that the block's leading indent is consistent. We check the
    /// role row explicitly to guard against the trailing-star breaking
    /// the name-column alignment.
    #[test]
    fn agents_subpanel_default_agent_name_starts_at_col_2_regardless_of_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the alpha row (y=3).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on alpha row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("alpha row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "default role name should start at col {SUBPANEL_CONTENT_INDENT} even with the trailing star, got {name_col}"
        );
    }

    // ── General sub-panel: Last-used row was already removed ──────────

    /// The General sub-panel no longer shows `Last used` — it only renders
    /// `Working dir`. Guards against a regression that reintroduces the row
    /// and grows the block back to 4 rows.
    #[test]
    fn general_subpanel_no_longer_shows_last_used() {
        let mut s = summary();
        s.last_role = Some("alpha".into());

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 60, 4), &s.workdir);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "General sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    // ── Roles sub-panel: Default row + per-role overrides ───────────

    /// Render the Roles sub-panel into a `TestBackend` of the given size
    /// and return one row of the buffer at `y` as a plain string. Used
    /// throughout this section to scrape per-row text after layout shifts.
    fn render_agents_row(
        ws: Option<&crate::workspace::WorkspaceConfig>,
        cfg: &AppConfig,
        width: u16,
        height: u16,
        y: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, width, height), ws, cfg);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, y)].symbol());
        }
        row
    }

    /// The Roles sub-panel renders `Default <role>` at the top, above
    /// the blank spacer and the per-role rows.
    #[test]
    fn agents_subpanel_shows_default_at_top() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default"),
            "Roles row 1 must hold `Default`; got {row:?}"
        );
        assert!(
            row.contains("alpha"),
            "Roles row 1 must hold the default role name; got {row:?}"
        );
    }

    /// When `default_role` is `None`, the Default row shows `(none)`.
    #[test]
    fn agents_subpanel_default_none_renders_placeholder() {
        let ws = ws_config_with_allowed(&[], None);
        let cfg = AppConfig::default();

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default") && row.contains("(none)"),
            "Default row should show `(none)` when no default role is set; got {row:?}"
        );
    }

    /// `Last used` must no longer appear anywhere in the Roles
    /// sub-panel — it was demoted as part of the preview cleanup that
    /// nested per-role overrides under each role name.
    #[test]
    fn agents_subpanel_no_longer_shows_last_used() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.last_role = Some("beta".into());
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "Roles sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    /// The Roles block is now a lean default + name list; per-role
    /// env overrides moved to the consolidated Environments block.
    /// This test pins that the Roles sub-panel does NOT mention any
    /// override key names — the keys belong only in the Environments
    /// block now.
    #[test]
    fn preview_agents_block_no_longer_lists_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut overrides = crate::workspace::WorkspaceRoleOverride::default();
        overrides.env.insert("API_KEY".into(), "literal".into());
        overrides
            .env
            .insert("LOG_LEVEL".into(), "op://Vault/Item/field".into());
        ws.roles.insert("alpha".into(), overrides);

        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        // Per-role override keys must NOT appear in the Roles block —
        // they live in the Environments block now.
        assert!(
            !joined.contains("API_KEY"),
            "override key API_KEY must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("LOG_LEVEL"),
            "override key LOG_LEVEL must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("[op]"),
            "`[op]` marker must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("(no overrides)"),
            "`(no overrides)` placeholder must NOT appear in the Roles block; got {joined}"
        );
        // Default + role name still render.
        assert!(
            joined.contains("Default") && joined.contains("alpha"),
            "Roles block must still show default + role name; got {joined}"
        );
    }

    /// When `allowed_roles` is empty (the "all roles allowed"
    /// shorthand), the preview lists every globally-configured role —
    /// matching what the editor's Roles tab shows. No `any role`
    /// placeholder.
    #[test]
    fn preview_agents_block_lists_all_global_agents_when_allowed_empty() {
        let ws = ws_config_with_allowed(&[], None);
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 12), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("alpha"),
            "alpha should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            joined.contains("beta"),
            "beta should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            !joined.contains("any role"),
            "old `any role` placeholder should be gone; got {joined}"
        );
    }

    // ── Environments sub-panel ─────────────────────────────────────────

    /// Render the Environments sub-panel into a fresh `TestBackend` of
    /// the given size and return the joined-with-newlines screen text.
    fn render_env_to_string(
        ws: &crate::workspace::WorkspaceConfig,
        width: u16,
        height: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(
                f,
                Rect::new(0, 0, width, height),
                workspace_env_rows(Some(ws)),
            );
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        joined
    }

    /// The Environments preview block lists workspace-level env keys in
    /// alphabetical order. Key names only — plain or op:// values never
    /// render. Op:// values get a `[op]` marker matching the editor
    /// convention.
    #[test]
    fn preview_includes_environments_block_with_workspace_env_keys() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env.insert("DB_URL".into(), "postgres://...".into());
        ws.env.insert("API_KEY".into(), "literal-secret".into());

        let joined = render_env_to_string(&ws, 60, 6);
        assert!(
            joined.contains("Environments"),
            "block title `Environments` must appear; got {joined}"
        );
        assert!(
            joined.contains("API_KEY"),
            "API_KEY env key must appear; got {joined}"
        );
        assert!(
            joined.contains("DB_URL"),
            "DB_URL env key must appear; got {joined}"
        );
        // Sub-section header from the previous layout must NOT appear in
        // the flat list.
        assert!(
            !joined.contains("All roles:"),
            "flat layout must not render the `All roles:` sub-header; got {joined}"
        );
        // Values must never appear in the preview.
        assert!(
            !joined.contains("postgres://"),
            "plain env values must not render; got {joined}"
        );
        assert!(
            !joined.contains("literal-secret"),
            "plain env values must not render; got {joined}"
        );
    }

    /// The Environments preview is one flat list sorted alphabetically
    /// by env name. Workspace-level rows have an empty right column;
    /// per-role override rows show the role name on the right.
    #[test]
    fn preview_environments_block_lists_envs_alphabetically_with_agent_on_right() {
        let mut ws = ws_config_with_allowed(&["beta", "alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        ws.env.insert("DB_URL".into(), "postgres://...".into());

        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let mut beta_overrides = crate::workspace::WorkspaceRoleOverride::default();
        beta_overrides.env.insert("DEBUG".into(), "1".into());
        ws.roles.insert("beta".into(), beta_overrides);

        let joined = render_env_to_string(&ws, 60, 14);
        // No sub-headers in the flat layout.
        assert!(
            !joined.contains("All roles:"),
            "flat layout must not render `All roles:`; got {joined}"
        );
        assert!(
            !joined.contains("alpha:"),
            "flat layout must not render `<role>:` sub-headers; got {joined}"
        );
        assert!(
            !joined.contains("beta:"),
            "flat layout must not render `<role>:` sub-headers; got {joined}"
        );

        // Find each name's y-row to pin alphabetical ordering across scopes.
        let mut api_y: Option<u16> = None;
        let mut db_y: Option<u16> = None;
        let mut debug_y: Option<u16> = None;
        let mut log_y: Option<u16> = None;
        for (y, row) in joined.lines().enumerate() {
            if api_y.is_none() && row.contains("API_KEY") {
                api_y = Some(y as u16);
            }
            if db_y.is_none() && row.contains("DB_URL") {
                db_y = Some(y as u16);
            }
            if debug_y.is_none() && row.contains("DEBUG") {
                debug_y = Some(y as u16);
            }
            if log_y.is_none() && row.contains("LOG_LEVEL") {
                log_y = Some(y as u16);
            }
        }
        let api = api_y.expect("API_KEY row must appear");
        let db = db_y.expect("DB_URL row must appear");
        let debug = debug_y.expect("DEBUG row must appear");
        let log = log_y.expect("LOG_LEVEL row must appear");
        assert!(
            api < db && db < debug && debug < log,
            "rows must be alphabetical: API_KEY < DB_URL < DEBUG < LOG_LEVEL; \
             got y=({api},{db},{debug},{log})"
        );

        // Role labels live on the right edge of their row.
        for row in joined.lines() {
            if row.contains("DEBUG") {
                assert!(
                    row.contains("beta"),
                    "DEBUG row must show `beta` on the right; got {row}"
                );
            }
            if row.contains("LOG_LEVEL") {
                assert!(
                    row.contains("alpha"),
                    "LOG_LEVEL row must show `alpha` on the right; got {row}"
                );
            }
        }
    }

    /// Roles listed in `allowed_roles` but with no env overrides do
    /// NOT contribute rows to the Environments block — their absence is
    /// the signal that they have no overrides. The Roles block still
    /// lists them.
    #[test]
    fn preview_environments_block_omits_agents_without_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        // Only alpha has overrides; beta is in the allowed list but
        // has no overrides.
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 10);
        assert!(
            joined.contains("alpha"),
            "alpha has overrides — its name must appear on its row; got {joined}"
        );
        assert!(
            !joined.contains("beta"),
            "beta has no overrides — its name must NOT appear in the Environments block; got {joined}"
        );
    }

    /// A workspace-level env key renders a row with the key name and an
    /// empty right column (no role label).
    #[test]
    fn preview_environments_flat_row_workspace_level_has_no_agent_label() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());

        let joined = render_env_to_string(&ws, 60, 4);
        // The row containing API_KEY must not also contain "alpha".
        let api_row = joined
            .lines()
            .find(|r| r.contains("API_KEY"))
            .expect("API_KEY row must appear");
        assert!(
            !api_row.contains("alpha"),
            "workspace-level row must not show an role label; got `{api_row}`"
        );
    }

    /// A per-role override env key renders a row with the key on the
    /// left and the role name on the right.
    #[test]
    fn preview_environments_flat_row_per_agent_has_agent_label_on_right() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 4);
        let log_row = joined
            .lines()
            .find(|r| r.contains("LOG_LEVEL"))
            .expect("LOG_LEVEL row must appear");
        assert!(
            log_row.contains("alpha"),
            "per-role row must show the role name; got `{log_row}`"
        );
        // Role name sits to the right of the key name on the same row.
        let key_pos = log_row.find("LOG_LEVEL").unwrap();
        let agent_pos = log_row.find("alpha").unwrap();
        assert!(
            agent_pos > key_pos,
            "role label must come AFTER the key name on the row; got key@{key_pos}, role@{agent_pos}"
        );
    }

    /// Per-role rows show the role label one cell before the right
    /// border, not flush against it. The cell at `inner_width - 1`
    /// (i.e. the column just inside the right border) must be a space.
    #[test]
    fn preview_environments_agent_label_has_one_cell_right_padding() {
        let mut ws = ws_config_with_allowed(&["agent-brown"], Some("agent-brown"));
        let mut brown = crate::workspace::WorkspaceRoleOverride::default();
        brown.env.insert("TEST5".into(), "v".into());
        ws.roles.insert("agent-brown".into(), brown);

        let width: u16 = 60;
        let backend = TestBackend::new(width, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(
                f,
                Rect::new(0, 0, width, 4),
                workspace_env_rows(Some(&ws)),
            );
        })
        .unwrap();
        let buf = term.backend().buffer();

        // Find the row containing TEST5; role label `agent-brown`
        // must end one cell before the right border so the cell at
        // x = width - 2 (i.e. the one just inside the right border
        // at x = width - 1) is a space, and the label's last char
        // sits at x = width - 3.
        let mut found_row: Option<u16> = None;
        for y in 0..buf.area.height {
            let row: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
            if row.contains("TEST5") {
                found_row = Some(y);
                break;
            }
        }
        let y = found_row.expect("TEST5 row must render");

        // Right border is at x = width - 1 (the `│` glyph).
        // The cell immediately inside (x = width - 2) must be blank
        // — that's the 1-cell padding the operator asked for.
        let cell_inside_border = buf[(width - 2, y)].symbol();
        assert_eq!(
            cell_inside_border,
            " ",
            "cell at x={} (one inside right border) must be a space — \
             role label should have 1-cell right padding; got {:?}",
            width - 2,
            cell_inside_border
        );

        // And the role label's last char (`n` of `agent-brown`)
        // must sit at x = width - 3 — the cell just before the pad.
        let label_last = buf[(width - 3, y)].symbol();
        assert_eq!(
            label_last,
            "n",
            "last char of `agent-brown` must sit at x={} (one cell \
             before the right border); got {:?}",
            width - 3,
            label_last
        );
    }

    /// The same env name at workspace and role scope renders TWO
    /// distinct rows: workspace first (empty right column), role
    /// second (with role label).
    #[test]
    fn preview_environments_same_key_in_workspace_and_agent_renders_two_rows() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "workspace-value".into());
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("API_KEY".into(), "role-value".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 6);
        let api_rows: Vec<&str> = joined.lines().filter(|r| r.contains("API_KEY")).collect();
        assert_eq!(
            api_rows.len(),
            2,
            "API_KEY must appear in TWO rows (workspace + alpha); got rows={api_rows:?}"
        );
        // Workspace row first (no role label), role row second.
        assert!(
            !api_rows[0].contains("alpha"),
            "first API_KEY row must be workspace-level (no role label); got `{}`",
            api_rows[0]
        );
        assert!(
            api_rows[1].contains("alpha"),
            "second API_KEY row must be the role override (alpha label); got `{}`",
            api_rows[1]
        );
    }

    /// Rows sort alphabetically by name regardless of scope. Workspace
    /// keys and per-role keys interleave when their names interleave.
    #[test]
    fn preview_environments_sorts_alphabetically_across_scopes() {
        let mut ws = ws_config_with_allowed(&["agent-smith", "agent-brown"], Some("agent-smith"));
        ws.env.insert("DB_URL".into(), "postgres://...".into());
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut smith = crate::workspace::WorkspaceRoleOverride::default();
        smith.env.insert("DEBUG".into(), "1".into());
        ws.roles.insert("agent-smith".into(), smith);

        let mut brown = crate::workspace::WorkspaceRoleOverride::default();
        brown.env.insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("agent-brown".into(), brown);

        let joined = render_env_to_string(&ws, 60, 8);
        // Capture the y-row of each env-key name and assert ordering.
        let mut order: Vec<(&str, usize)> = Vec::new();
        for (y, row) in joined.lines().enumerate() {
            for key in ["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"] {
                if row.contains(key) && !order.iter().any(|(k, _)| *k == key) {
                    order.push((key, y));
                }
            }
        }
        let names: Vec<&str> = order.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            vec!["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"],
            "rows must be sorted alphabetically across workspace and role scopes; got {order:?}"
        );
    }

    /// Op:// references in the workspace env get a leading `[op]` marker.
    /// The bare reference itself (e.g. "<op://Vault/Item/field>") must
    /// never appear — only the marker tag.
    #[test]
    fn preview_environments_marks_op_references_with_op_marker() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env.insert(
            "STRIPE_KEY".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc-vault/abc-item/field".into(),
                path: "Vault/Item/field".into(),
                account: None,
            }),
        );

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(
                f,
                Rect::new(0, 0, 60, 4),
                workspace_env_rows(Some(&ws)),
            );
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("[op]"),
            "op:// reference must be tagged with `[op]` marker; got {joined}"
        );
        assert!(
            joined.contains("STRIPE_KEY"),
            "key name must still appear next to `[op]`; got {joined}"
        );
        assert!(
            !joined.contains("op://"),
            "raw op:// reference must never render in the preview; got {joined}"
        );
    }

    /// When the workspace has zero env entries at every scope
    /// (workspace-level AND per-role overrides), the right-pane
    /// Environments preview block is omitted entirely — no header, no
    /// body, no border. The Roles block fills the freed space.
    #[test]
    fn preview_omits_environments_block_when_workspace_has_no_env_vars() {
        // Empty workspace env, no role overrides.
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            !joined.contains("Environments"),
            "Environments block must NOT render when the workspace has no env vars; got {joined}"
        );
        assert!(
            !joined.contains("(no environment variables)"),
            "the placeholder line must NOT appear (block is omitted entirely); got {joined}"
        );
    }

    #[test]
    fn preview_shows_unscoped_global_mounts_without_role_ambiguity_text() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], None);
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());
        cfg.add_mount(
            "cargo",
            crate::workspace::MountConfig {
                src: "/tmp/cargo".into(),
                dst: "/home/agent/.cargo".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            None,
        );
        cfg.add_mount(
            "beta-only",
            crate::workspace::MountConfig {
                src: "/tmp/beta".into(),
                dst: "/beta".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            Some("beta"),
        );

        let backend = TestBackend::new(72, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 72, 24), 0, &summary(), &cfg, &state);
        })
        .unwrap();

        let joined = buffer_text(term.backend().buffer());
        assert!(joined.contains("Global mounts"), "{joined}");
        assert!(joined.contains(".cargo"), "{joined}");
        assert!(!joined.contains("selected role affects"), "{joined}");
        assert!(!joined.contains("/beta"), "{joined}");
        assert!(joined.contains("+1 role mounts"), "{joined}");
    }

    /// The Environments block appears as soon as ANY env entry exists
    /// at the workspace level, even if no per-role override is set.
    #[test]
    fn preview_includes_environments_block_when_only_workspace_env_set() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Environments"),
            "Environments block header must appear when the workspace env is non-empty; got {joined}"
        );
        assert!(
            joined.contains("API_KEY"),
            "the workspace env key must render; got {joined}"
        );
    }

    #[test]
    fn preview_shows_compact_running_badge_for_active_instances() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let mut state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        state.instances = vec![
            crate::instance::InstanceIndexEntry {
                instance_id: "k7p9m2xq".into(),
                container_base: "jackin-demo-alpha-k7p9m2xq".into(),
                workspace_name: Some("demo".into()),
                workspace_label: "demo".into(),
                workdir: "/workspace/demo".into(),
                role_key: "alpha".into(),
                agent_runtime: "claude".into(),
                status: crate::instance::InstanceStatus::Active,
                updated_at: "2026-05-11T00:00:00Z".into(),
            },
            crate::instance::InstanceIndexEntry {
                instance_id: "done0001".into(),
                container_base: "jackin-demo-alpha-done0001".into(),
                workspace_name: Some("demo".into()),
                workspace_label: "demo".into(),
                workdir: "/workspace/demo".into(),
                role_key: "alpha".into(),
                agent_runtime: "claude".into(),
                status: crate::instance::InstanceStatus::CleanExited,
                updated_at: "2026-05-11T00:00:00Z".into(),
            },
        ];

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(72, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 72, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let joined = buffer_text(term.backend().buffer());
        // Compact badge shows the "Running" block title and instance count.
        assert!(joined.contains("Running"), "{joined}");
        assert!(joined.contains("1 instance running"), "{joined}");
        // CleanExited instances are not shown in the compact summary.
        assert!(
            !joined.contains("done0001"),
            "cleanly exited instances must not appear: {joined}"
        );
    }

    /// The Environments block appears when at least one per-role
    /// override is set, even if the workspace-level env map is empty.
    #[test]
    fn preview_includes_environments_block_when_only_per_agent_overrides_set() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Environments"),
            "Environments block header must appear when only per-role overrides exist; got {joined}"
        );
        assert!(
            joined.contains("LOG_LEVEL"),
            "the per-role override key must render; got {joined}"
        );
    }

    /// The right-pane preview blocks render in the order
    /// General → Mounts → Environments → Roles. Pinned by scraping the
    /// block-title labels off a full-pane render and confirming their
    /// y-order.
    #[test]
    fn preview_block_order_is_general_mounts_environments_agents() {
        // Build a workspace with a mount, an env var, and an role so
        // every block has visible content.
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.workdir = "/workspace/demo".into();
        ws.mounts.push(crate::workspace::MountConfig {
            src: "/tmp/demo".into(),
            dst: "/workspace/demo".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        });
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::tui::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        // For each block, find the y-row that holds its title (titles
        // are unique strings so we can scrape by row content).
        let mut general_y: Option<u16> = None;
        let mut mounts_y: Option<u16> = None;
        let mut envs_y: Option<u16> = None;
        let mut agents_y: Option<u16> = None;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if general_y.is_none() && row.contains(" General ") {
                general_y = Some(y);
            }
            if mounts_y.is_none() && row.contains(" Mounts ") {
                mounts_y = Some(y);
            }
            if envs_y.is_none() && row.contains(" Environments ") {
                envs_y = Some(y);
            }
            if agents_y.is_none() && row.contains(" Roles ") {
                agents_y = Some(y);
            }
        }

        let g = general_y.expect("General block title must appear");
        let m = mounts_y.expect("Mounts block title must appear");
        let e = envs_y.expect("Environments block title must appear");
        let a = agents_y.expect("Roles block title must appear");
        assert!(
            g < m && m < e && e < a,
            "block order must be General < Mounts < Environments < Roles; got y=({g},{m},{e},{a})"
        );
    }
}
