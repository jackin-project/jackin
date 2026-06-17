//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::state::{EditorState, EditorTab};
use jackin_console::tui::mount_display::workspace_config_mounts_content_width_with_cache;
use jackin_console::tui::screens::editor::view::{
    auth_state_geometry, editor_body_area, general_state_geometry, mount_state_geometry,
    role_state_geometry, secret_state_geometry,
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
    let geometry = role_state_geometry(state, config.roles.keys());
    EditorTabGeometry {
        content_width: geometry.content_width,
        content_height: geometry.content_height,
    }
}

fn secrets_tab_geometry(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> EditorTabGeometry {
    let geometry = secret_state_geometry(state, area.width, |role| config.roles.contains_key(role));
    EditorTabGeometry {
        content_width: geometry.content_width,
        content_height: geometry.content_height,
    }
}

fn auth_tab_geometry(state: &EditorState<'_>, config: &AppConfig) -> EditorTabGeometry {
    let geometry = auth_state_geometry(state, config);
    EditorTabGeometry {
        content_width: geometry.content_width,
        content_height: geometry.content_height,
    }
}
