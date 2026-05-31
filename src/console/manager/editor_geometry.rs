//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::AppConfig;
use crate::console::manager::mount_display::workspace_mounts_content_width_with_cache;
use crate::console::manager::state::{EditorState, EditorTab};

pub(crate) fn prepare_editor_for_render(
    area: Rect,
    state: &mut EditorState<'_>,
    config: &AppConfig,
) {
    let body = editor_body_area(area, state.cached_footer_h);
    prepare_editor_tab_for_area(body, state, config);
}

pub(crate) fn prepare_editor_tab_for_area(
    body: Rect,
    state: &mut EditorState<'_>,
    config: &AppConfig,
) {
    let lines = crate::console::tui::render::editor::editor_tab_lines(body, state, config);
    state.tab_content_width = jackin_tui::components::scrollable_panel::max_line_width(&lines);
    state.tab_content_height = lines.len();
    let viewport_w = jackin_tui::components::scrollable_panel::viewport_width(body);
    let viewport_h = jackin_tui::components::scrollable_panel::viewport_height(body);
    if state.active_tab == EditorTab::Mounts {
        let content_width = workspace_mounts_content_width_with_cache(
            &state.pending.mounts,
            &state.mount_info_cache,
        );
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            content_width,
            viewport_w,
            &mut state.workspace_mounts_scroll_x,
        );
    } else {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            state.tab_content_width,
            viewport_w,
            &mut state.tab_scroll_x,
        );
    }
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        state.tab_content_height,
        viewport_h,
        &mut state.tab_scroll_y,
    );
}

fn editor_body_area(area: Rect, footer_h: u16) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    chunks[2]
}
