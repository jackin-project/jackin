//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::state::{EditorState, EditorTab, SecretsScopeTag};
use jackin_console::tui::mount_display::workspace_config_mounts_content_width_with_cache;
use jackin_console::tui::screens::editor::view::{
    auth_display_row as editor_auth_display_row, editor_auth_line_width, editor_body_area,
    editor_role_load_row_width, editor_role_row_width, editor_roles_status_width,
    editor_secret_line_width, general_state_geometry, mount_state_geometry,
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
            mounts_content_width: workspace_config_mounts_content_width_with_cache(
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
    let geometry = general_state_geometry(state);
    EditorTabGeometry {
        content_width: geometry.content_width,
        content_height: geometry.content_height,
    }
}

fn mounts_tab_geometry(state: &EditorState<'_>) -> EditorTabGeometry {
    let geometry = mount_state_geometry(state);
    EditorTabGeometry {
        content_width: geometry.content_width,
        content_height: geometry.content_height,
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
        content_width: status_width
            .max(role_width)
            .max(editor_role_load_row_width()),
        content_height: 2 + config.roles.len() + usize::from(!config.roles.is_empty()) + 1,
    }
}

fn secrets_tab_geometry(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> EditorTabGeometry {
    let rows = state.secrets_flat_rows();
    let content_width = rows
        .iter()
        .map(|row| {
            editor_secret_line_width(
                row,
                area.width,
                |scope, key| secret_value_display(state, scope, key),
                |scope, key| {
                    state
                        .unmasked_rows
                        .contains(&(scope.clone(), key.to_owned()))
                },
                |role| config.roles.contains_key(role),
                |role| {
                    state
                        .pending
                        .roles
                        .get(role)
                        .map_or(0, |role| role.env.len())
                },
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
    let rows = state.auth_flat_rows(config);
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let content_width = rows
        .iter()
        .map(|row| {
            let display_row = editor_auth_display_row(row, &synthesized, &workspace_name);
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
    Some(jackin_console::tui::components::env_value::secret_display(
        value,
    ))
}
