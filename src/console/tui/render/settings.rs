#![expect(
    clippy::redundant_pub_crate,
    reason = "manager update code uses selected render geometry helpers through the moved tui facade"
)]

use ratatui::{
    Frame,
    layout::Rect,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
};
use super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
use jackin_console::tui::auth::AuthKind;
use crate::console::tui::render::modal_layout::{
    auth_form_rect, confirm_rect, mount_choice_rect, op_picker_rect, role_picker_rect,
    scope_picker_rect, source_picker_rect, text_input_rect,
};
use crate::console::tui::render::mount_display::{
    MOUNT_MODE_COL_WIDTH, format_mount_rows_with_cache, mount_path_width,
};
pub(crate) use crate::console::tui::state::SettingsEnvRow;
use crate::console::tui::state::{
    GlobalMountModal, MountInfoCache, SettingsAuthModal, SettingsEnvModal, SettingsEnvScope,
    SettingsState, SettingsTab,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::editor_rows::{
    SecretValueDisplay, action_row_style, render_tab_strip,
};
use jackin_console::tui::screens::settings::view::{
    env_lines as settings_env_lines, general_lines as settings_general_lines, tab_labels,
    trust_lines as settings_trust_lines,
};
use jackin_console::tui::view::{footer_height, render_footer, render_header};

pub(super) fn render_settings(
    frame: &mut Frame,
    area: Rect,
    state: &SettingsState<'_>,
    op_available: bool,
) {
    let footer =
        crate::console::tui::render::footer::settings::settings_footer_items(state, op_available);
    let footer_h = footer_height(&footer, area.width).max(1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    render_header(frame, chunks[0], "settings");
    render_tab_strip(
        frame,
        chunks[1],
        &tab_labels(state.active_tab),
        state.tab_bar_focused,
        state.hovered_tab,
    );

    match state.active_tab {
        SettingsTab::General => render_general_tab(frame, state, chunks[2]),
        SettingsTab::Mounts => render_mounts_tab(frame, state, chunks[2]),
        SettingsTab::Environments => render_env_tab(frame, state, chunks[2]),
        SettingsTab::Auth => render_auth_tab(frame, state, chunks[2]),
        SettingsTab::Trust => render_trust_tab(frame, state, chunks[2]),
    }

    render_footer(frame, chunks[3], &footer);
}

fn render_general_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let focused = !state.tab_bar_focused && state.error_popup.is_none();
    let lines = settings_general_lines(
        state.general.selected,
        state.general.pending_coauthor_trailer,
        state.general.pending_dco,
        focused,
    );
    super::render_scrollable_block_at(frame, area, lines, 0, 0, focused, None);
}

fn render_mounts_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    // Only show the cursor when the mounts content block is focused — not when
    // the tab bar owns focus. Pass None as `selected` to suppress `▸` entirely.
    let focused =
        !state.tab_bar_focused && state.mounts.scroll_focused && state.mounts.modal.is_none();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_lines(
        &state.mounts.pending,
        selected,
        true,
        &state.mounts.mount_info_cache,
    );
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll_x,
        state.mounts.scroll_y,
        focused,
        None,
    );
}

fn render_env_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let lines = env_lines(state, area.width);
    let focused = !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    super::render_scrollable_block_at(frame, area, lines, 0, state.env.scroll_y, focused, None);
}

fn render_auth_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let title = state.auth.selected_kind.map(|k| format!(" {} ", k.label()));
    let lines = auth_lines(state);
    let focused = !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll_y,
        focused,
        title.as_deref(),
    );
}

fn render_trust_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let lines = trust_lines(state);
    let focused = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll_x,
        state.trust.scroll_y,
        focused,
        None,
    );
}

fn global_mount_lines(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
    cache: &MountInfoCache,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    let path_w = mount_path_width(&display_rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if !display_rows.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {path:<path_w$}  {mode:<MOUNT_MODE_COL_WIDTH$}  Type",
                path = "Destination",
                mode = "Mode"
            ),
            Style::default().fg(WHITE),
        )));
    }
    for (i, row) in display_rows.iter().enumerate() {
        let is_selected = selected == Some(i);
        let prefix = if is_selected { "▸ " } else { "  " };
        let base_style = if is_selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                base_style,
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(row.kind.clone(), dim_style),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    if include_sentinel {
        let sentinel_selected = selected == Some(rows.len());
        let sentinel_prefix = if sentinel_selected { "▸ " } else { "  " };
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

fn env_lines(state: &SettingsState<'_>, area_width: u16) -> Vec<Line<'static>> {
    let rows = settings_env_flat_rows(state);
    let show_cursor =
        !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    settings_env_lines(
        &rows,
        state.env.selected,
        show_cursor,
        area_width,
        |scope, key| settings_env_value(state, scope, key).map(secret_value_display),
        |scope, key| state.env.unmasked_rows.contains(&(scope.clone(), key.to_string())),
        |role| state.env.pending.roles.get(role).map_or(0, std::collections::BTreeMap::len),
    )
}

fn secret_value_display(value: &EnvValue) -> SecretValueDisplay<'_> {
    match value {
        EnvValue::Plain(value) => SecretValueDisplay::Plain(value),
        EnvValue::OpRef(op_ref) => SecretValueDisplay::OpRefPath(&op_ref.path),
    }
}

fn settings_env_flat_rows(state: &SettingsState<'_>) -> Vec<SettingsEnvRow> {
    jackin_console::tui::screens::settings::update::settings_env_flat_rows(
        &state.env.pending,
        &state.env.expanded,
    )
}

fn settings_env_value<'a>(
    state: &'a SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    match scope {
        SettingsEnvScope::Global => state.env.pending.env.get(key),
        SettingsEnvScope::Role(role) => state
            .env
            .pending
            .roles
            .get(role)
            .and_then(|env| env.get(key)),
    }
}

fn auth_lines(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    use crate::console::tui::auth_panel::mode_str;

    let bold_white = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(PHOSPHOR_GREEN);
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let show_cursor =
        !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
    let Some(kind) = state.auth.selected_kind else {
        return state
            .auth
            .pending
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let selected = show_cursor && (state.auth.selected == i);
                let cursor_col = if selected { "▸ " } else { "  " };
                Line::from(Span::styled(
                    format!("{cursor_col}{}", row.kind.label()),
                    bold_white,
                ))
            })
            .collect();
    };
    let Some(row) = state.auth.pending.iter().find(|row| row.kind == kind) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mode_selected = show_cursor && (state.auth.selected == 0);
    let mode_style = if mode_selected {
        phosphor.add_modifier(Modifier::BOLD)
    } else {
        phosphor
    };
    let cursor_col = if mode_selected { "▸ " } else { "  " };
    lines.push(Line::from(vec![
        Span::styled(cursor_col, mode_style),
        Span::styled(format!("{:<14}", "Mode"), bold_white),
        Span::styled(mode_str(row.mode).to_string(), mode_style),
    ]));
    if let Some(env_name) = kind.required_env_var(row.mode) {
        let source_selected = show_cursor && (state.auth.selected == 1);
        let source_style = if source_selected {
            dim.add_modifier(Modifier::BOLD)
        } else {
            dim
        };
        let cursor_col = if source_selected { "▸ " } else { "  " };
        let mut spans = vec![
            Span::styled(cursor_col, source_style),
            Span::styled(format!("{:<14}", "Source"), bold_white),
        ];
        match settings_auth_source_value(state, kind, env_name) {
            Some(EnvValue::Plain(value)) if !value.is_empty() => {
                spans.push(Span::styled(
                    "●".repeat(value.chars().count().clamp(1, 12)),
                    source_style,
                ));
            }
            Some(EnvValue::OpRef(op_ref)) => {
                spans.push(Span::styled("[op] ", source_style));
                jackin_console::tui::components::op_breadcrumb::push_op_breadcrumb_spans(
                    &mut spans,
                    &op_ref.path,
                );
            }
            _ => {
                spans.push(Span::styled(
                    format!("unset  ({env_name} for {})", mode_str(row.mode)),
                    Style::default().fg(jackin_tui::theme::DANGER_RED),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    lines
}

fn settings_auth_source_value<'a>(
    state: &'a SettingsState<'_>,
    kind: AuthKind,
    env_name: &str,
) -> Option<&'a EnvValue> {
    if kind == AuthKind::Github {
        state.auth.github_env.get(env_name)
    } else {
        state.env.pending.env.get(env_name)
    }
}

fn trust_lines(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    settings_trust_lines(
        &state.trust.pending,
        state.trust.selected,
        state.trust.hovered,
        show_cursor,
    )
}

pub(super) fn render_global_mount_modal(frame: &mut Frame, modal: &GlobalMountModal<'_>) {
    match modal {
        GlobalMountModal::Text { state, .. } => {
            let area = text_input_rect(frame.area());
            jackin_tui::components::render_text_input(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            let area = super::centered_rect_fixed(frame.area(), 70, 22);
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            let area = mount_choice_rect(frame.area());
            jackin_console::tui::components::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            let area = scope_picker_rect(frame.area());
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            let area = role_picker_rect(frame.area(), state);
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            let area = confirm_rect(frame.area(), state);
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            use jackin_console::tui::components::confirm_save;
            let height = confirm_save::required_height(state).min(frame.area().height);
            let area = super::centered_rect_fixed(frame.area(), 80, height);
            confirm_save::render(frame, area, state);
        }
    }
}

pub(super) fn render_settings_env_modal(frame: &mut Frame, modal: &SettingsEnvModal<'_>) {
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            let area = text_input_rect(frame.area());
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            let area = source_picker_rect(frame.area());
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            let area = op_picker_rect(frame.area());
            crate::console::tui::op_picker::render::render(frame, area, state);
        }
        SettingsEnvModal::RolePicker { state } => {
            let area = role_picker_rect(frame.area(), state);
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            let area = scope_picker_rect(frame.area());
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            let area = confirm_rect(frame.area(), state);
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
    }
}

pub(super) fn render_settings_auth_modal(frame: &mut Frame, modal: &SettingsAuthModal<'_>) {
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let area = auth_form_rect(frame.area(), state);
            crate::console::tui::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            let area = source_picker_rect(frame.area());
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            let area = text_input_rect(frame.area());
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            // A naming sub-stage is a plain input box, sized like every
            // other text-input modal; drill-down stages use the picker rect.
            let area = if state.naming_stage_input().is_some() {
                text_input_rect(frame.area())
            } else {
                op_picker_rect(frame.area())
            };
            crate::console::tui::op_picker::render::render(frame, area, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_settings_to_dump(state: &SettingsState<'_>) -> String {
        let backend = TestBackend::new(90, 18);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|frame| render_settings(frame, frame.area(), state, false))
            .unwrap();
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn settings_header_does_not_duplicate_active_tab_label() {
        let config = AppConfig::default();
        for tab in SettingsTab::ALL {
            let mut state = SettingsState::from_config(&config);
            state.active_tab = tab;
            let dump = render_settings_to_dump(&state);
            let header = dump.lines().next().unwrap_or_default();
            assert!(
                header.contains("settings"),
                "settings header missing for {tab:?}: {header:?}"
            );
            assert!(
                !header.contains("settings ·"),
                "settings header must not duplicate active tab for {tab:?}: {header:?}"
            );
        }
    }
}
