//! Settings screen view helpers.

use super::model::GlobalMountConfirm;
use super::model::GlobalMountTextTarget;
use super::model::GlobalMountsState;
use super::model::SettingsAuthRow;
use super::model::SettingsEnvConfig;
use super::model::SettingsEnvRow;
use super::model::SettingsEnvScope;
use super::model::SettingsEnvState;
use super::model::SettingsEnvTextTarget;
use super::model::SettingsGeneralState;
use super::model::SettingsTab;
use super::model::SettingsTrustRow;
use super::model::SettingsTrustState;
use super::update::forbidden_settings_env_keys;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::BTreeMap;

use crate::tui::components::editor_rows::{
    AUTH_LABEL_COL_WIDTH, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind,
    SecretValueDisplay, action_row_style, disclosure_style, render_secret_key_line,
};
use crate::tui::components::mount_rows::MOUNT_MODE_COL_WIDTH;
use crate::tui::mount_display::{
    MountDisplayRow, format_config_mount_rows_with_cache, mount_path_width,
};

// Structural exception: settings rows are form/table rows with labels, values,
// disclosures, masked secrets, and action sentinels, so they cannot use the
// flat picker renderer even though they share its focus-gated cursor contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAuthLineRow {
    Kind { label: String },
    Mode { mode_label: String },
    Source { display: AuthSourceDisplay },
    SourceFolder { display: AuthSourceFolderDisplay },
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
pub const fn settings_header_title() -> &'static str {
    "settings"
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
pub fn global_mount_confirm_state(
    action: GlobalMountConfirm,
) -> jackin_tui::components::ConfirmState {
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
pub fn global_mount_scope_text_value(scope: Option<&str>) -> String {
    scope.unwrap_or_default().to_owned()
}

#[must_use]
pub const fn global_mount_text_target_label(
    target: &GlobalMountTextTarget,
) -> Option<&'static str> {
    match target {
        GlobalMountTextTarget::AddScope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::AddName => Some("Mount name"),
        GlobalMountTextTarget::AddSource => Some("Source"),
        GlobalMountTextTarget::AddDestination => Some("Destination"),
        GlobalMountTextTarget::Source => Some("Source"),
        GlobalMountTextTarget::Destination => Some("Destination"),
        GlobalMountTextTarget::Scope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::Rename => Some("Rename mount"),
    }
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
pub fn settings_env_value_text_label(key: &str) -> String {
    format!("Edit {key}")
}

#[must_use]
pub fn settings_env_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
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
pub fn settings_env_new_key_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "New global environment key".to_owned(),
        SettingsEnvScope::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
pub fn settings_env_new_key_after_picker_label(scope: &SettingsEnvScope) -> String {
    format!("New environment key for {}", env_scope_label(scope))
}

#[must_use]
pub fn settings_env_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
pub fn settings_env_empty_key_error_message() -> &'static str {
    "Env key cannot be empty."
}

#[must_use]
pub fn global_mount_name_empty_message() -> &'static str {
    "Mount name cannot be empty."
}

#[must_use]
pub fn global_mount_gone_message() -> &'static str {
    "Mount no longer exists; selection was cleared."
}

#[must_use]
pub fn global_mount_add_draft_lost_message() -> &'static str {
    "Add-mount draft was lost; press 'a' to start over."
}

#[must_use]
pub fn global_mount_destination_empty_message() -> &'static str {
    "Mount destination cannot be empty."
}

#[must_use]
pub fn global_mount_no_github_url_message() -> &'static str {
    "no GitHub URL for this mount"
}

#[must_use]
pub fn settings_no_registered_roles_error_message() -> &'static str {
    "No registered roles available."
}

#[must_use]
pub fn settings_sensitive_paths_not_confirmed_message() -> &'static str {
    "Save aborted: sensitive paths not confirmed."
}

#[must_use]
pub fn settings_error_popup_title() -> &'static str {
    "Settings error"
}

#[must_use]
pub fn settings_auth_op_read_failed_message(error: impl std::fmt::Display) -> String {
    format!("1Password read failed: {error}")
}

#[must_use]
pub fn env_forbidden_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_owned(),
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
pub fn general_state_lines(state: &SettingsGeneralState, show_cursor: bool) -> Vec<Line<'static>> {
    general_lines(
        state.selected,
        state.pending_coauthor_trailer,
        state.pending_dco,
        show_cursor,
    )
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
pub fn trust_state_lines(
    state: &SettingsTrustState,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    trust_lines(&state.pending, state.selected, hovered_row, show_cursor)
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
                    Span::raw(cursor_col.to_owned()),
                    Span::styled(arrow.to_owned(), disclosure_style()),
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
pub fn env_state_lines<Modal>(
    state: &SettingsEnvState<jackin_core::EnvValue, Modal>,
    show_cursor: bool,
    area_width: u16,
) -> Vec<Line<'static>> {
    let rows = crate::tui::screens::settings::update::settings_env_flat_rows(
        &state.pending,
        &state.expanded,
    );
    env_lines(
        &rows,
        state.selected,
        show_cursor,
        area_width,
        |scope, key| {
            state
                .pending_value(scope, key)
                .map(crate::tui::components::env_value::secret_display)
        },
        |scope, key| state.is_unmasked(scope, key),
        |role| state.pending.roles.get(role).map_or(0, BTreeMap::len),
    )
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
                Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), mode_style),
            ])
        }
        SettingsAuthLineRow::Source { display } => render_auth_source_line(display, selected),
        SettingsAuthLineRow::SourceFolder { display } => {
            render_auth_source_folder_line(display, selected)
        }
        SettingsAuthLineRow::Spacer => Line::from(""),
    }
}

fn render_auth_source_folder_line(
    display: &AuthSourceFolderDisplay,
    selected: bool,
) -> Line<'static> {
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let source_style = if selected {
        dim.add_modifier(Modifier::BOLD)
    } else {
        dim
    };
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let value = match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    };
    Line::from(vec![
        Span::styled(cursor_col, source_style),
        Span::styled(
            format!("{:<AUTH_LABEL_COL_WIDTH$}", "Source folder"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, source_style),
    ])
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
            format!("{:<AUTH_LABEL_COL_WIDTH$}", "Source"),
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

#[must_use]
pub fn global_mount_state_lines<Modal>(
    state: &GlobalMountsState<jackin_config::GlobalMountRow, Modal>,
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let mounts = state
        .pending
        .iter()
        .map(|row| row.mount.clone())
        .collect::<Vec<_>>();
    let display_rows = format_config_mount_rows_with_cache(&mounts, &state.mount_info_cache);
    global_mount_lines(&display_rows, selected, include_sentinel)
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
    detail_row_count: impl Fn(K, &M) -> usize,
    has_error: bool,
) -> usize
where
    K: Copy + PartialEq,
{
    let height = match selected_kind {
        None => rows.len(),
        Some(kind) => rows
            .iter()
            .find(|row| row.kind == kind)
            .map_or(0, |row| 1 + detail_row_count(kind, &row.mode)),
    };
    content_height_with_error_rows(height, has_error)
}

#[cfg(test)]
mod tests;
