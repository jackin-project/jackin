//! Root-console editor display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::config::AppConfig;
use crate::console::tui::components::auth_panel::editor_auth_lines_for_state;
use crate::console::tui::state::{EditorMode, EditorState, FieldFocus, SecretsScopeTag};
use jackin_console::tui::components::auth_panel::auth_panel_title;
use jackin_console::tui::components::env_value::secret_display as env_value_secret_display;
use jackin_console::tui::mount_display::format_config_mount_rows_with_cache;
use jackin_console::tui::screens::editor::view::{
    EditorRoleRow, general_lines as editor_general_lines, mount_lines as editor_mount_lines,
    role_lines as editor_role_lines, secret_lines as editor_secret_lines,
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
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();

    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };
    let workdir_display = crate::tui::shorten_home(&state.pending.workdir);

    editor_general_lines(
        cursor,
        show_cursor,
        name_value,
        &workdir_display,
        state.pending.keep_awake.enabled,
        state.pending.git_pull_on_entry,
    )
}

pub(crate) fn editor_mount_lines_for_state(state: &EditorState<'_>) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor = !state.tab_bar_focused()
        && state.workspace_mounts_scroll_focused()
        && state.modal.is_none();
    let rows = format_config_mount_rows_with_cache(&state.pending.mounts, &state.mount_info_cache);
    editor_mount_lines(&rows, cursor, state.hovered_mount_row(), show_cursor)
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
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();

    let rows = state.secrets_flat_rows();
    editor_secret_lines(
        &rows,
        cursor,
        show_cursor,
        area.width,
        |scope, key| match scope {
            SecretsScopeTag::Workspace => state.pending.env.get(key).map(env_value_secret_display),
            SecretsScopeTag::Role(role) => state
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .map(env_value_secret_display),
        },
        |scope, key| {
            state
                .unmasked_rows
                .contains(&(scope.clone(), key.to_owned()))
        },
        |role| config.roles.contains_key(role),
        |role| state.pending.roles.get(role).map_or(0, |o| o.env.len()),
    )
}
