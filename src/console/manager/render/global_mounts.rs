use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::BTreeMap;

use super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, footer_height, render_footer, render_header};
use crate::console::manager::auth_kind::AuthKind;
use crate::console::manager::render::list::{
    MOUNT_MODE_COL_WIDTH, format_mount_rows_with_cache, mount_path_width,
};
use crate::console::manager::state::{
    GlobalMountModal, MountInfoCache, SettingsAuthModal, SettingsEnvModal, SettingsEnvScope,
    SettingsState, SettingsTab,
};
use crate::operator_env::EnvValue;
use jackin_tui::HintSpan;

pub(in crate::console::manager) fn global_mounts_content_width(
    rows: &[crate::config::GlobalMountRow],
) -> usize {
    let cache = MountInfoCache::default();
    cache.refresh_global_rows(rows);
    global_mounts_content_width_with_cache(rows, &cache)
}

pub(in crate::console::manager) fn global_mounts_content_width_with_cache(
    rows: &[crate::config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let lines = global_mount_lines(rows, None, false, &cache);
    super::max_line_width(&lines)
}

pub(super) fn render_settings(
    frame: &mut Frame,
    state: &mut SettingsState<'_>,
    op_available: bool,
) {
    let area = frame.area();
    let footer = settings_footer_items(state, op_available);
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
    let labels = SettingsTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == state.active_tab))
        .collect::<Vec<_>>();
    super::editor::render_tab_strip(
        frame,
        chunks[1],
        &labels,
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

pub(super) fn settings_footer_items(
    state: &SettingsState<'_>,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    use super::modal::{
        settings_auth_modal_footer_items, settings_env_modal_footer_items,
        settings_mounts_modal_footer_items,
    };
    // When a modal is open, show its keys in the footer (the "behind" keys are unreachable).
    // Check in priority order: auth modal > env modal > mounts modal > no modal.
    if state.auth.modal.is_some() {
        settings_auth_modal_footer_items(&state.auth)
    } else if let Some(modal) = &state.env.modal {
        settings_env_modal_footer_items(modal)
    } else if let Some(modal) = &state.mounts.modal {
        settings_mounts_modal_footer_items(modal)
    } else {
        footer_items(state, op_available)
    }
}

fn render_general_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let lines = general_lines(state);
    let mut sx = 0u16;
    let mut sy = 0u16;
    super::render_scrollable_block(frame, area, lines, &mut sx, &mut sy, false, None);
}

fn general_lines(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    let label_bold = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let label_normal = Style::default().fg(WHITE);
    let value_bold = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let value_normal = Style::default().fg(PHOSPHOR_GREEN);

    let rows: [(usize, &str, bool); 2] = [
        (
            0,
            "Co-author trailer",
            state.general.pending_coauthor_trailer,
        ),
        (1, "DCO sign-off", state.general.pending_dco),
    ];

    rows.iter()
        .map(|(i, label, pending)| {
            let selected = state.general.selected == *i;
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

fn render_mounts_tab(
    frame: &mut Frame,
    state: &mut SettingsState<'_>,
    area: ratatui::layout::Rect,
) {
    let mut lines = global_mount_lines(
        &state.mounts.pending,
        Some(state.mounts.selected),
        true,
        &state.mounts.mount_info_cache,
    );
    if let Some(err) = &state.mounts.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(crate::console::widgets::DANGER_RED),
        )));
    }
    super::render_scrollable_block(
        frame,
        area,
        lines,
        &mut state.mounts.scroll_x,
        &mut state.mounts.scroll_y,
        state.mounts.scroll_focused,
        None,
    );
}

fn render_env_tab(frame: &mut Frame, state: &mut SettingsState<'_>, area: ratatui::layout::Rect) {
    let mut lines = env_lines(state, area.width);
    if let Some(err) = &state.env.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(crate::console::widgets::DANGER_RED),
        )));
    }
    let mut no_scroll_x = 0u16;
    super::render_scrollable_block(
        frame,
        area,
        lines,
        &mut no_scroll_x,
        &mut state.env.scroll_y,
        state.env.scroll_focused,
        None,
    );
}

fn render_auth_tab(frame: &mut Frame, state: &mut SettingsState<'_>, area: ratatui::layout::Rect) {
    let title = state.auth.selected_kind.map(|k| format!(" {} ", k.label()));
    let mut lines = auth_lines(state);
    if let Some(err) = &state.auth.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(crate::console::widgets::DANGER_RED),
        )));
    }
    let mut no_scroll_x = 0u16;
    super::render_scrollable_block(
        frame,
        area,
        lines,
        &mut no_scroll_x,
        &mut state.auth.scroll_y,
        state.auth.scroll_focused,
        title.as_deref(),
    );
}

fn render_trust_tab(frame: &mut Frame, state: &mut SettingsState<'_>, area: ratatui::layout::Rect) {
    let mut lines = trust_lines(state);
    if let Some(err) = &state.trust.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(crate::console::widgets::DANGER_RED),
        )));
    }
    super::render_scrollable_block(
        frame,
        area,
        lines,
        &mut state.trust.scroll_x,
        &mut state.trust.scroll_y,
        state.trust.scroll_focused,
        None,
    );
}

/// Natural content width of the Trust tab (used by mouse scroll).
pub(in crate::console::manager) fn trust_content_width(state: &SettingsState<'_>) -> usize {
    super::max_line_width(&trust_lines(state))
}

pub(in crate::console::manager) fn mounts_content_height(state: &SettingsState<'_>) -> usize {
    let mut height = global_mount_lines(
        &state.mounts.pending,
        Some(state.mounts.selected),
        true,
        &state.mounts.mount_info_cache,
    )
    .len();
    if state.mounts.error.is_some() {
        height = height.saturating_add(2);
    }
    height
}

pub(in crate::console::manager) fn env_content_height(
    state: &SettingsState<'_>,
    area_width: u16,
) -> usize {
    let mut height = env_lines(state, area_width).len();
    if state.env.error.is_some() {
        height = height.saturating_add(2);
    }
    height
}

pub(in crate::console::manager) fn auth_content_height(state: &SettingsState<'_>) -> usize {
    let mut height = auth_lines(state).len();
    if state.auth.error.is_some() {
        height = height.saturating_add(2);
    }
    height
}

pub(in crate::console::manager) fn trust_content_height(state: &SettingsState<'_>) -> usize {
    let mut height = trust_lines(state).len();
    if state.trust.error.is_some() {
        height = height.saturating_add(2);
    }
    height
}

fn footer_items(state: &SettingsState<'_>, op_available: bool) -> Vec<HintSpan<'static>> {
    if state.tab_bar_focused {
        // Tab bar has focus: show tab-navigation keys, then global actions.
        let mut items = vec![
            HintSpan::Key("\u{2190}\u{2192}"),
            HintSpan::Text("switch tab"),
            HintSpan::GroupSep,
            HintSpan::Key("Tab/\u{2193}"),
            HintSpan::Text("enter content"),
        ];
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("S"),
            HintSpan::Text("save settings"),
        ]);
        if state.is_dirty() {
            items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
        }
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text(if state.is_dirty() { "discard" } else { "back" }),
        ]);
        return items;
    }

    // Content area has focus.
    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
    ];

    let row_items = contextual_row_items(state, op_available);
    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("BackTab"),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
    ]);
    items.extend([HintSpan::Key("S"), HintSpan::Text("save settings")]);
    if state.is_dirty() {
        items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text(if state.is_dirty() { "discard" } else { "back" }),
    ]);
    items
}

#[allow(clippy::too_many_lines)]
fn contextual_row_items(state: &SettingsState<'_>, op_available: bool) -> Vec<HintSpan<'static>> {
    match state.active_tab {
        SettingsTab::General => {
            vec![
                HintSpan::Key("\u{2191}\u{2193}"),
                HintSpan::Text("navigate"),
                HintSpan::Sep,
                HintSpan::Key("Space"),
                HintSpan::Text("toggle"),
            ]
        }
        SettingsTab::Mounts => {
            let cursor = state.mounts.selected;
            let mount_count = state.mounts.pending.len();
            if cursor == mount_count {
                vec![HintSpan::Key("Enter/A"), HintSpan::Text("add")]
            } else {
                let mut items = vec![
                    HintSpan::Key("D"),
                    HintSpan::Text("remove"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("add"),
                ];
                if state
                    .mounts
                    .pending
                    .get(cursor)
                    .and_then(|row| state.mounts.mount_info_cache.github_web_url(&row.mount.src))
                    .is_some()
                {
                    items.push(HintSpan::Sep);
                    items.push(HintSpan::Key("O"));
                    items.push(HintSpan::Text("open in GitHub"));
                }
                items.extend([
                    HintSpan::Sep,
                    HintSpan::Key("R"),
                    HintSpan::Text("toggle ro/rw"),
                    HintSpan::Sep,
                    HintSpan::Key("N"),
                    HintSpan::Text("rename"),
                    HintSpan::Sep,
                    HintSpan::Key("1"),
                    HintSpan::Text("edit source"),
                    HintSpan::Sep,
                    HintSpan::Key("2"),
                    HintSpan::Text("edit dst"),
                    HintSpan::Sep,
                    HintSpan::Key("3"),
                    HintSpan::Text("edit scope"),
                    HintSpan::Sep,
                    HintSpan::Key("H/L"),
                    HintSpan::Text("scroll"),
                ]);
                items
            }
        }
        SettingsTab::Environments => {
            let rows = settings_env_flat_rows(state);
            match rows.get(state.env.selected) {
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_env_value_is_op_ref(state, scope, key) =>
                {
                    let mut items = vec![
                        HintSpan::Key("Enter"),
                        HintSpan::Sep,
                        HintSpan::Key("P"),
                        HintSpan::Text("re-pick from 1Password"),
                        HintSpan::Sep,
                        HintSpan::Key("D"),
                        HintSpan::Text("delete"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                    ];
                    if op_available {
                        // Enter/P both work; if 1Password is unavailable, hint is less useful.
                    } else {
                        // 1Password not available; remove the Enter/P hint.
                        items.drain(..4);
                    }
                    items
                }
                Some(SettingsEnvRow::Key { .. }) => {
                    let mut items = vec![
                        HintSpan::Key("Enter"),
                        HintSpan::Text("edit"),
                        HintSpan::Sep,
                        HintSpan::Key("D"),
                        HintSpan::Text("delete"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                        HintSpan::Sep,
                        HintSpan::Key("M"),
                        HintSpan::Text("mask/unmask"),
                    ];
                    if op_available {
                        items.push(HintSpan::Sep);
                        items.push(HintSpan::Key("P"));
                        items.push(HintSpan::Text("1Password"));
                    }
                    items
                }
                Some(SettingsEnvRow::RoleHeader { .. }) => vec![
                    HintSpan::Key("Enter"),
                    HintSpan::Text("expand"),
                    HintSpan::Sep,
                    HintSpan::Key("←/→"),
                    HintSpan::Text("collapse/expand"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("add"),
                ],
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    let mut items = vec![HintSpan::Key("Enter"), HintSpan::Text("add")];
                    if op_available {
                        items.extend([
                            HintSpan::Sep,
                            HintSpan::Key("P"),
                            HintSpan::Text("1Password"),
                        ]);
                    }
                    items
                }
                Some(SettingsEnvRow::SectionSpacer) | None => Vec::new(),
            }
        }
        SettingsTab::Auth => {
            if state.auth.selected_kind.is_none() {
                vec![HintSpan::Key("Enter"), HintSpan::Text("manage auth")]
            } else if state.auth.selected == 0 {
                // Esc here pops back to the auth list; the global footer already
                // shows Esc for the settings-level exit — omit it here to avoid duplication.
                vec![HintSpan::Key("Enter"), HintSpan::Text("edit mode")]
            } else {
                vec![HintSpan::Key("Enter"), HintSpan::Text("edit source")]
            }
        }
        SettingsTab::Trust => {
            if state.trust.pending.is_empty() {
                Vec::new()
            } else {
                vec![
                    HintSpan::Key("Space"),
                    HintSpan::Text("trust/untrust"),
                    HintSpan::Sep,
                    HintSpan::Key("H/L"),
                    HintSpan::Text("scroll"),
                ]
            }
        }
    }
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
            super::editor::action_row_style(sentinel_selected),
        )));
    }
    lines
}

fn env_lines(state: &SettingsState<'_>, area_width: u16) -> Vec<Line<'static>> {
    let rows = settings_env_flat_rows(state);
    let mut lines = Vec::with_capacity(rows.len());
    let label_width = 22;
    for (i, row) in rows.iter().enumerate() {
        let selected = state.env.selected == i;
        let cursor_col = if selected { "▸ " } else { "  " };
        match row {
            SettingsEnvRow::Key { scope, key } => {
                let Some(value) = settings_env_value(state, scope, key) else {
                    continue;
                };
                let masked = !state
                    .env
                    .unmasked_rows
                    .contains(&(scope.clone(), key.clone()));
                lines.push(super::editor::render_secrets_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    masked,
                    area_width,
                    label_width,
                ));
            }
            SettingsEnvRow::GlobalAddSentinel => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add environment variable"),
                    super::editor::action_row_style(selected),
                )));
            }
            SettingsEnvRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "▼" } else { "▶" };
                let count = state.env.pending.roles.get(role).map_or(0, BTreeMap::len);
                lines.push(Line::from(vec![
                    Span::raw(cursor_col.to_string()),
                    Span::styled(arrow.to_string(), super::editor::disclosure_style()),
                    Span::styled(
                        format!(" Role: {role}  ({count} vars)"),
                        super::editor::disclosure_style(),
                    ),
                ]));
            }
            SettingsEnvRow::RoleAddSentinel(role) => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add {role} environment variable"),
                    super::editor::action_row_style(selected),
                )));
            }
            SettingsEnvRow::SectionSpacer => lines.push(Line::from("")),
        }
    }
    lines
}

#[derive(Debug, Clone)]
pub(in crate::console::manager) enum SettingsEnvRow {
    Key {
        scope: SettingsEnvScope,
        key: String,
    },
    GlobalAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleAddSentinel(String),
    SectionSpacer,
}

pub(in crate::console::manager) fn settings_env_flat_rows(
    state: &SettingsState<'_>,
) -> Vec<SettingsEnvRow> {
    let mut rows = Vec::new();
    for key in state.env.pending.env.keys() {
        rows.push(SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: key.clone(),
        });
    }
    if !state.env.pending.env.is_empty() {
        rows.push(SettingsEnvRow::SectionSpacer);
    }
    rows.push(SettingsEnvRow::GlobalAddSentinel);
    for (role, role_env) in &state.env.pending.roles {
        if role_env.is_empty() {
            continue;
        }
        rows.push(SettingsEnvRow::SectionSpacer);
        let expanded = state.env.expanded.contains(role);
        rows.push(SettingsEnvRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            if let Some(env) = state.env.pending.roles.get(role) {
                for key in env.keys() {
                    rows.push(SettingsEnvRow::Key {
                        scope: SettingsEnvScope::Role(role.clone()),
                        key: key.clone(),
                    });
                }
            }
            rows.push(SettingsEnvRow::SectionSpacer);
            rows.push(SettingsEnvRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
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

fn settings_env_value_is_op_ref(
    state: &SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> bool {
    settings_env_value(state, scope, key)
        .is_some_and(|value| matches!(value, crate::operator_env::EnvValue::OpRef(_)))
}

fn auth_lines(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    use crate::console::widgets::auth_panel::mode_str;

    let bold_white = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(PHOSPHOR_GREEN);
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let Some(kind) = state.auth.selected_kind else {
        return state
            .auth
            .pending
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let selected = state.auth.selected == i;
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
    let mode_style = if state.auth.selected == 0 {
        phosphor.add_modifier(Modifier::BOLD)
    } else {
        phosphor
    };
    let cursor_col = if state.auth.selected == 0 {
        "▸ "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(cursor_col, mode_style),
        Span::styled(format!("{:<14}", "Mode"), bold_white),
        Span::styled(mode_str(row.mode).to_string(), mode_style),
    ]));
    if let Some(env_name) = kind.required_env_var(row.mode) {
        let source_style = if state.auth.selected == 1 {
            dim.add_modifier(Modifier::BOLD)
        } else {
            dim
        };
        let cursor_col = if state.auth.selected == 1 {
            "▸ "
        } else {
            "  "
        };
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
                super::editor::push_op_breadcrumb_spans(&mut spans, &op_ref.path);
            }
            _ => {
                spans.push(Span::styled(
                    format!("unset  ({env_name} for {})", mode_str(row.mode)),
                    Style::default().fg(crate::console::widgets::DANGER_RED),
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
    let mut lines = vec![Line::from(Span::styled(
        "  Role                         Trust      Git",
        Style::default().fg(WHITE),
    ))];
    if state.trust.pending.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    }
    for (i, row) in state.trust.pending.iter().enumerate() {
        let selected = state.trust.selected == i;
        let mut style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        // Hover lift: graphite background on the hovered (non-selected) row,
        // matching the tab/list hover cue.
        if !selected && state.trust.hovered == Some(i) {
            style = style.bg(super::TAB_BG_INACTIVE_HOVER);
        }
        let prefix = if selected { "▸ " } else { "  " };
        let trust = if row.trusted { "trusted" } else { "untrusted" };
        lines.push(Line::from(Span::styled(
            format!(
                "{prefix}{:<28} {:<10} {}",
                truncate(&row.role, 28),
                trust,
                row.git // full URL — horizontal scroll handles overflow
            ),
            style,
        )));
    }
    lines
}

pub(super) fn render_global_mount_modal(frame: &mut Frame, modal: &mut GlobalMountModal<'_>) {
    match modal {
        GlobalMountModal::Text { state, .. } => {
            let area = super::modal::text_input_rect(frame.area());
            crate::console::widgets::text_input::render(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            let area = super::centered_rect_fixed(frame.area(), 70, 22);
            crate::console::widgets::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            let area = super::modal::mount_choice_rect(frame.area());
            crate::console::widgets::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            let area = super::modal::scope_picker_rect(frame.area());
            crate::console::widgets::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            let area = super::modal::role_picker_rect(frame.area(), state);
            crate::console::widgets::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            let area = super::modal::confirm_rect(frame.area(), state);
            crate::console::widgets::confirm::render(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            use crate::console::widgets::confirm_save;
            let height = confirm_save::required_height(state).min(frame.area().height);
            let area = super::centered_rect_fixed(frame.area(), 80, height);
            confirm_save::render(frame, area, state);
        }
    }
}

pub(super) fn render_settings_env_modal(frame: &mut Frame, modal: &mut SettingsEnvModal<'_>) {
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            let area = super::modal::text_input_rect(frame.area());
            crate::console::widgets::text_input::render(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            let area = super::modal::source_picker_rect(frame.area());
            crate::console::widgets::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            let area = super::modal::op_picker_rect(frame.area());
            crate::console::widgets::op_picker::render::render(frame, area, state);
        }
        SettingsEnvModal::RolePicker { state } => {
            let area = super::modal::role_picker_rect(frame.area(), state);
            crate::console::widgets::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            let area = super::modal::scope_picker_rect(frame.area());
            crate::console::widgets::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            let area = super::modal::confirm_rect(frame.area(), state);
            crate::console::widgets::confirm::render(frame, area, state);
        }
    }
}

pub(super) fn render_settings_auth_modal(frame: &mut Frame, modal: &mut SettingsAuthModal<'_>) {
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let area = super::modal::auth_form_rect(frame.area(), state);
            crate::console::widgets::auth_panel::render::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            let area = super::modal::source_picker_rect(frame.area());
            crate::console::widgets::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            let area = super::modal::text_input_rect(frame.area());
            crate::console::widgets::text_input::render(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            // A naming sub-stage is a plain input box, sized like every
            // other text-input modal; drill-down stages use the picker rect.
            let area = if state.naming_stage_input().is_some() {
                super::modal::text_input_rect(frame.area())
            } else {
                super::modal::op_picker_rect(frame.area())
            };
            crate::console::widgets::op_picker::render::render(frame, area, state);
        }
    }
}

fn truncate(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 1 {
        out.pop();
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_settings_to_dump(state: &mut SettingsState<'_>) -> String {
        let backend = TestBackend::new(90, 18);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|frame| render_settings(frame, state, false))
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
            let dump = render_settings_to_dump(&mut state);
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
