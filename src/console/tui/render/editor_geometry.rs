//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::AppConfig;
use crate::console::tui::render::mount_display::{
    workspace_mounts_content_height, workspace_mounts_content_width_with_cache,
};
use crate::console::tui::state::auth_flat_rows;
use crate::console::tui::state::{
    AuthRow, EditorMode, EditorState, EditorTab, SecretsRow, SecretsScopeTag,
};
use crate::operator_env::EnvValue;

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
    let keep_awake_display = if state.pending.keep_awake.enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if state.pending.git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    let rows = [
        editor_row_width("Name", name_value),
        editor_row_width("Working dir", &workdir_display),
        editor_row_width("Keep awake", keep_awake_display),
        editor_row_width("Git pull", git_pull_display),
    ];
    EditorTabGeometry {
        content_width: *rows.iter().max().unwrap_or(&0),
        content_height: rows.len(),
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
        .max(text_width("  + Add mount")),
        content_height,
    }
}

fn roles_tab_geometry(state: &EditorState<'_>, config: &AppConfig) -> EditorTabGeometry {
    let is_all = jackin_console::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let total = config.roles.len();
    let status_width = if is_all {
        text_width("  Allowed roles:    all  ")
    } else {
        text_width(&format!(
            "  Allowed roles:    custom     ({allowed_count} of {total} allowed)"
        ))
    };
    let role_width = config
        .roles
        .keys()
        .map(|role_name| text_width(&format!("  [x] * {role_name}")))
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width: status_width
            .max(role_width)
            .max(text_width("  + Load role")),
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
        .map(|row| secrets_row_width(row, area, state, config))
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn auth_tab_geometry(state: &EditorState<'_>, config: &AppConfig) -> EditorTabGeometry {
    let rows = auth_flat_rows(state, config);
    let content_width = rows
        .iter()
        .map(|row| auth_row_width(row, state, config))
        .max()
        .unwrap_or(0);
    EditorTabGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn editor_row_width(label: &str, value: &str) -> usize {
    padded_width(&format!("  {label:15}{value}"))
}

fn secrets_flat_rows(editor: &EditorState<'_>) -> Vec<SecretsRow> {
    jackin_console::tui::screens::editor::update::secrets_flat_rows(
        &editor.pending.env,
        &editor.pending.roles,
        &editor.secrets_expanded,
        |role| &role.env,
    )
}

fn secrets_row_width(
    row: &SecretsRow,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> usize {
    const LABEL_WIDTH: usize = 22;
    match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            let default_value = EnvValue::Plain(String::new());
            let value = state.pending.env.get(key).unwrap_or(&default_value);
            let masked = !state
                .unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, key.clone()));
            secrets_key_width(key, value, masked, area.width, LABEL_WIDTH)
        }
        SecretsRow::WorkspaceAddSentinel => text_width("  + Add environment variable"),
        SecretsRow::RoleHeader { role, .. } => {
            let count = state.pending.roles.get(role).map_or(0, |o| o.env.len());
            let mut width = text_width(&format!("       ▼ Role: {role}  ({count} vars)"));
            if !config.roles.contains_key(role) {
                width += text_width("  (not in registry)");
            }
            padded_width_cols(width, 7)
        }
        SecretsRow::RoleKeyRow { role, key } => {
            let default_value = EnvValue::Plain(String::new());
            let value = state
                .pending
                .roles
                .get(role)
                .and_then(|role| role.env.get(key))
                .unwrap_or(&default_value);
            let masked = !state
                .unmasked_rows
                .contains(&(SecretsScopeTag::Role(role.clone()), key.clone()));
            secrets_key_width(key, value, masked, area.width, LABEL_WIDTH)
        }
        SecretsRow::RoleAddSentinel(role) => {
            padded_width(&format!("       + Add {role} environment variable"))
        }
        SecretsRow::SectionSpacer => 0,
    }
}

fn secrets_key_width(
    key: &str,
    value: &EnvValue,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> usize {
    let prefix_width = 2 + 5 + text_width(&format!("{key:label_width$}")) + 2;
    let value_width = match value {
        EnvValue::OpRef(reference) => op_reference_width(&reference.path)
            .map(|width| text_width("[op] ") + width)
            .unwrap_or_else(|| text_width("     <unparseable path - re-pick>")),
        EnvValue::Plain(_) if masked => text_width("●●●●●●●●●●●"),
        EnvValue::Plain(value) => {
            let budget = (area_width as usize)
                .saturating_sub(label_width)
                .saturating_sub(8)
                .max(1);
            value.chars().count().min(budget)
        }
    };
    padded_width_cols(prefix_width + value_width, 2)
}

fn auth_row_width(row: &AuthRow, state: &EditorState<'_>, config: &AppConfig) -> usize {
    match row {
        AuthRow::AuthKindRow { kind } => padded_width(&format!("  {}", kind.label())),
        AuthRow::WorkspaceMode { .. } => padded_width("  Mode        inherited"),
        AuthRow::WorkspaceSource { kind } => auth_source_width("Source", 0, *kind, state, config),
        AuthRow::RoleHeader { role, .. } => padded_width(&format!("▼ Role: {role}")),
        AuthRow::RoleMode { .. } => padded_width("      Mode        inherited"),
        AuthRow::RoleSource { role, kind } => {
            let _ = role;
            auth_source_width("Source", 6, *kind, state, config)
        }
        AuthRow::AddSentinel { eligible } => {
            let suffix = if *eligible == 0 {
                "   (all roles overridden)"
            } else {
                ""
            };
            padded_width(&format!("  + Override for a role{suffix}"))
        }
        AuthRow::Spacer => 0,
    }
}

fn auth_source_width(
    label: &str,
    indent: usize,
    kind: jackin_console::tui::auth::AuthKind,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> usize {
    let synthesized = crate::console::tui::state::synthesize_appconfig_for_auth(state, config);
    let workspace_name = crate::console::tui::state::workspace_name_for_panel(state);
    let mode =
        crate::console::domain::resolve_panel_mode(&synthesized, kind, &workspace_name, "");
    let label_width = if indent == 0 { 14 } else { 12 };
    let prefix = indent + text_width(&format!("{label:<label_width$}"));
    let value_width = match kind.required_env_var(mode) {
        None => text_width("not required"),
        Some(env_name) => text_width(&format!("unset  ({env_name} for {})", mode.as_str())),
    };
    padded_width_cols(prefix + value_width, indent)
}

fn op_reference_width(path: &str) -> Option<usize> {
    let parts = jackin_console::op_breadcrumb::parse_path_breadcrumb(path)?;
    Some(jackin_console::op_breadcrumb::breadcrumb_display_width(
        &parts,
    ))
}

fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
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
