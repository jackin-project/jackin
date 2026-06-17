//! Root-console editor display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::config::AppConfig;
use crate::console::tui::components::auth_panel::editor_auth_lines_for_state;
use crate::console::tui::state::{EditorState, FieldFocus};
use jackin_console::tui::components::auth_panel::auth_panel_title;
use jackin_console::tui::screens::editor::view::{
    EditorRoleRow, general_state_lines as editor_general_state_lines,
    mount_state_lines as editor_mount_state_lines, role_lines as editor_role_lines,
    secret_state_lines as editor_secret_state_lines,
};

pub(crate) fn render_general_tab(frame: &mut Frame<'_>, area: Rect, state: &EditorState<'_>) {
    let rows = editor_general_lines_for_state(state);
    let focused =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        rows,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_mounts_tab(frame: &mut Frame<'_>, area: Rect, state: &EditorState<'_>) {
    let lines = editor_mount_lines_for_state(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.workspace_mounts_scroll_x,
        state.tab_scroll_y,
        state.workspace_mounts_scroll_focused() && state.modal.is_none(),
        None,
    );
}

pub(crate) fn render_roles_tab(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) {
    let lines = editor_role_lines_for_state(state, config);
    let focused =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_secrets_tab(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) {
    let lines = editor_secret_lines_for_state(area, state, config);
    let focused =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_auth_tab(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) {
    let lines = editor_auth_lines_for_state(state, config);
    let title = state
        .auth_selected_kind
        .map(|k| auth_panel_title(k.label()));
    let focused =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        title.as_deref(),
    );
}

pub(crate) fn editor_general_lines_for_state(state: &EditorState<'_>) -> Vec<Line<'static>> {
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    editor_general_state_lines(state, show_cursor)
}

pub(crate) fn editor_mount_lines_for_state(state: &EditorState<'_>) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused()
        && state.workspace_mounts_scroll_focused()
        && state.modal.is_none();
    editor_mount_state_lines(state, show_cursor)
}

pub(crate) fn editor_role_lines_for_state(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();

    let is_all = jackin_console::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let rows: Vec<EditorRoleRow> = config
        .roles
        .keys()
        .map(|role_name| EditorRoleRow {
            name: role_name.clone(),
            effectively_allowed: jackin_console::workspace::agent_is_effectively_allowed(
                &state.pending,
                role_name,
            ),
            is_default: state.pending.default_role.as_deref() == Some(role_name.as_str()),
        })
        .collect();

    editor_role_lines(&rows, allowed_count, is_all, cursor, show_cursor)
}

pub(crate) fn editor_secret_lines_for_state(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<Line<'static>> {
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();

    editor_secret_state_lines(state, show_cursor, area.width, |role| {
        config.roles.contains_key(role)
    })
}
