//! Editor screen view helpers.

use super::model::{EditorMode, EditorTab, SecretsScopeTag};
use super::update::forbidden_secret_keys;
use crate::tui::components::editor_rows::{
    AuthSourceDisplay, SecretValueDisplay, action_row_style, disclosure_style,
    render_secret_key_line,
};
use crate::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, render_mount_header,
};
use crate::tui::mount_display::{MountDisplayRow, mount_path_width};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorRoleRow {
    pub name: String,
    pub effectively_allowed: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    AddSentinel { eligible: usize },
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorScrollGeometry {
    pub active_mounts: bool,
    pub content_width: usize,
    pub content_height: usize,
    pub mounts_content_width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

pub fn editor_frame_areas(area: Rect, footer_h: u16) -> EditorFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    EditorFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

#[must_use]
pub fn editor_header_title(mode: &EditorMode) -> String {
    match mode {
        EditorMode::Edit { name } => format!("edit workspace · {name}"),
        EditorMode::Create => "create workspace".to_owned(),
    }
}

#[must_use]
pub fn editor_name_value(
    mode: &EditorMode,
    pending_name: Option<&str>,
    create_fallback: &str,
) -> String {
    match mode {
        EditorMode::Edit { name } => pending_name.unwrap_or(name).to_owned(),
        EditorMode::Create => pending_name.unwrap_or(create_fallback).to_owned(),
    }
}

#[must_use]
pub fn secret_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
pub fn secret_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(secret_delete_confirm_prompt(key))
}

#[must_use]
pub fn editor_name_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Rename workspace", current)
}

#[must_use]
pub fn editor_workdir_pick_state<M: crate::tui::components::workdir_pick::WorkdirMount>(
    mounts: &[M],
) -> crate::tui::components::workdir_pick::WorkdirPickState {
    crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(mounts)
}

#[must_use]
pub fn secret_value_input_state<'a>(
    key: &str,
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(format!("Edit {key}"), current)
}

#[must_use]
pub fn secret_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

#[must_use]
pub fn secret_new_value_input_state<'a>(key: &str) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(
        format!("Value for {key}"),
        String::new(),
    )
}

#[must_use]
pub fn secret_source_picker_state(
    key: impl Into<String>,
    op_available: bool,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), op_available)
}

#[must_use]
pub fn secret_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
pub fn secret_new_key_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "New workspace environment key".to_owned(),
        SecretsScopeTag::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
pub fn secret_new_key_after_picker_label(scope: &SecretsScopeTag) -> String {
    format!("New environment key for {}", secrets_scope_label(scope))
}

#[must_use]
pub fn secret_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
pub fn role_load_input_state<'a>(
    trusted_roles: Vec<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state =
        jackin_tui::components::TextInputState::new_with_forbidden("Load role", "", trusted_roles);
    state.forbidden_label = "trusted role registry".into();
    state
}

#[must_use]
pub fn mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn mount_dst_choice_state(
    src: impl Into<String>,
) -> crate::tui::components::mount_dst_choice::MountDstChoiceState {
    crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src)
}

#[must_use]
pub fn role_trust_confirm_state(
    role: String,
    repository: String,
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![("Role".into(), role), ("Repository".into(), repository)],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    )
}

#[must_use]
pub fn isolated_state_save_confirm_state(
    affected_containers: &[String],
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Edit affects preserved isolated state for {} stopped container(s):\n  {}\n\n\
         Delete the preserved state and save?",
        affected_containers.len(),
        affected_containers.join("\n  "),
    ))
}

#[must_use]
pub fn running_isolated_state_save_block_message(affected_containers: &[String]) -> String {
    format!(
        "Cannot save: {} container(s) are running with isolated state for an affected mount: {}; eject them first.",
        affected_containers.len(),
        affected_containers.join(", "),
    )
}

pub fn clamp_editor_scroll_for_frame(
    body: Rect,
    geometry: EditorScrollGeometry,
    tab_scroll_x: &mut u16,
    tab_scroll_y: &mut u16,
    mounts_scroll_x: &mut u16,
) {
    let viewport_w = jackin_tui::components::scrollable_panel::viewport_width(body);
    let viewport_h = jackin_tui::components::scrollable_panel::viewport_height(body);
    if geometry.active_mounts {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.mounts_content_width,
            viewport_w,
            mounts_scroll_x,
        );
    } else {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.content_width,
            viewport_w,
            tab_scroll_x,
        );
    }
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        geometry.content_height,
        viewport_h,
        tab_scroll_y,
    );
}

pub fn editor_body_area(area: Rect, footer_h: u16) -> Rect {
    editor_frame_areas(area, footer_h).body
}

pub fn editor_row_width(label: &str, value: &str) -> usize {
    padded_width(&format!("  {label:15}{value}"))
}

#[must_use]
pub fn editor_general_content_width(
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> usize {
    general_row_widths(
        name_value,
        workdir_display,
        keep_awake_enabled,
        git_pull_on_entry,
    )
    .into_iter()
    .max()
    .unwrap_or(0)
}

#[must_use]
pub fn editor_mount_add_row_width() -> usize {
    text_width("  + Add mount")
}

#[must_use]
pub fn editor_roles_status_width(is_all: bool, allowed_count: usize, total_count: usize) -> usize {
    if is_all {
        text_width("  Allowed roles:    all  ")
    } else {
        text_width(&format!(
            "  Allowed roles:    custom     ({allowed_count} of {total_count} allowed)"
        ))
    }
}

#[must_use]
pub fn editor_role_row_width(role_name: &str) -> usize {
    text_width(&format!("  [x] * {role_name}"))
}

#[must_use]
pub fn editor_role_load_row_width() -> usize {
    text_width("  + Load role")
}

#[must_use]
pub fn general_lines(
    cursor: usize,
    show_cursor: bool,
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> Vec<Line<'static>> {
    let keep_awake_display = if keep_awake_enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    vec![
        render_editor_row(0, cursor, "Name", name_value, show_cursor),
        render_editor_row(1, cursor, "Working dir", workdir_display, show_cursor),
        render_editor_row(2, cursor, "Keep awake", keep_awake_display, show_cursor),
        render_editor_row(3, cursor, "Git pull", git_pull_display, show_cursor),
    ]
}

fn general_row_widths(
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> [usize; 4] {
    let keep_awake_display = if keep_awake_enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    [
        editor_row_width("Name", name_value),
        editor_row_width("Working dir", workdir_display),
        editor_row_width("Keep awake", keep_awake_display),
        editor_row_width("Git pull", git_pull_display),
    ]
}

#[must_use]
pub fn mount_lines(
    rows: &[MountDisplayRow],
    cursor: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'_>> = vec![render_mount_header(path_w)];

    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (i == cursor);
        let hovered = !selected && hovered_row == Some(i);
        let hb = |s: Style| {
            if hovered {
                s.bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER)
            } else {
                s
            }
        };
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let base_style = if selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                hb(base_style),
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                hb(Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
            ),
            Span::styled("  ", hb(Style::default())),
            Span::styled(
                format!("{:<MOUNT_ISOLATION_COL_WIDTH$}", row.isolation),
                hb(Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
            ),
            Span::styled("  ", hb(Style::default())),
            Span::styled(row.kind.clone(), hb(dim_style)),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            )));
        }
    }

    let sentinel_idx = rows.len();
    let sentinel_selected = show_cursor && (cursor == sentinel_idx);
    let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
    if !rows.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add mount"),
        action_row_style(sentinel_selected),
    )));

    lines
}

#[must_use]
pub fn role_lines(
    rows: &[EditorRoleRow],
    allowed_count: usize,
    is_all: bool,
    cursor: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let badge_text = if is_all { "  all  " } else { "  custom  " };
    let badge_bg = if is_all {
        jackin_tui::theme::PHOSPHOR_GREEN
    } else {
        jackin_tui::theme::WHITE
    };
    let badge_style = Style::default()
        .bg(badge_bg)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let mut status_spans = vec![
        Span::styled(
            "  Allowed roles:  ",
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(badge_text, badge_style),
    ];
    if !is_all {
        status_spans.push(Span::styled(
            format!("   ({allowed_count} of {} allowed)", rows.len()),
            Style::default()
                .fg(jackin_tui::theme::ACTION_ACCENT)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    let mut lines = vec![Line::from(status_spans), Line::from("")];

    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (i == cursor);
        let check = if row.effectively_allowed {
            "[x]"
        } else {
            "[ ]"
        };
        let star = if row.is_default { "\u{2605}" } else { " " };
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let text = format!("{prefix}{check} {star} {}", row.name);
        let style = if selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    let sentinel_idx = rows.len();
    let sentinel_selected = show_cursor && (cursor == sentinel_idx);
    let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
    if !rows.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Load role"),
        action_row_style(sentinel_selected),
    )));

    lines
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn secret_lines<'a>(
    rows: &[super::model::SecretsRow],
    cursor: usize,
    show_cursor: bool,
    area_width: u16,
    value_for: impl Fn(&SecretsScopeTag, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SecretsScopeTag, &str) -> bool,
    role_in_registry: impl Fn(&str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(rows.len());
    let label_width = 22;

    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (i == cursor);
        let cursor_col = if selected { "\u{25b8} " } else { "  " };
        match row {
            super::model::SecretsRow::WorkspaceKeyRow(key) => {
                let scope = SecretsScopeTag::Workspace;
                let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
                lines.push(render_secret_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    !is_unmasked(&scope, key),
                    area_width,
                    label_width,
                ));
            }
            super::model::SecretsRow::WorkspaceAddSentinel => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add environment variable"),
                    action_row_style(selected),
                )));
            }
            super::model::SecretsRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
                let mut spans = vec![
                    Span::raw(format!("{cursor_col}     ")),
                    Span::styled(arrow, disclosure_style()),
                    Span::styled(
                        format!(" Role: {role}  ({} vars)", role_var_count(role)),
                        disclosure_style(),
                    ),
                ];
                if !role_in_registry(role) {
                    spans.push(Span::styled(
                        "  (not in registry)",
                        Style::default()
                            .fg(jackin_tui::theme::PHOSPHOR_DIM)
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                lines.push(Line::from(spans));
            }
            super::model::SecretsRow::RoleKeyRow { role, key } => {
                let scope = SecretsScopeTag::Role(role.clone());
                let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
                lines.push(render_secret_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    !is_unmasked(&scope, key),
                    area_width,
                    label_width,
                ));
            }
            super::model::SecretsRow::RoleAddSentinel(role) => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}     + Add {role} environment variable"),
                    action_row_style(selected),
                )));
            }
            super::model::SecretsRow::SectionSpacer => lines.push(Line::from("")),
        }
    }

    lines
}

#[must_use]
pub fn editor_secret_line_width<'a>(
    row: &super::model::SecretsRow,
    area_width: u16,
    value_for: impl Fn(&SecretsScopeTag, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SecretsScopeTag, &str) -> bool,
    role_in_registry: impl Fn(&str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> usize {
    const LABEL_WIDTH: usize = 22;
    match row {
        super::model::SecretsRow::WorkspaceKeyRow(key) => {
            let scope = SecretsScopeTag::Workspace;
            let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
            secret_key_line_width(
                key,
                value,
                !is_unmasked(&scope, key),
                area_width,
                LABEL_WIDTH,
            )
        }
        super::model::SecretsRow::WorkspaceAddSentinel => {
            padded_width("  + Add environment variable")
        }
        super::model::SecretsRow::RoleHeader { role, .. } => {
            let mut width = text_width(&format!(
                "       \u{25bc} Role: {role}  ({} vars)",
                role_var_count(role)
            ));
            if !role_in_registry(role) {
                width += text_width("  (not in registry)");
            }
            padded_width_cols(width, 7)
        }
        super::model::SecretsRow::RoleKeyRow { role, key } => {
            let scope = SecretsScopeTag::Role(role.clone());
            let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
            secret_key_line_width(
                key,
                value,
                !is_unmasked(&scope, key),
                area_width,
                LABEL_WIDTH,
            )
        }
        super::model::SecretsRow::RoleAddSentinel(role) => {
            padded_width(&format!("       + Add {role} environment variable"))
        }
        super::model::SecretsRow::SectionSpacer => 0,
    }
}

fn secret_key_line_width(
    key: &str,
    value: SecretValueDisplay<'_>,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> usize {
    const OP_MARKER: &str = "[op] ";
    const NO_MARKER: &str = "     ";
    const MASK: &str =
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}";
    const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let op_breadcrumb = match value {
        SecretValueDisplay::OpRefPath(path) => {
            crate::tui::op_breadcrumb::parse_path_breadcrumb(path)
        }
        SecretValueDisplay::Plain(_) => None,
    };
    let marker = if op_breadcrumb.is_some() {
        OP_MARKER
    } else {
        NO_MARKER
    };
    let prefix_width =
        text_width("  ") + text_width(marker) + text_width(&format!("{key:label_width$}")) + 2;
    let value_width = if let Some(parts) = op_breadcrumb.as_ref() {
        crate::tui::op_breadcrumb::breadcrumb_display_width(parts)
    } else if masked {
        text_width(MASK)
    } else {
        let plain_str = match value {
            SecretValueDisplay::Plain(value) => value,
            SecretValueDisplay::OpRefPath(_) => OP_REF_REPICK_PLACEHOLDER,
        };
        let budget = (area_width as usize)
            .saturating_sub(label_width)
            .saturating_sub(8)
            .max(1);
        plain_str.chars().count().min(budget)
    };
    padded_width_cols(prefix_width + value_width, 2)
}

#[must_use]
pub fn auth_lines(
    rows: &[EditorAuthLineRow],
    cursor: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| render_auth_line(show_cursor && (i == cursor), row))
        .collect()
}

#[must_use]
pub fn editor_auth_line_width(row: &EditorAuthLineRow) -> usize {
    match row {
        EditorAuthLineRow::AuthKind { label } => padded_width(&format!("  {label}")),
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let suffix = if *inherited { " (inherited)" } else { "" };
            padded_width(&format!("  {:<12}{mode_label}{suffix}", "Mode"))
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            auth_source_line_width("Source", display, 0)
        }
        EditorAuthLineRow::RoleHeader { role, .. } => {
            padded_width(&format!("\u{25bc} Role: {role}"))
        }
        EditorAuthLineRow::RoleMode { mode_label } => {
            padded_width(&format!("      {:<12}{mode_label}", "Mode"))
        }
        EditorAuthLineRow::RoleSource { display } => auth_source_line_width("Source", display, 6),
        EditorAuthLineRow::AddSentinel { .. } => padded_width("  + Override for a role"),
        EditorAuthLineRow::Spacer => 0,
    }
}

fn render_auth_line(selected: bool, row: &EditorAuthLineRow) -> Line<'static> {
    let bold_white = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let dim_green = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let phosphor = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    match row {
        EditorAuthLineRow::AuthKind { label } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(label.clone(), bold_white),
            ])
        }
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            let suffix = if *inherited { " (inherited)" } else { "" };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(format!("{:<12}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), phosphor),
                Span::styled(suffix.to_owned(), dim_green),
            ])
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            render_auth_source_line("Source", display, 0)
        }
        EditorAuthLineRow::RoleHeader { role, expanded } => {
            let glyph = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
            Line::from(vec![
                Span::styled(glyph.to_owned(), disclosure_style()),
                Span::styled(format!(" Role: {role}"), disclosure_style()),
            ])
        }
        EditorAuthLineRow::RoleMode { mode_label } => Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{:<12}", "Mode"), bold_white),
            Span::styled(mode_label.clone(), phosphor),
        ]),
        EditorAuthLineRow::RoleSource { display } => render_auth_source_line("Source", display, 6),
        EditorAuthLineRow::AddSentinel { .. } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled("+ Override for a role", action_row_style(selected)),
            ])
        }
        EditorAuthLineRow::Spacer => Line::from(""),
    }
}

fn auth_source_line_width(label: &str, display: &AuthSourceDisplay, indent: usize) -> usize {
    let label_width = if indent == 0 { 14 } else { 12 };
    let prefix_width = indent + text_width(&format!("{label:<label_width$}"));
    let value_width = match display {
        AuthSourceDisplay::NotRequired => text_width("not required"),
        AuthSourceDisplay::OpRefPath(path) => {
            text_width("[op] ")
                + crate::tui::op_breadcrumb::parse_path_breadcrumb(path).map_or_else(
                    || text_width("<unparseable path - re-pick>"),
                    |parts| crate::tui::op_breadcrumb::breadcrumb_display_width(&parts),
                )
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            text_width(&"\u{25cf}".repeat((*chars).clamp(1, 12)))
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => text_width(&format!("unset  ({env_name} for {mode_label})")),
    };
    padded_width_cols(prefix_width + value_width, indent)
}

fn render_auth_source_line(
    label: &str,
    display: &AuthSourceDisplay,
    indent: usize,
) -> Line<'static> {
    let label_width = if indent == 0 { 14 } else { 12 };
    let mut spans = vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled(
                "not required",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled(
                "[op] ",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
            crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => {
            spans.push(Span::styled(
                format!("unset  ({env_name} for {mode_label})"),
                Style::default().fg(jackin_tui::theme::DANGER_RED),
            ));
        }
    }

    Line::from(spans)
}

fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    show_cursor: bool,
) -> Line<'static> {
    let selected = show_cursor && (row == cursor);
    let prefix = if selected { "\u{25b8} " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::WHITE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::WHITE)
    };
    let value_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label:15}"), label_style),
        Span::styled(value.to_owned(), value_style),
    ])
}

pub fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

pub fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

pub fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[must_use]
pub fn tab_labels(active: EditorTab) -> Vec<(&'static str, bool)> {
    EditorTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub fn secrets_scope_label(scope: &SecretsScopeTag) -> &str {
    match scope {
        SecretsScopeTag::Workspace => "workspace",
        SecretsScopeTag::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn secrets_forbidden_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_owned(),
        SecretsScopeTag::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn secret_key_input_state<'a>(
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    forbidden_keys: Vec<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state =
        jackin_tui::components::TextInputState::new_with_forbidden(label, initial, forbidden_keys);
    state.forbidden_label = secrets_forbidden_label(scope);
    state
}

#[must_use]
pub fn secret_key_input_state_from_pending<'a, R, V>(
    workspace_env: &std::collections::BTreeMap<String, V>,
    roles: &std::collections::BTreeMap<String, R>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    role_env: impl Fn(&R) -> &std::collections::BTreeMap<String, V>,
) -> jackin_tui::components::TextInputState<'a> {
    secret_key_input_state(
        scope,
        label,
        initial,
        forbidden_secret_keys(workspace_env, roles, scope, role_env),
    )
}

#[cfg(test)]
mod tests;
