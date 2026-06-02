//! Editor screen view helpers.

use super::model::{EditorTab, SecretsScopeTag};
use super::update::forbidden_secret_keys;
use crate::mount_display::{MountDisplayRow, mount_path_width};
use crate::tui::components::editor_rows::{
    AuthSourceDisplay, SecretValueDisplay, action_row_style, disclosure_style,
    render_secret_key_line,
};
use crate::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, render_mount_header,
};
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
pub fn secret_new_value_input_state<'a>(
    key: &str,
) -> jackin_tui::components::TextInputState<'a> {
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
        SecretsScopeTag::Workspace => "New workspace environment key".to_string(),
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
pub fn role_trust_confirm_state(role: String, repository: String) -> jackin_tui::components::ConfirmState {
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

#[must_use]
pub fn mount_lines(
    rows: &[MountDisplayRow],
    cursor: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line> = vec![render_mount_header(path_w)];

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
    let sentinel_prefix = if sentinel_selected {
        "\u{25b8} "
    } else {
        "  "
    };
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
        let check = if row.effectively_allowed { "[x]" } else { "[ ]" };
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
    let sentinel_prefix = if sentinel_selected {
        "\u{25b8} "
    } else {
        "  "
    };
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
        SecretValueDisplay::OpRefPath(path) => crate::op_breadcrumb::parse_path_breadcrumb(path),
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
        crate::op_breadcrumb::breadcrumb_display_width(parts)
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
        EditorAuthLineRow::AddSentinel { eligible } => {
            let suffix = if *eligible == 0 {
                "   (all roles overridden)"
            } else {
                ""
            };
            padded_width(&format!("  + Override for a role{suffix}"))
        }
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
                Span::styled(suffix.to_string(), dim_green),
            ])
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            render_auth_source_line("Source", display, 0)
        }
        EditorAuthLineRow::RoleHeader { role, expanded } => {
            let glyph = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
            Line::from(vec![
                Span::styled(glyph.to_string(), disclosure_style()),
                Span::styled(format!(" Role: {role}"), disclosure_style()),
            ])
        }
        EditorAuthLineRow::RoleMode { mode_label } => Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{:<12}", "Mode"), bold_white),
            Span::styled(mode_label.clone(), phosphor),
        ]),
        EditorAuthLineRow::RoleSource { display } => render_auth_source_line("Source", display, 6),
        EditorAuthLineRow::AddSentinel { eligible } => {
            let label_style = if *eligible == 0 {
                dim_green
            } else {
                action_row_style(selected)
            };
            let suffix = if *eligible == 0 {
                "   (all roles overridden)".to_string()
            } else {
                String::new()
            };
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled("+ Override for a role", label_style),
                Span::styled(suffix, dim_green),
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
                + crate::op_breadcrumb::parse_path_breadcrumb(path)
                    .map(|parts| crate::op_breadcrumb::breadcrumb_display_width(&parts))
                    .unwrap_or_else(|| text_width("<unparseable path - re-pick>"))
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
        Span::styled(value.to_string(), value_style),
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
        SecretsScopeTag::Workspace => "workspace env".to_string(),
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
    let mut state = jackin_tui::components::TextInputState::new_with_forbidden(
        label,
        initial,
        forbidden_keys,
    );
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
mod tests {
    use super::*;

    #[test]
    fn general_lines_highlight_selected_row() {
        let lines = general_lines(2, true, "demo", "~/repo", true, false);

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].spans[0].content.as_ref(), "  Name           ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "\u{25b8} Keep awake     ");
        assert_eq!(lines[2].spans[1].content.as_ref(), "enabled (macOS only)");
        assert_eq!(lines[3].spans[1].content.as_ref(), "disabled");
    }

    #[test]
    fn editor_frame_areas_match_header_tabs_body_footer_contract() {
        let areas = editor_frame_areas(Rect::new(0, 0, 80, 20), 2);

        assert_eq!(areas.header, Rect::new(0, 0, 80, 3));
        assert_eq!(areas.tabs, Rect::new(0, 3, 80, 2));
        assert_eq!(areas.body, Rect::new(0, 5, 80, 13));
        assert_eq!(areas.footer, Rect::new(0, 18, 80, 2));
        assert_eq!(editor_body_area(Rect::new(0, 0, 80, 20), 2), areas.body);
    }

    #[test]
    fn secret_delete_confirm_prompt_names_key() {
        assert_eq!(
            secret_delete_confirm_prompt("TOKEN"),
            "Delete environment variable TOKEN?"
        );
    }

    #[test]
    fn editor_modal_state_helpers_name_fields() {
        assert_eq!(editor_name_input_state("demo").label, "Rename workspace");
        assert_eq!(editor_name_input_state("demo").value(), "demo");
        assert_eq!(secret_value_input_state("TOKEN", "value").label, "Edit TOKEN");
        assert!(secret_value_input_state("TOKEN", "").is_valid());
        assert_eq!(secret_new_value_input_state("TOKEN").label, "Value for TOKEN");
        assert!(secret_new_value_input_state("TOKEN").is_valid());
        assert_eq!(mount_destination_input_state("/workspace").label, "Destination");
        assert_eq!(mount_destination_input_state("/workspace").value(), "/workspace");
    }

    #[test]
    fn secret_source_picker_state_names_key() {
        let state = secret_source_picker_state("TOKEN", true);

        assert_eq!(state.key, "TOKEN");
        assert!(state.op_available);
    }

    #[test]
    fn secret_new_key_labels_follow_scope() {
        assert_eq!(
            secret_new_key_label(&SecretsScopeTag::Workspace),
            "New workspace environment key"
        );
        assert_eq!(
            secret_new_key_label(&SecretsScopeTag::Role("alpha".to_string())),
            "New alpha environment key"
        );
        assert_eq!(
            secret_new_key_after_picker_label(&SecretsScopeTag::Workspace),
            "New environment key for workspace"
        );
        assert_eq!(
            secret_new_key_after_picker_label(&SecretsScopeTag::Role("alpha".to_string())),
            "New environment key for alpha"
        );
        assert_eq!(secret_empty_key_label(), "Key cannot be empty");
    }

    #[test]
    fn role_load_input_state_names_registry_guard() {
        let state = role_load_input_state(vec!["known/role".to_string()]);

        assert_eq!(state.label, "Load role");
        assert_eq!(state.forbidden_label, "trusted role registry");
        assert_eq!(state.value(), "");
    }

    #[test]
    fn secret_delete_confirm_state_uses_key_prompt() {
        let state = secret_delete_confirm_state("TOKEN");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert_eq!(prompt, "Delete environment variable TOKEN?");
    }

    #[test]
    fn role_trust_confirm_state_names_role_and_repository() {
        let state = role_trust_confirm_state("alpha".to_string(), "https://example.test/role".to_string());

        assert_eq!(state.title(), "Trust role source");
        let jackin_tui::components::ConfirmKind::Details { prompt, rows, .. } = state.kind()
        else {
            panic!("expected detail confirm");
        };
        assert_eq!(prompt, "Trust this role source?");
        assert!(rows
            .iter()
            .any(|(label, value)| label == "Repository" && value == "https://example.test/role"));
    }

    #[test]
    fn isolated_state_save_confirm_state_lists_containers() {
        let state = isolated_state_save_confirm_state(&["one".to_string(), "two".to_string()]);

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert!(prompt.contains("2 stopped container(s)"));
        assert!(prompt.contains("one"));
        assert!(prompt.contains("two"));
    }

    #[test]
    fn secret_key_input_state_marks_scope_duplicates() {
        let state = secret_key_input_state(
            &SecretsScopeTag::Role("alpha".to_string()),
            "New alpha key",
            "TOKEN",
            vec!["TOKEN".to_string()],
        );

        assert_eq!(state.label, "New alpha key");
        assert_eq!(state.forbidden_label, "role alpha");
        assert!(state.is_duplicate());
    }

    #[test]
    fn secret_key_input_state_from_pending_marks_scope_duplicates() {
        #[derive(Default)]
        struct Role {
            env: std::collections::BTreeMap<String, String>,
        }

        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "alpha".to_string(),
            Role {
                env: std::collections::BTreeMap::from([("TOKEN".to_string(), "x".to_string())]),
            },
        );
        let workspace = std::collections::BTreeMap::from([("WORKSPACE".to_string(), "x".to_string())]);

        let state = secret_key_input_state_from_pending(
            &workspace,
            &roles,
            &SecretsScopeTag::Role("alpha".to_string()),
            "New alpha key",
            "TOKEN",
            |role| &role.env,
        );

        assert_eq!(state.forbidden_label, "role alpha");
        assert!(state.is_duplicate());
    }

    #[test]
    fn mount_lines_render_header_rows_and_sentinel() {
        let rows = [MountDisplayRow {
            destination: "/workspace".to_string(),
            host_source: Some("host: ~/project".to_string()),
            mode: "rw",
            isolation: "shared",
            kind: "bind".to_string(),
        }];

        let lines = mount_lines(&rows, 1, Some(0), true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  Destination      Mode  Isolation  Type");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
        assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
    }

    #[test]
    fn role_lines_render_status_rows_roles_and_sentinel() {
        let rows = vec![
            EditorRoleRow {
                name: "alpha".to_string(),
                effectively_allowed: true,
                is_default: false,
            },
            EditorRoleRow {
                name: "beta".to_string(),
                effectively_allowed: false,
                is_default: true,
            },
        ];

        let lines = role_lines(&rows, 1, false, 2, true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  Allowed roles:  ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "  custom  ");
        assert_eq!(lines[0].spans[2].content.as_ref(), "   (1 of 2 allowed)");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  [x]   alpha");
        assert_eq!(lines[3].spans[0].content.as_ref(), "  [ ] \u{2605} beta");
        assert_eq!(lines[5].spans[0].content.as_ref(), "\u{25b8} + Load role");
        assert_eq!(
            editor_roles_status_width(false, 1, 2),
            text_width("  Allowed roles:    custom     (1 of 2 allowed)")
        );
        assert_eq!(
            editor_role_row_width("alpha"),
            text_width("  [x] * alpha")
        );
        assert_eq!(editor_role_load_row_width(), text_width("  + Load role"));
    }

    #[test]
    fn secret_lines_render_workspace_and_role_rows() {
        let rows = vec![
            super::super::model::SecretsRow::WorkspaceKeyRow("TOKEN".to_string()),
            super::super::model::SecretsRow::WorkspaceAddSentinel,
            super::super::model::SecretsRow::RoleHeader {
                role: "alpha".to_string(),
                expanded: true,
            },
            super::super::model::SecretsRow::RoleKeyRow {
                role: "alpha".to_string(),
                key: "ROLE_TOKEN".to_string(),
            },
            super::super::model::SecretsRow::RoleAddSentinel("alpha".to_string()),
        ];

        let lines = secret_lines(
            &rows,
            3,
            true,
            80,
            |scope, key| match (scope, key) {
                (SecretsScopeTag::Workspace, "TOKEN") => Some(SecretValueDisplay::Plain("secret")),
                (SecretsScopeTag::Role(role), "ROLE_TOKEN") if role == "alpha" => {
                    Some(SecretValueDisplay::OpRefPath("op://Vault/Item/field"))
                }
                _ => None,
            },
            |scope, key| matches!((scope, key), (SecretsScopeTag::Workspace, "TOKEN")),
            |_| true,
            |_| 1,
        );

        assert_eq!(lines[0].spans[2].content.as_ref(), "TOKEN                 ");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  + Add environment variable");
        assert_eq!(lines[2].spans[2].content.as_ref(), " Role: alpha  (1 vars)");
        assert_eq!(lines[3].spans[0].content.as_ref(), "\u{25b8} ");
        assert_eq!(lines[4].spans[0].content.as_ref(), "       + Add alpha environment variable");
        assert_eq!(
            editor_secret_line_width(
                &rows[0],
                80,
                |scope, key| match (scope, key) {
                    (SecretsScopeTag::Workspace, "TOKEN") => Some(SecretValueDisplay::Plain("secret")),
                    _ => None,
                },
                |scope, key| matches!((scope, key), (SecretsScopeTag::Workspace, "TOKEN")),
                |_| true,
                |_| 1,
            ),
            39
        );
        assert_eq!(
            editor_secret_line_width(&rows[1], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
            padded_width("  + Add environment variable")
        );
        assert_eq!(
            editor_secret_line_width(&rows[2], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
            padded_width("       \u{25bc} Role: alpha  (1 vars)")
        );
        assert_eq!(
            editor_secret_line_width(&rows[4], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
            padded_width("       + Add alpha environment variable")
        );
    }

    #[test]
    fn auth_lines_render_kind_mode_source_and_sentinel() {
        let rows = vec![
            EditorAuthLineRow::AuthKind {
                label: "Claude".to_string(),
            },
            EditorAuthLineRow::WorkspaceMode {
                mode_label: "api-key".to_string(),
                inherited: true,
            },
            EditorAuthLineRow::WorkspaceSource {
                display: AuthSourceDisplay::Unset {
                    env_name: "CLAUDE_API_KEY".to_string(),
                    mode_label: "api-key".to_string(),
                },
            },
            EditorAuthLineRow::RoleHeader {
                role: "alpha".to_string(),
                expanded: false,
            },
            EditorAuthLineRow::AddSentinel { eligible: 0 },
        ];

        let lines = auth_lines(&rows, 1, true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
        assert_eq!(lines[1].spans[2].content.as_ref(), "api-key");
        assert_eq!(lines[1].spans[3].content.as_ref(), " (inherited)");
        assert_eq!(lines[2].spans[2].content.as_ref(), "unset  (CLAUDE_API_KEY for api-key)");
        assert_eq!(lines[3].spans[1].content.as_ref(), " Role: alpha");
        assert_eq!(lines[4].spans[2].content.as_ref(), "   (all roles overridden)");
        assert_eq!(editor_auth_line_width(&rows[0]), padded_width("  Claude"));
        assert_eq!(
            editor_auth_line_width(&rows[1]),
            padded_width("  Mode        api-key (inherited)")
        );
        assert_eq!(
            editor_auth_line_width(&rows[2]),
            padded_width("Source        unset  (CLAUDE_API_KEY for api-key)")
        );
        assert_eq!(
            editor_auth_line_width(&rows[4]),
            padded_width("  + Override for a role   (all roles overridden)")
        );
    }
}
