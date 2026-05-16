use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::BTreeMap;

use super::{
    FooterItem, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, footer_height, render_footer, render_header,
};
use crate::console::manager::auth_kind::AuthKind;
use crate::console::manager::render::list::{
    MOUNT_MODE_COL_WIDTH, format_mount_rows, mount_path_width,
};
use crate::console::manager::state::{
    GlobalMountModal, SettingsAuthModal, SettingsEnvModal, SettingsEnvScope, SettingsState,
    SettingsTab,
};
use crate::operator_env::EnvValue;

pub(in crate::console::manager) fn global_mounts_content_width(
    rows: &[crate::config::GlobalMountRow],
) -> usize {
    let lines = global_mount_lines(rows, None, false);
    super::max_line_width(&lines)
}

pub(super) fn render_settings(
    frame: &mut Frame,
    state: &mut SettingsState<'_>,
    op_available: bool,
) {
    use super::modal::{
        settings_auth_modal_footer_items, settings_env_modal_footer_items,
        settings_mounts_modal_footer_items,
    };
    let area = frame.area();
    // When a modal is open, show its keys in the footer (the "behind" keys are unreachable).
    // Check in priority order: auth modal > env modal > mounts modal > no modal.
    let footer = if let Some(modal) = &state.auth.modal {
        settings_auth_modal_footer_items(modal)
    } else if let Some(modal) = &state.env.modal {
        settings_env_modal_footer_items(modal)
    } else if let Some(modal) = &state.mounts.modal {
        settings_mounts_modal_footer_items(modal)
    } else {
        footer_items(state, op_available)
    };
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
    super::editor::render_tab_strip(frame, chunks[1], &labels, state.tab_bar_focused);

    match state.active_tab {
        SettingsTab::General => render_general_tab(frame, state, chunks[2]),
        SettingsTab::Mounts => render_mounts_tab(frame, state, chunks[2]),
        SettingsTab::Environments => render_env_tab(frame, state, chunks[2]),
        SettingsTab::Auth => render_auth_tab(frame, state, chunks[2]),
        SettingsTab::Trust => render_trust_tab(frame, state, chunks[2]),
    }

    render_footer(frame, chunks[3], &footer);
}

fn render_general_tab(
    frame: &mut Frame,
    state: &SettingsState<'_>,
    area: ratatui::layout::Rect,
) {
    let lines = general_lines(state);
    let mut sx = 0u16;
    let mut sy = 0u16;
    super::render_scrollable_block(frame, area, lines, &mut sx, &mut sy, false, None);
}

fn general_lines(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    let header = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let on_style = Style::default().fg(PHOSPHOR_GREEN).add_modifier(Modifier::BOLD);
    let off_style = Style::default().fg(PHOSPHOR_DIM);
    let label_style = Style::default().fg(WHITE);
    let value = if state.general.pending {
        "enabled"
    } else {
        "disabled"
    };
    let value_style = if state.general.pending {
        on_style
    } else {
        off_style
    };
    vec![
        Line::from(Span::styled(
            "  Setting                        Value",
            header,
        )),
        Line::from(vec![
            Span::styled("\u{25b8} ", on_style),
            Span::styled("Auto co-author trailer           ", label_style),
            Span::styled(value, value_style),
        ]),
    ]
}

fn render_mounts_tab(
    frame: &mut Frame,
    state: &mut SettingsState<'_>,
    area: ratatui::layout::Rect,
) {
    let mut lines = global_mount_lines(&state.mounts.pending, Some(state.mounts.selected), true);
    if let Some(err) = &state.mounts.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(crate::console::widgets::auth_panel::DANGER_RED),
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
            Style::default().fg(crate::console::widgets::auth_panel::DANGER_RED),
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
            Style::default().fg(crate::console::widgets::auth_panel::DANGER_RED),
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
            Style::default().fg(crate::console::widgets::auth_panel::DANGER_RED),
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

fn footer_items(state: &SettingsState<'_>, op_available: bool) -> Vec<FooterItem> {
    if state.tab_bar_focused {
        // Tab bar has focus: show tab-navigation keys, then global actions.
        let mut items = vec![
            FooterItem::Key("\u{2190}\u{2192}"),
            FooterItem::Text("switch tab"),
            FooterItem::GroupSep,
            FooterItem::Key("Tab/\u{2193}"),
            FooterItem::Text("enter content"),
        ];
        items.extend([
            FooterItem::GroupSep,
            FooterItem::Key("S"),
            FooterItem::Text("save settings"),
        ]);
        if state.is_dirty() {
            items.push(FooterItem::Dyn(format!(
                "({} changes)",
                state.change_count()
            )));
        }
        items.extend([
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text(if state.is_dirty() { "discard" } else { "back" }),
        ]);
        return items;
    }

    // Content area has focus.
    let mut items = vec![
        FooterItem::Key("\u{2191}\u{2193}"),
        FooterItem::Text("navigate"),
    ];

    let row_items = contextual_row_items(state, op_available);
    if !row_items.is_empty() {
        items.push(FooterItem::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        FooterItem::GroupSep,
        FooterItem::Key("BackTab"),
        FooterItem::Text("tab bar"),
        FooterItem::GroupSep,
    ]);
    items.extend([FooterItem::Key("S"), FooterItem::Text("save settings")]);
    if state.is_dirty() {
        items.push(FooterItem::Dyn(format!(
            "({} changes)",
            state.change_count()
        )));
    }
    items.extend([
        FooterItem::GroupSep,
        FooterItem::Key("Esc"),
        FooterItem::Text(if state.is_dirty() { "discard" } else { "back" }),
    ]);
    items
}

#[allow(clippy::too_many_lines)]
fn contextual_row_items(state: &SettingsState<'_>, op_available: bool) -> Vec<FooterItem> {
    match state.active_tab {
        SettingsTab::General => {
            vec![
                FooterItem::Key("Space"),
                FooterItem::Text("toggle"),
            ]
        }
        SettingsTab::Mounts => {
            let cursor = state.mounts.selected;
            let mount_count = state.mounts.pending.len();
            if cursor == mount_count {
                vec![FooterItem::Key("Enter/A"), FooterItem::Text("add")]
            } else {
                let mut items = vec![
                    FooterItem::Key("D"),
                    FooterItem::Text("remove"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                ];
                if let Some(row) = state.mounts.pending.get(cursor)
                    && matches!(
                        super::super::mount_info::inspect(&row.mount.src),
                        super::super::mount_info::MountKind::Git {
                            origin: Some(super::super::mount_info::GitOrigin::Github { .. }),
                            ..
                        }
                    )
                {
                    items.push(FooterItem::Sep);
                    items.push(FooterItem::Key("O"));
                    items.push(FooterItem::Text("open in GitHub"));
                }
                items.extend([
                    FooterItem::Sep,
                    FooterItem::Key("R"),
                    FooterItem::Text("toggle ro/rw"),
                    FooterItem::Sep,
                    FooterItem::Key("N"),
                    FooterItem::Text("rename"),
                    FooterItem::Sep,
                    FooterItem::Key("1"),
                    FooterItem::Text("edit source"),
                    FooterItem::Sep,
                    FooterItem::Key("2"),
                    FooterItem::Text("edit dst"),
                    FooterItem::Sep,
                    FooterItem::Key("3"),
                    FooterItem::Text("edit scope"),
                    FooterItem::Sep,
                    FooterItem::Key("H/L"),
                    FooterItem::Text("scroll"),
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
                        FooterItem::Key("Enter"),
                        FooterItem::Sep,
                        FooterItem::Key("P"),
                        FooterItem::Text("re-pick from 1Password"),
                        FooterItem::Sep,
                        FooterItem::Key("D"),
                        FooterItem::Text("delete"),
                        FooterItem::Sep,
                        FooterItem::Key("A"),
                        FooterItem::Text("add"),
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
                        FooterItem::Key("Enter"),
                        FooterItem::Text("edit"),
                        FooterItem::Sep,
                        FooterItem::Key("D"),
                        FooterItem::Text("delete"),
                        FooterItem::Sep,
                        FooterItem::Key("A"),
                        FooterItem::Text("add"),
                        FooterItem::Sep,
                        FooterItem::Key("M"),
                        FooterItem::Text("mask/unmask"),
                    ];
                    if op_available {
                        items.push(FooterItem::Sep);
                        items.push(FooterItem::Key("P"));
                        items.push(FooterItem::Text("1Password"));
                    }
                    items
                }
                Some(SettingsEnvRow::RoleHeader { .. }) => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("expand"),
                    FooterItem::Sep,
                    FooterItem::Key("←/→"),
                    FooterItem::Text("collapse/expand"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                ],
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    let mut items = vec![FooterItem::Key("Enter"), FooterItem::Text("add")];
                    if op_available {
                        items.extend([
                            FooterItem::Sep,
                            FooterItem::Key("P"),
                            FooterItem::Text("1Password"),
                        ]);
                    }
                    items
                }
                Some(SettingsEnvRow::SectionSpacer) | None => Vec::new(),
            }
        }
        SettingsTab::Auth => {
            if state.auth.selected_kind.is_none() {
                vec![FooterItem::Key("Enter"), FooterItem::Text("manage auth")]
            } else if state.auth.selected == 0 {
                // Esc here pops back to the auth list; the global footer already
                // shows Esc for the settings-level exit — omit it here to avoid duplication.
                vec![FooterItem::Key("Enter"), FooterItem::Text("edit mode")]
            } else {
                vec![FooterItem::Key("Enter"), FooterItem::Text("edit source")]
            }
        }
        SettingsTab::Trust => {
            if state.trust.pending.is_empty() {
                Vec::new()
            } else {
                vec![
                    FooterItem::Key("Space"),
                    FooterItem::Text("trust/untrust"),
                    FooterItem::Sep,
                    FooterItem::Key("H/L"),
                    FooterItem::Text("scroll"),
                ]
            }
        }
    }
}

fn global_mount_lines(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows(&mounts);
    let path_w = mount_path_width(&display_rows);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "  {path:<path_w$}  {mode:<MOUNT_MODE_COL_WIDTH$}  Type",
            path = "Destination",
            mode = "Mode"
        ),
        Style::default().fg(WHITE),
    ))];
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
                    Style::default().fg(crate::console::widgets::auth_panel::DANGER_RED),
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
        let style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
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
            let area = super::modal::op_picker_rect(frame.area());
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
