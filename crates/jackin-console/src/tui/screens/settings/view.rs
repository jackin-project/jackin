//! Settings screen view helpers.

use super::model::GlobalMountConfirm;
use super::model::SettingsAuthRow;
use super::model::SettingsEnvConfig;
use super::model::SettingsEnvRow;
use super::model::SettingsEnvScope;
use super::model::SettingsEnvTextTarget;
use super::model::SettingsTab;
use super::model::SettingsTrustRow;
use super::update::forbidden_settings_env_keys;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::mount_display::{MountDisplayRow, mount_path_width};
use crate::tui::components::editor_rows::{
    AuthSourceDisplay, SecretValueDisplay, action_row_style, disclosure_style,
    render_secret_key_line,
};
use crate::tui::components::mount_rows::MOUNT_MODE_COL_WIDTH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAuthLineRow {
    Kind { label: String },
    Mode { mode_label: String },
    Source { display: AuthSourceDisplay },
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

pub fn settings_frame_areas(area: Rect, footer_h: u16) -> SettingsFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    SettingsFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

#[must_use]
pub fn tab_labels(active: SettingsTab) -> Vec<(&'static str, bool)> {
    SettingsTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub const fn global_mount_confirm_prompt(action: GlobalMountConfirm) -> &'static str {
    match action {
        GlobalMountConfirm::Save => "Save settings to ~/.config/jackin/config.toml?",
        GlobalMountConfirm::Sensitive => "Sensitive global mount path detected. Save anyway?",
        GlobalMountConfirm::Remove => "Remove selected global mount?",
        GlobalMountConfirm::Discard => "Discard unsaved global mount changes?",
    }
}

#[must_use]
pub fn global_mount_confirm_state(action: GlobalMountConfirm) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(global_mount_confirm_prompt(action))
}

#[must_use]
pub fn global_mount_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::with_title(
        " Which agent role do you want to add? ",
    )
}

#[must_use]
pub fn global_mount_text_input_state<'a>(
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new(label, initial)
}

#[must_use]
pub fn settings_env_text_input_state<'a>(
    target: &SettingsEnvTextTarget,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    if matches!(target, SettingsEnvTextTarget::EnvValue { .. }) {
        jackin_tui::components::TextInputState::new_allow_empty(label, initial)
    } else {
        jackin_tui::components::TextInputState::new(label, initial)
    }
}

#[must_use]
pub fn settings_env_source_picker_state(
    key: impl Into<String>,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), true)
}

#[must_use]
pub fn settings_env_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
pub fn settings_env_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
pub fn settings_env_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(settings_env_delete_confirm_prompt(key))
}

#[must_use]
pub fn env_scope_label(scope: &SettingsEnvScope) -> &str {
    match scope {
        SettingsEnvScope::Global => "global",
        SettingsEnvScope::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn env_forbidden_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_string(),
        SettingsEnvScope::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn settings_env_key_input_state<'a, V>(
    pending: &SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state = jackin_tui::components::TextInputState::new_with_forbidden(
        label,
        initial,
        forbidden_settings_env_keys(pending, scope),
    );
    state.forbidden_label = env_forbidden_label(scope);
    state
}

#[must_use]
pub fn content_height_with_error_rows(height: usize, has_error: bool) -> usize {
    if has_error {
        height.saturating_add(2)
    } else {
        height
    }
}

#[must_use]
pub fn mounts_content_height(row_height: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_height, has_error)
}

#[must_use]
pub fn env_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_count, has_error)
}

#[must_use]
pub fn trust_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(1 + row_count.max(1), has_error)
}

#[must_use]
pub fn general_lines(
    selected_row: usize,
    pending_coauthor_trailer: bool,
    pending_dco: bool,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let label_bold = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let label_normal = Style::default().fg(jackin_tui::theme::WHITE);
    let value_bold = Style::default()
        .fg(jackin_tui::theme::PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let value_normal = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    let rows: [(usize, &str, bool); 2] = [
        (0, "Co-author trailer", pending_coauthor_trailer),
        (1, "DCO sign-off", pending_dco),
    ];

    rows.iter()
        .map(|(i, label, pending)| {
            let selected = show_cursor && (selected_row == *i);
            let prefix = if selected { "\u{25b8} " } else { "  " };
            let ls = if selected { label_bold } else { label_normal };
            let vs = if selected { value_bold } else { value_normal };
            let value = if *pending { "enabled" } else { "disabled" };
            Line::from(vec![
                Span::styled(prefix, ls),
                Span::styled(format!("{label:<26}"), ls),
                Span::styled(value, vs),
            ])
        })
        .collect()
}

#[must_use]
pub fn trust_lines(
    rows: &[SettingsTrustRow],
    selected_row: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "  Role                         Trust      Git",
        Style::default().fg(jackin_tui::theme::WHITE),
    ))];
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let mut style = if selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        if !selected && hovered_row == Some(i) {
            style = style.bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER);
        }
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let trust = if row.trusted { "trusted" } else { "untrusted" };
        lines.push(Line::from(Span::styled(
            format!(
                "{prefix}{:<28} {:<10} {}",
                truncate(&row.role, 28),
                trust,
                row.git
            ),
            style,
        )));
    }
    lines
}

#[must_use]
pub fn env_lines<'a>(
    rows: &[SettingsEnvRow],
    selected_row: usize,
    show_cursor: bool,
    area_width: u16,
    value_for: impl Fn(&SettingsEnvScope, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SettingsEnvScope, &str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(rows.len());
    let label_width = 22;
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let cursor_col = if selected { "\u{25b8} " } else { "  " };
        match row {
            SettingsEnvRow::Key { scope, key } => {
                let Some(value) = value_for(scope, key) else {
                    continue;
                };
                lines.push(render_secret_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    !is_unmasked(scope, key),
                    area_width,
                    label_width,
                ));
            }
            SettingsEnvRow::GlobalAddSentinel => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add environment variable"),
                    action_row_style(selected),
                )));
            }
            SettingsEnvRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
                lines.push(Line::from(vec![
                    Span::raw(cursor_col.to_string()),
                    Span::styled(arrow.to_string(), disclosure_style()),
                    Span::styled(
                        format!(" Role: {role}  ({} vars)", role_var_count(role)),
                        disclosure_style(),
                    ),
                ]));
            }
            SettingsEnvRow::RoleAddSentinel(role) => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add {role} environment variable"),
                    action_row_style(selected),
                )));
            }
            SettingsEnvRow::SectionSpacer => lines.push(Line::from("")),
        }
    }
    lines
}

#[must_use]
pub fn auth_lines(
    rows: &[SettingsAuthLineRow],
    selected_row: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = show_cursor && (selected_row == i);
            render_auth_line(row, selected)
        })
        .collect()
}

fn render_auth_line(row: &SettingsAuthLineRow, selected: bool) -> Line<'static> {
    let bold_white = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    match row {
        SettingsAuthLineRow::Kind { label } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(Span::styled(format!("{cursor_col}{label}"), bold_white))
        }
        SettingsAuthLineRow::Mode { mode_label } => {
            let mode_style = if selected {
                phosphor.add_modifier(Modifier::BOLD)
            } else {
                phosphor
            };
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::styled(cursor_col, mode_style),
                Span::styled(format!("{:<14}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), mode_style),
            ])
        }
        SettingsAuthLineRow::Source { display } => render_auth_source_line(display, selected),
        SettingsAuthLineRow::Spacer => Line::from(""),
    }
}

fn render_auth_source_line(display: &AuthSourceDisplay, selected: bool) -> Line<'static> {
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let source_style = if selected {
        dim.add_modifier(Modifier::BOLD)
    } else {
        dim
    };
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let mut spans = vec![
        Span::styled(cursor_col, source_style),
        Span::styled(
            format!("{:<14}", "Source"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled("not required", source_style));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled("[op] ", source_style));
            crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                source_style,
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

#[must_use]
pub fn global_mount_lines(
    rows: &[MountDisplayRow],
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if !rows.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {path:<path_w$}  {mode:<MOUNT_MODE_COL_WIDTH$}  Type",
                path = "Destination",
                mode = "Mode"
            ),
            Style::default().fg(jackin_tui::theme::WHITE),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let is_selected = selected == Some(i);
        let prefix = if is_selected { "\u{25b8} " } else { "  " };
        let base_style = if is_selected {
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
                base_style,
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(row.kind.clone(), dim_style),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            )));
        }
    }
    if include_sentinel {
        let sentinel_selected = selected == Some(rows.len());
        let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
        if !rows.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("{sentinel_prefix}+ Add mount"),
            action_row_style(sentinel_selected),
        )));
    }
    lines
}

fn truncate(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 1 {
        out.pop();
        out.push('\u{2026}');
    }
    out
}

pub fn clamp_mounts_scroll_x_for_frame(area: Rect, content_width: usize, scroll_x: &mut u16) {
    let areas = settings_frame_areas(area, 2);
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        content_width,
        jackin_tui::components::scrollable_panel::viewport_width(areas.body),
        scroll_x,
    );
}

#[must_use]
pub fn auth_content_height<K, M>(
    selected_kind: Option<K>,
    rows: &[SettingsAuthRow<K, M>],
    mode_needs_credential: impl Fn(K, &M) -> bool,
    has_error: bool,
) -> usize
where
    K: Copy + PartialEq,
{
    let height = match selected_kind {
        None => rows.len(),
        Some(kind) => rows.iter().find(|row| row.kind == kind).map_or(0, |row| {
            if mode_needs_credential(kind, &row.mode) {
                3
            } else {
                2
            }
        }),
    };
    content_height_with_error_rows(height, has_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Plain,
        Credential,
    }

    #[test]
    fn general_lines_highlight_selected_setting() {
        let lines = general_lines(1, true, false, true);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
        assert_eq!(lines[0].spans[2].content.as_ref(), "enabled");
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
        assert_eq!(lines[1].spans[2].content.as_ref(), "disabled");
    }

    #[test]
    fn settings_frame_areas_match_header_tabs_body_footer_contract() {
        let areas = settings_frame_areas(Rect::new(0, 0, 80, 20), 2);

        assert_eq!(areas.header, Rect::new(0, 0, 80, 3));
        assert_eq!(areas.tabs, Rect::new(0, 3, 80, 2));
        assert_eq!(areas.body, Rect::new(0, 5, 80, 13));
        assert_eq!(areas.footer, Rect::new(0, 18, 80, 2));
    }

    #[test]
    fn clamp_mounts_scroll_x_for_frame_uses_settings_body_area() {
        let mut scroll_x = u16::MAX;
        let area = Rect::new(0, 0, 80, 20);

        clamp_mounts_scroll_x_for_frame(area, 100, &mut scroll_x);

        let body = settings_frame_areas(area, 2).body;
        let expected = jackin_tui::components::scrollable_panel::max_offset(
            100,
            jackin_tui::components::scrollable_panel::viewport_width(body),
        ) as u16;
        assert_eq!(scroll_x, expected);
    }

    #[test]
    fn tab_content_heights_account_for_error_rows() {
        assert_eq!(mounts_content_height(4, false), 4);
        assert_eq!(mounts_content_height(4, true), 6);
        assert_eq!(env_content_height(3, true), 5);
        assert_eq!(trust_content_height(0, false), 2);
        assert_eq!(trust_content_height(3, true), 6);
    }

    #[test]
    fn global_mount_confirm_prompts_are_settings_owned() {
        assert_eq!(
            global_mount_confirm_prompt(GlobalMountConfirm::Remove),
            "Remove selected global mount?"
        );
        assert_eq!(
            global_mount_confirm_prompt(GlobalMountConfirm::Sensitive),
            "Sensitive global mount path detected. Save anyway?"
        );
    }

    #[test]
    fn global_mount_confirm_state_uses_settings_prompt() {
        let state = global_mount_confirm_state(GlobalMountConfirm::Discard);

        assert_eq!(state.title(), "Confirm");
        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm state");
        };
        assert_eq!(prompt, "Discard unsaved global mount changes?");
    }

    #[test]
    fn settings_env_text_input_state_allows_empty_values_only() {
        let value_target = SettingsEnvTextTarget::EnvValue {
            scope: SettingsEnvScope::Global,
            key: "TOKEN".to_string(),
        };
        let key_target = SettingsEnvTextTarget::EnvKey {
            scope: SettingsEnvScope::Global,
        };

        let value_state = settings_env_text_input_state(&value_target, "Edit TOKEN", "");
        let key_state = settings_env_text_input_state(&key_target, "New key", "");

        assert!(value_state.is_valid());
        assert!(!key_state.is_valid());
    }

    #[test]
    fn settings_env_source_picker_state_names_key() {
        let state = settings_env_source_picker_state("TOKEN");

        assert_eq!(state.key, "TOKEN");
        assert!(state.op_available);
    }

    #[test]
    fn settings_env_delete_confirm_state_uses_key_prompt() {
        let state = settings_env_delete_confirm_state("TOKEN");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm state");
        };
        assert_eq!(prompt, "Delete environment variable TOKEN?");
    }

    #[test]
    fn global_mount_text_input_state_names_label() {
        let state = global_mount_text_input_state("Destination", "/workspace");

        assert_eq!(state.label, "Destination");
        assert_eq!(state.value(), "/workspace");
    }

    #[test]
    fn settings_env_delete_confirm_prompt_names_key() {
        assert_eq!(
            settings_env_delete_confirm_prompt("TOKEN"),
            "Delete environment variable TOKEN?"
        );
    }

    #[test]
    fn settings_env_key_input_state_marks_scope_duplicates() {
        let mut pending = SettingsEnvConfig {
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
        };
        pending.env.insert("GLOBAL".to_string(), "1".to_string());
        pending
            .roles
            .entry("alpha".to_string())
            .or_default()
            .insert("ROLE_TOKEN".to_string(), "2".to_string());

        let state = settings_env_key_input_state(
            &pending,
            &SettingsEnvScope::Role("alpha".to_string()),
            "New alpha environment key",
            "",
        );

        assert_eq!(state.label, "New alpha environment key");
        assert_eq!(state.forbidden_label, "role alpha");
        assert!(!state.is_duplicate());

        let duplicate = settings_env_key_input_state(
            &pending,
            &SettingsEnvScope::Role("alpha".to_string()),
            "New alpha environment key",
            "ROLE_TOKEN",
        );
        assert!(duplicate.is_duplicate());
    }

    #[test]
    fn trust_lines_include_header_empty_row_and_truncate_long_role() {
        let rows = [SettingsTrustRow {
            role: "very-long-role-name-that-will-truncate".to_string(),
            git: "https://github.com/example/role".to_string(),
            trusted: true,
        }];

        let empty = trust_lines(&[], 0, None, false);
        assert_eq!(empty[0].spans[0].content.as_ref(), "  Role                         Trust      Git");
        assert_eq!(empty[1].spans[0].content.as_ref(), "  (none)");

        let lines = trust_lines(&rows, 0, None, true);
        let rendered = lines[1].spans[0].content.as_ref();
        assert!(rendered.starts_with("\u{25b8} very-long-role-name-that-wi\u{2026}"));
        assert!(rendered.contains("trusted"));
        assert!(rendered.contains("https://github.com/example/role"));
    }

    #[test]
    fn auth_lines_render_kind_mode_source_and_spacer() {
        let rows = vec![
            SettingsAuthLineRow::Kind {
                label: "Claude".to_string(),
            },
            SettingsAuthLineRow::Mode {
                mode_label: "api-key".to_string(),
            },
            SettingsAuthLineRow::Source {
                display: AuthSourceDisplay::MaskedPlain { chars: 20 },
            },
            SettingsAuthLineRow::Spacer,
        ];

        let lines = auth_lines(&rows, 2, true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  Claude");
        assert_eq!(lines[1].spans[1].content.as_ref(), "Mode          ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "\u{25b8} ");
        assert_eq!(lines[2].spans[2].content.as_ref(), "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}");
        assert!(lines[3].spans.is_empty());
    }

    #[test]
    fn env_lines_render_key_header_and_sentinels() {
        let rows = vec![
            SettingsEnvRow::Key {
                scope: SettingsEnvScope::Global,
                key: "TOKEN".to_string(),
            },
            SettingsEnvRow::GlobalAddSentinel,
            SettingsEnvRow::RoleHeader {
                role: "architect".to_string(),
                expanded: true,
            },
            SettingsEnvRow::RoleAddSentinel("architect".to_string()),
        ];

        let lines = env_lines(
            &rows,
            1,
            true,
            80,
            |_, key| (key == "TOKEN").then_some(SecretValueDisplay::Plain("secret")),
            |_, key| key == "TOKEN",
            |_| 2,
        );

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} + Add environment variable");
        assert!(lines[2].spans[2].content.contains("Role: architect  (2 vars)"));
        assert_eq!(lines[3].spans[0].content.as_ref(), "  + Add architect environment variable");
    }

    #[test]
    fn global_mount_lines_render_header_rows_and_sentinel() {
        let rows = [MountDisplayRow {
            destination: "/workspace".to_string(),
            host_source: Some("host: ~/project".to_string()),
            mode: "ro",
            isolation: "shared",
            kind: "bind".to_string(),
        }];

        let lines = global_mount_lines(&rows, Some(1), true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  Destination      Mode  Type");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
        assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
    }

    #[test]
    fn auth_content_height_lists_all_kinds_before_drill_in() {
        let rows = vec![
            SettingsAuthRow {
                kind: Kind::Plain,
                mode: false,
            },
            SettingsAuthRow {
                kind: Kind::Credential,
                mode: true,
            },
        ];

        assert_eq!(auth_content_height(None, &rows, |_, mode| *mode, false), 2);
    }

    #[test]
    fn auth_content_height_drill_in_tracks_credential_row_and_error() {
        let rows = vec![SettingsAuthRow {
            kind: Kind::Credential,
            mode: true,
        }];

        assert_eq!(
            auth_content_height(Some(Kind::Credential), &rows, |_, mode| *mode, true),
            5
        );
    }
}
