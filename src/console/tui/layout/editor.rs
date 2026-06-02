//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::components::mount_display::{
    workspace_mounts_content_height, workspace_mounts_content_width_with_cache,
};
use crate::console::tui::state::{
    EditorMode, EditorState, EditorTab, SecretsScopeTag, auth_flat_rows, secrets_flat_rows,
};
use jackin_console::tui::screens::editor::view::{
    editor_auth_line_width, editor_body_area, editor_general_content_width,
    editor_mount_add_row_width, editor_role_load_row_width, editor_role_row_width,
    editor_roles_status_width, editor_secret_line_width,
};

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
    let geometry = editor_tab_geometry(body, state, config);
    state.tab_content_width = geometry.content_width;
    state.tab_content_height = geometry.content_height;
    jackin_console::tui::screens::editor::view::clamp_editor_scroll_for_frame(
        body,
        jackin_console::tui::screens::editor::view::EditorScrollGeometry {
            active_mounts: state.active_tab == EditorTab::Mounts,
            content_width: geometry.content_width,
            content_height: geometry.content_height,
            mounts_content_width: workspace_mounts_content_width_with_cache(
                &state.pending.mounts,
                &state.mount_info_cache,
            ),
        },
        &mut state.tab_scroll_x,
        &mut state.tab_scroll_y,
        &mut state.workspace_mounts_scroll_x,
    );
}

struct EditorTabGeometry {
    content_width: usize,
    content_height: usize,
}

fn editor_tab_geometry(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> EditorTabGeometry {
    match state.active_tab {
        EditorTab::General => general_tab_geometry(state),
        EditorTab::Mounts => mounts_tab_geometry(state),
        EditorTab::Roles => roles_tab_geometry(state, config),
        EditorTab::Secrets => secrets_tab_geometry(area, state, config),
        EditorTab::Auth => auth_tab_geometry(state, config),
    }
}

fn general_tab_geometry(state: &EditorState<'_>) -> EditorTabGeometry {
    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };
    let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
    EditorTabGeometry {
        content_width: editor_general_content_width(
            name_value,
            &workdir_display,
            state.pending.keep_awake.enabled,
            state.pending.git_pull_on_entry,
        ),
        content_height: 4,
    }
}

fn mounts_tab_geometry(state: &EditorState<'_>) -> EditorTabGeometry {
    let content_height = if state.pending.mounts.is_empty() {
        2
    } else {
        workspace_mounts_content_height(&state.pending.mounts) + 2
    };
    EditorTabGeometry {
        content_width: workspace_mounts_content_width_with_cache(
            &state.pending.mounts,
            &state.mount_info_cache,
        )
        .max(editor_mount_add_row_width()),
        content_height,
    }
}

fn roles_tab_geometry(state: &EditorState<'_>, config: &AppConfig) -> EditorTabGeometry {
    let is_all = jackin_console::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let total = config.roles.len();
    let status_width = editor_roles_status_width(is_all, allowed_count, total);
    let role_width = config
        .roles
        .keys()
        .map(|role_name| editor_role_row_width(role_name))
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width: status_width.max(role_width).max(editor_role_load_row_width()),
        content_height: 2 + config.roles.len() + usize::from(!config.roles.is_empty()) + 1,
    }
}

fn secrets_tab_geometry(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> EditorTabGeometry {
    let rows = secrets_flat_rows(state);
    let content_width = rows
        .iter()
        .map(|row| {
            editor_secret_line_width(
                row,
                area.width,
                |scope, key| secret_value_display(state, scope, key),
                |scope, key| state.unmasked_rows.contains(&(scope.clone(), key.to_string())),
                |role| config.roles.contains_key(role),
                |role| state.pending.roles.get(role).map_or(0, |role| role.env.len()),
            )
        })
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn auth_tab_geometry(state: &EditorState<'_>, config: &AppConfig) -> EditorTabGeometry {
    let rows = auth_flat_rows(state, config);
    let synthesized = crate::console::tui::state::synthesize_appconfig_for_auth(state, config);
    let workspace_name = crate::console::tui::state::workspace_name_for_panel(state);
    let content_width = rows
        .iter()
        .map(|row| {
            let display_row =
                crate::console::tui::components::auth_panel::editor_auth_display_row(
                    row,
                    &synthesized,
                    &workspace_name,
                );
            editor_auth_line_width(&display_row)
        })
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn secret_value_display<'a>(
    state: &'a EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
) -> Option<jackin_console::tui::components::editor_rows::SecretValueDisplay<'a>> {
    let value = match scope {
        SecretsScopeTag::Workspace => state.pending.env.get(key),
        SecretsScopeTag::Role(role) => state
            .pending
            .roles
            .get(role)
            .and_then(|role| role.env.get(key)),
    }?;
    Some(crate::console::tui::components::env_value::secret_display(value))
}
