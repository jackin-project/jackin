//! Editor-stage rendering: full-screen editor with header, tab bar,
//! per-tab body renderers (General / Mounts / Roles / Secrets), and the
//! contextual footer composition that varies with the active tab + cursor.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::state::{EditorMode, EditorState, EditorTab, FieldFocus, SecretsScopeTag};
use super::list::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, format_mount_rows, mount_path_width,
    render_mount_header,
};
use super::{
    FooterItem, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_footer, render_header,
};
use crate::config::AppConfig;
use crate::operator_env::EnvValue;

// ── Editor stage ────────────────────────────────────────────────────

pub fn render_editor(
    frame: &mut Frame,
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit workspace · {name}"),
        EditorMode::Create => "create workspace".to_string(),
    };
    render_header(frame, chunks[0], &title);

    render_tab_strip(frame, chunks[1], state.active_tab);

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, chunks[2], state),
        EditorTab::Mounts => render_mounts_tab(frame, chunks[2], state),
        EditorTab::Roles => render_roles_tab(frame, chunks[2], state, config),
        EditorTab::Secrets => render_secrets_tab(frame, chunks[2], state, config),
        EditorTab::Auth => render_auth_tab(frame, chunks[2], state, config),
    }

    let mut items: Vec<FooterItem> = Vec::new();

    let row_items = contextual_row_items(state, config, op_available);
    if !row_items.is_empty() {
        items.extend(row_items);
        items.push(FooterItem::GroupSep);
    }

    items.push(FooterItem::Key("S"));
    items.push(FooterItem::Text("save workspace"));
    if state.is_dirty() {
        items.push(FooterItem::Dyn(format!(
            "({} changes)",
            state.change_count()
        )));
    }

    items.push(FooterItem::GroupSep);
    items.push(FooterItem::Key("Esc"));
    if state.is_dirty() {
        items.push(FooterItem::Text("discard"));
    } else {
        items.push(FooterItem::Text("back"));
    }

    render_footer(frame, chunks[3], &items);

    // Pre-commit validation surface; the popup handles commit errors.
    if state.modal.is_none()
        && let Some(err) = state.save_flow.error_message()
    {
        let banner_area = Rect {
            x: chunks[2].x,
            y: chunks[2].y,
            width: chunks[2].width,
            height: 1,
        };
        let banner = Paragraph::new(format!("✗ {err}")).style(
            Style::default()
                .fg(Color::Rgb(255, 94, 122))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(ratatui::widgets::Clear, banner_area);
        frame.render_widget(banner, banner_area);
    }
}

/// Compute a row-specific hint fragment based on the active tab and cursor.
/// Returns an empty vec when the current position has no action.
#[allow(clippy::too_many_lines)]
fn contextual_row_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) -> Vec<FooterItem> {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => {
            //   row 0 = Name        (editable — Enter opens rename)
            //   row 1 = Working dir (editable — Enter opens workdir picker)
            //   row 2 = Keep awake  (toggle — Space flips on/off)
            match cursor {
                0 => vec![FooterItem::Key("Enter"), FooterItem::Text("rename")],
                // WorkdirPick requires at least one mount to choose from;
                // suppress the hint when there are none so the key isn't
                // advertised as available when Enter would be a no-op.
                1 if !state.pending.mounts.is_empty() => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("pick working directory"),
                ],
                2 => vec![FooterItem::Key("Space"), FooterItem::Text("toggle")],
                _ => Vec::new(),
            }
        }
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            if cursor < mount_count {
                let mut items = vec![
                    FooterItem::Key("D"),
                    FooterItem::Text("remove"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                ];
                // Surface `O open in GitHub` when the cursor is on a mount
                // whose source resolves to a GitHub-hosted git repo with a
                // web URL. Editor-only — the list view's mounts pane is
                // a preview, not a focus target.
                if let Some(m) = state.pending.mounts.get(cursor)
                    && matches!(
                        super::super::mount_info::inspect(&m.src),
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
                // `R` toggles the readonly flag on the highlighted mount row
                // (rw ↔ ro). Sentinel row omits this hint — there's nothing
                // to toggle yet.
                items.push(FooterItem::Sep);
                items.push(FooterItem::Key("R"));
                items.push(FooterItem::Text("toggle ro/rw"));
                // `I` cycles the per-mount isolation strategy on the
                // highlighted row (shared ↔ worktree).
                // Same gating as R: hidden on the `+ Add mount` sentinel.
                items.push(FooterItem::Sep);
                items.push(FooterItem::Key("I"));
                items.push(FooterItem::Text("cycle isolation"));
                items
            } else {
                // Sentinel "+ Add mount" row — both Enter and A invoke the
                // same add-mount flow, so render as a single combined key.
                vec![FooterItem::Key("Enter/A"), FooterItem::Text("add")]
            }
        }
        EditorTab::Roles => {
            if cursor < config.roles.len() {
                vec![
                    FooterItem::Key("Space"),
                    FooterItem::Text("allow/disallow"),
                    FooterItem::Sep,
                    FooterItem::Key("*"),
                    FooterItem::Text("set/unset default"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add role"),
                ]
            } else {
                vec![FooterItem::Key("Enter/A"), FooterItem::Text("add role")]
            }
        }
        EditorTab::Secrets => {
            // Row-specific hints depend on which SecretsRow kind the cursor
            // is sitting on. Op:// rows are read-only at the value level —
            // the operator deletes and re-adds via the source picker — so
            // we drop `Enter edit` and `M mask/unmask` on those rows.
            let rows = secrets_flat_rows(state);
            // Determine if the focused key row carries an OpRef value.
            let focused_value_is_op_ref = match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(key)) => state
                    .pending
                    .env
                    .get(key)
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
                Some(SecretsRow::RoleKeyRow { role, key }) => state
                    .pending
                    .roles
                    .get(role)
                    .and_then(|ov| ov.env.get(key))
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
                _ => false,
            };
            match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. })
                    if focused_value_is_op_ref =>
                {
                    // Op:// rows: only D delete · A add · Q exit.
                    // Per operator preference, mask/unmask and Enter edit
                    // are suppressed because the breadcrumb isn't a
                    // credential and isn't text-editable.
                    vec![
                        FooterItem::Key("D"),
                        FooterItem::Text("delete"),
                        FooterItem::Sep,
                        FooterItem::Key("A"),
                        FooterItem::Text("add"),
                        FooterItem::Sep,
                        FooterItem::Key("Q"),
                        FooterItem::Text("exit"),
                    ]
                }
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. }) => {
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
                        items.extend([
                            FooterItem::Sep,
                            FooterItem::Key("P"),
                            FooterItem::Text("1Password"),
                        ]);
                    }
                    items
                }
                Some(SecretsRow::RoleHeader { .. }) => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("expand"),
                    FooterItem::Sep,
                    FooterItem::Key("←/→"),
                    FooterItem::Text("collapse/expand"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                ],
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::RoleAddSentinel(_)) => {
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
                // Cursor never lands on `SectionSpacer` (skipped by the
                // `↑`/`↓` handlers), but if anything ever queries the
                // hint for that index we degrade to a no-op empty set.
                Some(SecretsRow::SectionSpacer) | None => vec![],
            }
        }
        EditorTab::Auth => {
            // Auth tab rows are editable workspace + workspace × role
            // entries; the global section is read-only.
            let rows = auth_editable_row_count(state);
            if rows == 0 {
                Vec::new()
            } else {
                vec![FooterItem::Key("Enter"), FooterItem::Text("edit auth")]
            }
        }
    }
}

/// Number of rows in the auth panel that the operator can navigate / edit.
/// Workspace rows (one per agent) plus role × agent rows. The global
/// section is read-only and not part of the editable count.
pub(in crate::console::manager) const fn auth_editable_row_count(state: &EditorState<'_>) -> usize {
    // 2 agents (Claude + Codex) at the workspace layer; another 2 per role.
    let agents = 2;
    agents + state.pending.allowed_roles.len() * agents
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Roles, "Roles"),
        (EditorTab::Secrets, "Environments"),
        (EditorTab::Auth, "Auth"),
    ];
    let mut spans = Vec::new();
    for (tab, label) in labels {
        let style = if tab == active {
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_general_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));

    let FieldFocus::Row(cursor) = state.active_field;

    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };

    // Both Edit and Create modes show the same three rows:
    //   0 = Name        (editable; Enter opens rename TextInput)
    //   1 = Working dir (editable; Enter opens workdir picker)
    //   2 = Keep awake  (toggle; Space flips pending.keep_awake.enabled)
    //
    // The former `Default role` (ro) and `Last used` (ro) rows were
    // removed from the General tab. `Default role` is now editable on the
    // Roles tab (see `*` keybinding); `Last used` was informational
    // clutter and has no place here. The underlying schema fields
    // (`default_role`, `last_role`) still live on `WorkspaceConfig` —
    // we just don't surface them on the General tab anymore.
    //
    // Per-row dirty markers were removed for consistency with the other
    // tabs; the footer's `S save workspace (N changes)` is the canonical
    // unsaved-state indicator.
    let mut rows: Vec<Line> = Vec::new();

    rows.push(render_editor_row(0, cursor, "Name", name_value));
    let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
    rows.push(render_editor_row(
        1,
        cursor,
        "Working dir",
        &workdir_display,
    ));
    // Keep-awake row. The "(macOS only)" suffix when enabled mirrors the
    // CLI `workspace show` output, surfacing the platform constraint
    // exactly where it matters: the moment an operator opts in.
    let keep_awake_display = if state.pending.keep_awake.enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    rows.push(render_editor_row(
        2,
        cursor,
        "Keep awake",
        keep_awake_display,
    ));

    frame.render_widget(Paragraph::new(rows).block(block), area);
}

/// Render a field row with cursor highlight when `row == cursor`.
fn render_editor_row(row: usize, cursor: usize, label: &str, value: &str) -> Line<'static> {
    let selected = row == cursor;
    let prefix = if selected { "▸ " } else { "  " };
    // Labels stay white regardless of focus — focus is signalled by the
    // `▸` prefix and the bold weight, not by a colour shift.
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let mut spans = vec![Span::styled(format!("{prefix}{label:15}"), label_style)];
    let value_style = if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_GREEN)
    };
    spans.push(Span::styled(value.to_string(), value_style));
    Line::from(spans)
}

fn render_mounts_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    let white = WHITE;

    // Build aligned table rows for all mounts.
    let rows = format_mount_rows(&state.pending.mounts);
    let path_w = mount_path_width(&rows);

    // Header row — shares path_w so the "mode" and "type" columns line up
    // with data rows regardless of path width.
    let mut lines: Vec<Line> = vec![render_mount_header(path_w)];

    lines.extend(rows.iter().enumerate().map(|(i, (path, mode, iso, kind))| {
        let selected = i == cursor;
        let prefix = if selected { "▸ " } else { "  " };
        let base_style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        Line::from(vec![
            Span::styled(format!("{prefix}{path:<path_w$}  "), base_style),
            Span::styled(
                format!("{mode:<MOUNT_MODE_COL_WIDTH$}"),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            // Two-space gap before the iso column — matches the header.
            Span::raw("  "),
            Span::styled(
                format!("{iso:<MOUNT_ISOLATION_COL_WIDTH$}"),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            // Two-space gap before the type column — matches the header.
            Span::raw("  "),
            Span::styled(kind.clone(), dim_style),
        ])
    }));

    // Sentinel row: + Add mount — selectable, styled distinctly from mounts.
    let sentinel_idx = state.pending.mounts.len();
    let sentinel_selected = cursor == sentinel_idx;
    let sentinel_prefix = if sentinel_selected { "▸ " } else { "  " };
    let sentinel_style = if sentinel_selected {
        Style::default().fg(white).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(white)
    };
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add mount"),
        sentinel_style,
    )));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_roles_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    // Status line: "Allowed roles:  [ all ]" or "[ custom ]   (3 of 5 allowed)"
    let is_all = super::super::agent_allow::allows_all_agents(&state.pending);
    let total = config.roles.len();
    let allowed_count = state.pending.allowed_roles.len();

    let badge_text = if is_all { "  all  " } else { "  custom  " };
    let badge_bg = if is_all { PHOSPHOR_GREEN } else { WHITE };
    let badge_style = Style::default()
        .bg(badge_bg)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let mut status_spans = vec![
        Span::styled(
            "  Allowed roles:  ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(badge_text, badge_style),
    ];
    if !is_all {
        status_spans.push(Span::styled(
            format!("   ({allowed_count} of {total} allowed)"),
            Style::default()
                .fg(Color::Rgb(180, 255, 180))
                .add_modifier(Modifier::ITALIC),
        ));
    }
    let status_line = Line::from(status_spans);

    // Blank spacer between the status line and the role rows. The old
    // `allowed?  ·  role` column header got dropped — the `[x]` / `[ ]`
    // prefix on each row already signals the toggle semantics, so a
    // dedicated header added noise without clarity.
    let mut lines = vec![status_line, Line::from("")];

    // Role rows. Cursor is 0-based into config.roles (no header offset).
    //
    // `[x]` reflects the *effectively allowed* state, not literal list
    // membership. An empty `allowed_roles` list is the shorthand for
    // "all roles allowed" (matches the `all` badge above) — in that
    // mode every row renders `[x]`. Otherwise only roles named in the
    // list render `[x]`.
    for (i, (role_name, _)) in config.roles.iter().enumerate() {
        let selected = i == cursor;
        let effectively_allowed =
            super::super::agent_allow::agent_is_effectively_allowed(&state.pending, role_name);
        let is_default = state.pending.default_role.as_deref() == Some(role_name.as_str());
        let check = if effectively_allowed { "[x]" } else { "[ ]" };
        let star = if is_default { "★" } else { " " };
        let prefix = if selected { "▸ " } else { "  " };
        let text = format!("{prefix}{check}  {star} {role_name}");
        let style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let sentinel_idx = config.roles.len();
    let sentinel_selected = cursor == sentinel_idx;
    let sentinel_prefix = if sentinel_selected { "▸ " } else { "  " };
    let sentinel_style = if sentinel_selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add role"),
        sentinel_style,
    )));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Flat row model for the Secrets tab; cursor is a single index.
#[derive(Debug, Clone)]
pub(in crate::console::manager) enum SecretsRow {
    WorkspaceKeyRow(String),
    WorkspaceAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleKeyRow {
        role: String,
        key: String,
    },
    RoleAddSentinel(String),
    /// Non-focusable; cursor `↑`/`↓` skip over it.
    SectionSpacer,
}

pub(in crate::console::manager) fn secrets_flat_rows(editor: &EditorState<'_>) -> Vec<SecretsRow> {
    let mut rows = Vec::new();
    for key in editor.pending.env.keys() {
        rows.push(SecretsRow::WorkspaceKeyRow(key.clone()));
    }
    rows.push(SecretsRow::WorkspaceAddSentinel);
    for role in editor.pending.roles.keys() {
        rows.push(SecretsRow::SectionSpacer);
        let expanded = editor.secrets_expanded.contains(role);
        rows.push(SecretsRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            if let Some(ov) = editor.pending.roles.get(role) {
                for key in ov.env.keys() {
                    rows.push(SecretsRow::RoleKeyRow {
                        role: role.clone(),
                        key: key.clone(),
                    });
                }
            }
            rows.push(SecretsRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
}

/// Mirrors launch-time semantics from
/// [`crate::app::context::eligible_roles_for_workspace`]. Roles
/// already carrying an override are NOT filtered — operators may add
/// more keys to an existing override.
pub(in crate::console::manager) fn eligible_agents_for_override(
    editor: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<String> {
    if editor.pending.allowed_roles.is_empty() {
        config.roles.keys().cloned().collect()
    } else {
        editor.pending.allowed_roles.clone()
    }
}

// Linear match per row kind reads better than scattered helpers.
#[allow(clippy::too_many_lines)]
fn render_secrets_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    let rows = secrets_flat_rows(state);
    let mut lines: Vec<Line> = Vec::with_capacity(rows.len());

    // Match General tab's label column for visual rhythm parity.
    let label_width: usize = 22;

    for (i, row) in rows.iter().enumerate() {
        let selected = i == cursor;
        // 7-char prefix: 2-char cursor col + 5-char op-marker col.
        // The marker col is blank on non-op rows so [op] keys line up.
        let cursor_col = if selected { "▸ " } else { "  " };
        match row {
            SecretsRow::WorkspaceKeyRow(key) => {
                let default_value = EnvValue::Plain(String::new());
                let value = state.pending.env.get(key).unwrap_or(&default_value);
                let masked = !state
                    .unmasked_rows
                    .contains(&(SecretsScopeTag::Workspace, key.clone()));
                lines.push(render_secrets_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    masked,
                    area.width,
                    label_width,
                ));
            }
            SecretsRow::WorkspaceAddSentinel => {
                let style = if selected {
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}     + Add environment variable"),
                    style,
                )));
            }
            SecretsRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "▼" } else { "▶" };
                let in_registry = config.roles.contains_key(role);
                let count = state.pending.roles.get(role).map_or(0, |o| o.env.len());
                let mut spans = vec![Span::styled(
                    format!("{cursor_col}     {arrow} Role: {role}  ({count} vars)"),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                )];
                if !in_registry {
                    spans.push(Span::styled(
                        "  (not in registry)",
                        Style::default()
                            .fg(PHOSPHOR_DIM)
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                lines.push(Line::from(spans));
            }
            SecretsRow::RoleKeyRow { role, key } => {
                let empty =
                    std::collections::BTreeMap::<String, crate::operator_env::EnvValue>::new();
                let pend_env = state.pending.roles.get(role).map_or(&empty, |o| &o.env);
                let default_value = EnvValue::Plain(String::new());
                let value = pend_env.get(key).unwrap_or(&default_value);
                let masked = !state
                    .unmasked_rows
                    .contains(&(SecretsScopeTag::Role(role.clone()), key.clone()));
                lines.push(render_secrets_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    masked,
                    area.width,
                    label_width,
                ));
            }
            SecretsRow::RoleAddSentinel(role) => {
                let style = if selected {
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}     + Add {role} environment variable"),
                    style,
                )));
            }
            SecretsRow::SectionSpacer => {
                lines.push(Line::from(""));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Display-side breadcrumb parser for `OpRef.path`.
/// Grammar: `<Vault>/<Item>[<subtitle>?]/[<Section>/]<Field>[?<query>]`
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PathBreadcrumb {
    pub vault: String,
    pub item: String,
    pub item_subtitle: Option<String>,
    pub section: Option<String>,
    pub field: String,
    pub attribute_query: Option<String>,
}

/// Parse a snapshot breadcrumb. Returns `None` on empty input or non-3-/4-segment counts.
pub(super) fn parse_path_breadcrumb(path: &str) -> Option<PathBreadcrumb> {
    if path.is_empty() {
        return None;
    }
    // Peel off optional `?attribute=...` / `?attr=...` / `?ssh-format=...` query.
    let (path_no_q, attr) = path
        .find('?')
        .map_or((path, None), |i| (&path[..i], Some(path[i..].to_string())));
    let segs: Vec<&str> = path_no_q.split('/').collect();
    let (item, item_subtitle, vault, section, field) = match segs.as_slice() {
        [vault, item_seg, field] => {
            let (item, sub) = split_bracket_subtitle(item_seg);
            (item, sub, vault.to_string(), None, field.to_string())
        }
        [vault, item_seg, section, field] => {
            let (item, sub) = split_bracket_subtitle(item_seg);
            (
                item,
                sub,
                vault.to_string(),
                Some(section.to_string()),
                field.to_string(),
            )
        }
        _ => return None,
    };
    Some(PathBreadcrumb {
        vault,
        item,
        item_subtitle,
        section,
        field,
        attribute_query: attr,
    })
}

fn split_bracket_subtitle(s: &str) -> (String, Option<String>) {
    // rfind so an inner '[' in the subtitle is tolerated.
    if let Some(open) = s.rfind('[')
        && s.ends_with(']')
        && open < s.len() - 1
    {
        return (
            s[..open].to_string(),
            Some(s[open + 1..s.len() - 1].to_string()),
        );
    }
    (s.to_string(), None)
}

/// `OpRef` rows skip masking and render as a breadcrumb (3-segment:
/// `vault / item → field`, 4-segment adds `section`). An optional
/// `[subtitle]` annotation after the item renders in `PHOSPHOR_DIM`; an
/// optional `?attribute=...` query suffix renders in `PHOSPHOR_DIM` after
/// the field. `Plain` rows (including legacy bare `op://...` strings)
/// render as a literal / masked value with no `[op]` marker — the visual
/// migration signal that the row needs re-picking to upgrade.
fn render_secrets_key_line(
    selected: bool,
    cursor_col: &str,
    key: &str,
    value: &EnvValue,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> Line<'static> {
    const OP_MARKER: &str = "[op] ";
    const NO_MARKER: &str = "     ";
    const MASK: &str = "●●●●●●●●●●●";
    const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let dim = Style::default().fg(PHOSPHOR_DIM);

    // Variant-aware dispatch: only `OpRef` rows render with the `[op]`
    // marker and breadcrumb. `Plain` values (including legacy bare
    // `op://...` strings) render as literal / masked — the visual signal
    // that the row needs re-picking to upgrade to a pinned `OpRef`.
    let op_breadcrumb = match value {
        EnvValue::OpRef(r) => parse_path_breadcrumb(&r.path),
        EnvValue::Plain(_) => None,
    };
    let marker = if op_breadcrumb.is_some() {
        OP_MARKER
    } else {
        NO_MARKER
    };
    let mut spans = vec![
        Span::raw(cursor_col.to_string()),
        Span::styled(marker.to_string(), dim),
        Span::styled(format!("{key:label_width$}"), label_style),
        Span::raw("  "), // always at least two spaces between key and value
    ];

    // OpRef rows render as a breadcrumb regardless of `masked` — the
    // path is not the credential, so masking it makes the row a
    // less informative version of itself.
    if let Some(parts) = op_breadcrumb {
        let white_style = Style::default().fg(WHITE);
        let green = Style::default().fg(PHOSPHOR_GREEN);
        let green_bold = Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD);
        spans.push(Span::styled(parts.vault, white_style));
        spans.push(Span::styled(" / ", dim));
        spans.push(Span::styled(parts.item, green));
        if let Some(subtitle) = parts.item_subtitle {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(subtitle, dim));
        }
        if let Some(section) = parts.section {
            // 4-segment reference: the field lives inside a named
            // section of the item. Render the section between the
            // item and the field.
            spans.push(Span::styled(" / ", dim));
            spans.push(Span::styled(section, green));
        }
        spans.push(Span::styled(" \u{2192} ", dim));
        spans.push(Span::styled(parts.field, green_bold));
        if let Some(query) = parts.attribute_query {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(query, dim));
        }
        return Line::from(spans);
    }

    // Plain branch: render as masked or literal value.
    // For an OpRef whose path failed to parse (malformed / empty), show an
    // explicit re-pick placeholder rather than leaking the UUID URI.
    let plain_str = match value {
        EnvValue::Plain(s) => s.as_str(),
        EnvValue::OpRef(_) => OP_REF_REPICK_PLACEHOLDER,
    };

    let value_style = if masked {
        Style::default().fg(PHOSPHOR_DIM)
    } else if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_GREEN)
    };

    let rendered_value: String = if masked {
        MASK.to_string()
    } else {
        // Truncate with `…` when the value exceeds the remaining width.
        // Gap budget: prefix(2) + label_width + some breathing room + dirty
        // marker. Approximate with `area_width - label_width - 8`.
        let budget = (area_width as usize)
            .saturating_sub(label_width)
            .saturating_sub(8)
            .max(1);
        if plain_str.chars().count() > budget {
            let mut s: String = plain_str.chars().take(budget.saturating_sub(1)).collect();
            s.push('…');
            s
        } else {
            plain_str.to_string()
        }
    };
    spans.push(Span::styled(rendered_value, value_style));
    Line::from(spans)
}

/// Render the Auth tab.
///
/// Materializes a synthetic [`AppConfig`] from the editor's pending workspace
/// merged with the (mostly read-only) global layer of the live config so the
/// panel's `AuthPanelState::compute_for` can render with the operator's
/// in-flight edits reflected immediately.
fn render_auth_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    use crate::console::widgets::auth_panel;

    let synthesized = synthesize_appconfig_for_auth(state, config);
    let workspace_name = workspace_name_for_panel(state);
    let panel_state = auth_panel::AuthPanelState::compute_for(&synthesized, &workspace_name);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = auth_editable_row_count(state).saturating_sub(1);
    let selected = if cursor > max_idx {
        Some(max_idx)
    } else {
        Some(cursor)
    };

    auth_panel::render_with_selection(frame, area, &panel_state, selected);
}

/// Synthesize an `AppConfig` whose `[claude]/[codex]` come from the live
/// global config and whose `[workspaces.<ws>]` mirrors `editor.pending`.
/// The Auth panel reads from this so changes the operator makes via the
/// auth-edit form show up immediately, before save.
pub(in crate::console::manager) fn synthesize_appconfig_for_auth(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> AppConfig {
    let mut synthesized = AppConfig {
        claude: config.claude.clone(),
        codex: config.codex.clone(),
        env: config.env.clone(),
        roles: config.roles.clone(),
        ..AppConfig::default()
    };
    let ws_name = workspace_name_for_panel(state);
    synthesized
        .workspaces
        .insert(ws_name, state.pending.clone());
    synthesized
}

/// Resolve the workspace key used by the Auth panel. In Edit mode this is
/// the existing workspace name; in Create mode we use `pending_name` if set,
/// otherwise a stable placeholder ("(new workspace)") so the panel can still
/// render with the pending values populated.
pub(in crate::console::manager) fn workspace_name_for_panel(state: &EditorState<'_>) -> String {
    match &state.mode {
        EditorMode::Edit { name } => state.pending_name.clone().unwrap_or_else(|| name.clone()),
        EditorMode::Create => state
            .pending_name
            .clone()
            .unwrap_or_else(|| "(new workspace)".to_string()),
    }
}

/// Map a flattened editable row index (the cursor) to a concrete
/// `(scope, agent)` pair the form modal can target. The flattened layout
/// is `[workspace × Claude, workspace × Codex, role0 × Claude, role0 × Codex, ...]`.
pub(in crate::console::manager) fn resolve_auth_row_target(
    state: &EditorState<'_>,
    row: usize,
) -> Option<crate::console::manager::state::AuthFormTarget> {
    use crate::agent::Agent;
    use crate::console::manager::state::AuthFormTarget;
    let agents = [Agent::Claude, Agent::Codex];
    if row < agents.len() {
        return Some(AuthFormTarget::Workspace { agent: agents[row] });
    }
    let mut idx = agents.len();
    for role in &state.pending.allowed_roles {
        for agent in agents {
            if idx == row {
                return Some(AuthFormTarget::WorkspaceRole {
                    role: role.clone(),
                    agent,
                });
            }
            idx += 1;
        }
    }
    None
}

#[cfg(test)]
mod contextual_row_items_tests {
    //! Row-specific footer-hint composition for the editor tabs.

    use super::super::FooterItem;
    use super::contextual_row_items;
    use crate::config::{AppConfig, RoleSource};
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{MountConfig, WorkspaceConfig};

    /// Collect every `FooterItem::Text` label from a hint list.
    fn text_labels(items: &[FooterItem]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|it| {
                if let FooterItem::Text(t) = it {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Collect every `FooterItem::Key` glyph from a hint list.
    fn key_glyphs(items: &[FooterItem]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|it| {
                if let FooterItem::Key(k) = it {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Build an editor state sitting on the Mounts tab with a single mount
    /// pointing at `src`. The cursor is on row 0 (the mount we just added).
    fn editor_at_mounts_row0(src: &str) -> EditorState<'static> {
        let ws = WorkspaceConfig {
            mounts: vec![MountConfig {
                src: src.to_string(),
                dst: src.to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    #[test]
    fn github_mount_row_includes_open_in_github_hint() {
        // Build a synthetic GitHub repo on-disk so `mount_info::inspect`
        // classifies the source as `MountKind::Git { origin: Some(GitOrigin::Github { .. }) }`.
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git_dir.join("config"),
            r#"[remote "origin"]
    url = git@github.com:owner/repo.git
"#,
        )
        .unwrap();

        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
        let keys = key_glyphs(&hint);
        let labels = text_labels(&hint);
        assert!(
            keys.contains(&"O"),
            "GitHub mount row must include `O` key hint; got keys={keys:?}"
        );
        assert!(
            labels.contains(&"open in GitHub"),
            "GitHub mount row must include `open in GitHub` label; got labels={labels:?}"
        );
        // Composes with the existing D/A pair, so all three keys are present.
        assert!(keys.contains(&"D"));
        assert!(keys.contains(&"A"));
    }

    #[test]
    fn non_github_mount_row_omits_open_in_github_hint() {
        // Plain folder (no .git) — no GitHub URL, so `O` must not appear.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
        let keys = key_glyphs(&hint);
        assert!(
            !keys.contains(&"O"),
            "plain-folder mount must not include `O`; got keys={keys:?}"
        );
        // But the existing D/A hints must still be present.
        assert!(keys.contains(&"D"));
        assert!(keys.contains(&"A"));
    }

    #[test]
    fn mount_row_includes_toggle_readonly_hint() {
        // Every mount-data row must surface `R toggle ro/rw`, regardless of
        // whether the row is a GitHub repo. Plain-folder case — confirms the
        // hint composes alongside D/A even without the O extension.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
        let keys = key_glyphs(&hint);
        let labels = text_labels(&hint);
        assert!(
            keys.contains(&"R"),
            "mount data row must include `R` key hint; got keys={keys:?}"
        );
        assert!(
            labels.contains(&"toggle ro/rw"),
            "mount data row must include `toggle ro/rw` label; got labels={labels:?}"
        );
    }

    #[test]
    fn mounts_sentinel_row_omits_toggle_readonly_hint() {
        // The `+ Add mount` sentinel has nothing to toggle — R must not
        // appear on that row's footer. Confirms the hint is cursor-aware.
        let tmp = tempfile::tempdir().unwrap();
        let mut editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        editor.active_field = FieldFocus::Row(editor.pending.mounts.len());
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
        let keys = key_glyphs(&hint);
        assert!(
            !keys.contains(&"R"),
            "sentinel row must not advertise R; got keys={keys:?}"
        );
    }

    /// Guard that every footer hint built by `contextual_row_items` exposes
    /// single-letter hotkeys in uppercase. Multi-character glyphs (Enter,
    /// Tab, Esc, arrows, `*`) pass through unchanged.
    #[test]
    fn footer_hotkeys_are_uppercase() {
        // A representative spread: Mounts (data row + sentinel) + Roles.
        // General row 0 Edit + Create uses only `Enter`, which is multi-char.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let config = config_with_agents(&["agent-smith"]);

        // Mounts data-row hint.
        let mounts_row = contextual_row_items(&editor, &config, true);
        assert_hint_hotkeys_uppercase(&mounts_row, "Mounts row 0");

        // Mounts sentinel "+ Add mount" row.
        let mut sentinel_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        sentinel_editor.active_field = FieldFocus::Row(sentinel_editor.pending.mounts.len());
        let sentinel_row = contextual_row_items(&sentinel_editor, &config, true);
        assert_hint_hotkeys_uppercase(&sentinel_row, "Mounts sentinel");

        // Roles tab uses Space + `*` — both multi-char / non-alpha.
        let mut roles_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        roles_editor.active_tab = EditorTab::Roles;
        let roles_row = contextual_row_items(&roles_editor, &config, true);
        assert_hint_hotkeys_uppercase(&roles_row, "Roles");
    }

    /// Scan a footer-hint list and assert every single-character `Key`
    /// alphabetic glyph is uppercase. Multi-character glyphs (Enter, Tab,
    /// Esc, arrows, etc.) and non-alpha keys (`*`) pass through.
    fn assert_hint_hotkeys_uppercase(hint: &[FooterItem], context: &str) {
        for item in hint {
            if let FooterItem::Key(k) = item {
                let chars: Vec<char> = k.chars().collect();
                if chars.len() == 1 {
                    let c = chars[0];
                    if c.is_alphabetic() {
                        assert!(
                            c.is_uppercase(),
                            "[{context}] single-letter hotkey must be uppercase; got {k:?}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod agents_tab_render_tests {
    //! Pins `[x]`/`[ ]` to the *effectively allowed* state — empty
    //! `allowed_roles` is the "all allowed" shorthand.
    use super::render_roles_tab;
    use crate::config::{AppConfig, RoleSource};
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
        WorkspaceConfig {
            allowed_roles: names.iter().map(|s| (*s).into()).collect(),
            ..WorkspaceConfig::default()
        }
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    fn render_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Roles;
        editor.active_field = FieldFocus::Row(0);
        let backend = TestBackend::new(60, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_roles_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        // Collapse the buffer to newline-delimited rows so the test
        // assertion can match per-row semantics ("row N contains `[x]`").
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
    fn in_all_mode_all_rows_render_as_checked() {
        // Empty `allowed_roles` ⇒ "all" mode ⇒ every row is `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&[]);
        let dump = render_to_dump(ws, &cfg);

        // Every role name should appear on a line that also carries `[x]`.
        for name in ["alpha", "beta", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[x]"),
                "in 'all' mode role `{name}` row must render `[x]`; got `{line}`"
            );
            assert!(
                !line.contains("[ ]"),
                "in 'all' mode role `{name}` must not render `[ ]`; got `{line}`"
            );
        }
    }

    /// The default-role row carries the `★` marker; non-default rows
    /// render a plain space in the marker column. Pins the glyph that
    /// the `*` keybinding produces in the rendered list.
    #[test]
    fn default_agent_row_carries_star_marker() {
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = ws_with_allowed(&[]);
        ws.default_role = Some("beta".into());
        let dump = render_to_dump(ws, &cfg);

        let beta_line = dump
            .lines()
            .find(|l| l.contains("beta"))
            .expect("beta must render");
        assert!(
            beta_line.contains('\u{2605}'),
            "default role row must carry the `★` marker; got `{beta_line}`"
        );

        let alpha_line = dump
            .lines()
            .find(|l| l.contains("alpha"))
            .expect("alpha must render");
        assert!(
            !alpha_line.contains('\u{2605}'),
            "non-default rows must not carry `★`; got `{alpha_line}`"
        );
    }

    #[test]
    fn in_custom_mode_only_listed_agents_show_checked() {
        // Non-empty list ⇒ "custom" mode ⇒ only listed rows are `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&["beta"]);
        let dump = render_to_dump(ws, &cfg);

        let beta_line = dump
            .lines()
            .find(|l| l.contains("beta"))
            .expect("beta must render");
        assert!(
            beta_line.contains("[x]"),
            "listed role `beta` must render `[x]`; got `{beta_line}`"
        );

        for name in ["alpha", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[ ]"),
                "unlisted role `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
            );
        }
    }
}

#[cfg(test)]
mod secrets_tab_render_tests {
    //! Render-buffer tests for the Secrets tab. Verifies the masking
    //! default, the unmasked literal-value path, and that the flat-row
    //! builder honours `secrets_expanded` for per-role override sections.
    use super::render_secrets_tab;
    use crate::config::AppConfig;
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus, SecretsScopeTag};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    /// Build an editor sitting on the Secrets tab with a single
    /// workspace-level env key (`DB_URL = postgres://localhost/db`).
    fn editor_with_workspace_env() -> EditorState<'static> {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "postgres://localhost/db".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    /// Build an editor sitting on the Secrets tab with one role override
    /// carrying a single env key (`agent-smith`: `LOG_LEVEL = debug`).
    fn editor_with_agent_override() -> EditorState<'static> {
        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("LOG_LEVEL".into(), "debug".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
            },
        );
        let ws = WorkspaceConfig {
            roles,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    /// Render the Secrets tab to a 80x15 `TestBackend`, return the raw
    /// buffer as newline-delimited rows so tests can search for glyphs.
    fn render_to_dump(editor: &EditorState<'_>) -> String {
        let config = AppConfig::default();
        let backend = TestBackend::new(80, 15);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 80, 15), editor, &config);
        })
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
    fn secrets_tab_defaults_to_masked() {
        // `new_edit` leaves `unmasked_rows` empty, so every plain-text
        // value renders masked by default.
        let editor = editor_with_workspace_env();
        assert!(
            editor.unmasked_rows.is_empty(),
            "new_edit must leave unmasked_rows empty (default = all masked)"
        );
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("●●●●●●●●●●●"),
            "masked-default render must show the mask glyph; got:\n{dump}"
        );
        assert!(
            !dump.contains("postgres://localhost/db"),
            "masked-default render must hide the literal value; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_unmasked_shows_literal_value() {
        let mut editor = editor_with_workspace_env();
        editor
            .unmasked_rows
            .insert((SecretsScopeTag::Workspace, "DB_URL".into()));
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("postgres://localhost/db"),
            "unmasked render must show literal value; got:\n{dump}"
        );
        assert!(
            !dump.contains("●●●●●●●●●●●"),
            "unmasked render must not show the mask glyph; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_collapsed_agent_omits_key_rows() {
        // `secrets_expanded` is empty by default (set by `new_edit`), so
        // the role section header renders but its `LOG_LEVEL` key row
        // does not.
        let editor = editor_with_agent_override();
        assert!(editor.secrets_expanded.is_empty());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "role header must render; got:\n{dump}"
        );
        assert!(
            !dump.contains("LOG_LEVEL"),
            "collapsed role section must omit key rows; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_expanded_agent_shows_key_rows() {
        let mut editor = editor_with_agent_override();
        editor.secrets_expanded.insert("agent-smith".into());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "role header must still render when expanded; got:\n{dump}"
        );
        assert!(
            dump.contains("LOG_LEVEL"),
            "expanded role section must show its key rows; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_cursor_skips_workspace_header_label() {
        let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            !rows.is_empty(),
            "secrets_flat_rows must always include at least the WorkspaceAddSentinel"
        );
        assert!(
            matches!(rows.first(), Some(super::SecretsRow::WorkspaceAddSentinel)),
            "row 0 must be the focusable `+ Add` sentinel, not a header; got {:?}",
            rows.first()
        );
        assert!(
            matches!(editor.active_field, FieldFocus::Row(0)),
            "editor must open on row 0 = sentinel"
        );
    }

    /// Pins the exact flat-row sequence for a workspace with env vars,
    /// one expanded role (with keys), and one collapsed role. Cursor
    /// arithmetic in `input/editor.rs` is derived directly from this
    /// sequence, so a wrong order causes silent wrong-row selections.
    #[test]
    fn secrets_flat_rows_sequence_is_canonical() {
        use crate::workspace::WorkspaceRoleOverride;

        let mut env = std::collections::BTreeMap::new();
        env.insert("ALPHA".into(), "1".into());
        env.insert("BETA".into(), "2".into());

        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("KEY".into(), "v".into());

        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-a".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
            },
        );
        roles.insert(
            "agent-b".into(),
            WorkspaceRoleOverride {
                env: std::collections::BTreeMap::new(),
                claude: None,
                codex: None,
            },
        );

        let ws = WorkspaceConfig {
            env,
            roles,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        // Expand agent-a, leave agent-b collapsed.
        editor.secrets_expanded.insert("agent-a".into());

        let rows = super::secrets_flat_rows(&editor);
        // Expected sequence:
        //  0  WorkspaceKeyRow("ALPHA")
        //  1  WorkspaceKeyRow("BETA")
        //  2  WorkspaceAddSentinel
        //  3  SectionSpacer
        //  4  AgentHeader { role: "agent-a", expanded: true }
        //  5  AgentKeyRow { role: "agent-a", key: "KEY" }
        //  6  AgentAddSentinel("agent-a")
        //  7  SectionSpacer
        //  8  AgentHeader { role: "agent-b", expanded: false }
        assert_eq!(rows.len(), 9, "unexpected row count: {rows:?}");
        assert!(matches!(&rows[0], super::SecretsRow::WorkspaceKeyRow(k) if k == "ALPHA"));
        assert!(matches!(&rows[1], super::SecretsRow::WorkspaceKeyRow(k) if k == "BETA"));
        assert!(matches!(&rows[2], super::SecretsRow::WorkspaceAddSentinel));
        assert!(matches!(&rows[3], super::SecretsRow::SectionSpacer));
        assert!(
            matches!(&rows[4], super::SecretsRow::RoleHeader { role, expanded: true } if role == "agent-a")
        );
        assert!(
            matches!(&rows[5], super::SecretsRow::RoleKeyRow { role, key } if role == "agent-a" && key == "KEY")
        );
        assert!(matches!(&rows[6], super::SecretsRow::RoleAddSentinel(a) if a == "agent-a"));
        assert!(matches!(&rows[7], super::SecretsRow::SectionSpacer));
        assert!(
            matches!(&rows[8], super::SecretsRow::RoleHeader { role, expanded: false } if role == "agent-b")
        );
    }

    #[test]
    fn secrets_tab_empty_renders_only_sentinel() {
        let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        let dump = render_to_dump(&editor);

        assert!(
            dump.contains("+ Add environment variable"),
            "the `+ Add environment variable` sentinel must render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("Workspace env"),
            "the `Workspace env` preamble label must NOT render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("(no env vars)"),
            "the `(no env vars)` placeholder must NOT render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("env var"),
            "TUI text must say `environment variable`, not `env var`; dump:\n{dump}"
        );
    }

    #[test]
    fn op_row_breadcrumb_render_three_segment() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("Work"),
            "breadcrumb must render vault segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("db"),
            "breadcrumb must render item segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("password"),
            "breadcrumb must render field segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("\u{2192}"),
            "breadcrumb must include the → glyph between item and field; dump:\n{dump}"
        );
        assert!(
            !dump.contains("op://"),
            "op:// scheme prefix must not appear in the breadcrumb; dump:\n{dump}"
        );
        // Mask glyph must not appear on OpRef rows even though
        // editor defaults to all-masked.
        assert!(
            editor.unmasked_rows.is_empty(),
            "default state is all-masked; OpRef rows must still bypass masking"
        );
        assert!(
            !dump.contains("●●●"),
            "OpRef rows must never render the mask glyph; dump:\n{dump}"
        );
    }

    /// 4-segment is `vault/item/section/field` per the 1Password CLI
    /// syntax — not the earlier `account/vault/item/field` reading.
    #[test]
    fn op_row_breadcrumb_render_four_segment_with_section() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "API_KEY".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Personal/API Keys/auth/secret_key".into(),
                path: "Personal/API Keys/auth/secret_key".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        // All four components must appear, in order, with the arrow
        // glyph between the section and the field.
        assert!(
            dump.contains("Personal"),
            "vault must render; dump:\n{dump}"
        );
        assert!(dump.contains("API Keys"), "item must render; dump:\n{dump}");
        assert!(
            dump.contains("auth"),
            "section must render between item and field; dump:\n{dump}"
        );
        assert!(
            dump.contains("secret_key"),
            "field must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("\u{2192}"),
            "arrow glyph must precede the field; dump:\n{dump}"
        );
        // The account-prefix branch is dead — no email-style rendering
        // for 4-segment refs.
        assert!(
            !dump.contains('@'),
            "4-segment refs must not render an account email prefix; dump:\n{dump}"
        );
    }

    /// Text marker (not glyph) — `⚿` rendered inconsistently across
    /// terminals; `[op]` reads as "1Password" at a glance.
    #[test]
    fn op_row_renders_with_op_text_marker() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("[op]"),
            "OpRef row must render the `[op]` text marker; dump:\n{dump}"
        );
        assert!(
            !dump.contains("\u{26BF}"),
            "the legacy `⚿` glyph must not appear after the marker swap; dump:\n{dump}"
        );
    }

    #[test]
    fn plain_row_renders_without_op_marker() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DEBUG".into(), "1".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            !dump.contains("[op]"),
            "plain-text row must not render the `[op]` marker; dump:\n{dump}"
        );
    }

    #[test]
    fn op_row_marker_column_is_5_chars_wide_with_brackets() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("[op] "),
            "OpRef row must render the marker as exactly `[op] ` (5 chars \
             including trailing space); dump:\n{dump}"
        );
    }

    #[test]
    fn plain_row_marker_column_is_5_blank_chars_for_alignment() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DEBUG".into(), "1".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // 7-char prefix region = cursor (1..3) + marker (3..8); on
        // a plain row, cells 3..8 are all blanks.
        let backend = TestBackend::new(80, 15);
        let mut term = Terminal::new(backend).unwrap();
        let config = AppConfig::default();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 80, 15), &editor, &config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut cells = String::new();
        for x in 3..8 {
            cells.push_str(buf[(x, 1)].symbol());
        }
        assert_eq!(
            cells, "     ",
            "plain row marker column (cells 3..8 of row 1) must be 5 \
             blank spaces for alignment; got {cells:?}"
        );
    }

    #[test]
    fn secrets_tab_renders_keys_in_alphabetical_order() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("ZULU".into(), "z".into());
        env.insert("ALPHA".into(), "a".into());
        env.insert("MIKE".into(), "m".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        let alpha = dump.find("ALPHA").expect("ALPHA must appear");
        let mike = dump.find("MIKE").expect("MIKE must appear");
        let zulu = dump.find("ZULU").expect("ZULU must appear");
        assert!(
            alpha < mike && mike < zulu,
            "keys must render alphabetically (ALPHA < MIKE < ZULU); offsets {alpha}/{mike}/{zulu}\n{dump}"
        );
    }

    #[test]
    fn section_spacer_appears_between_workspace_and_first_agent_section() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "postgres://localhost/db".into());
        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("LOG_LEVEL".into(), "debug".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
            },
        );
        let ws = WorkspaceConfig {
            env,
            roles,
            ..WorkspaceConfig::default()
        };
        let editor = EditorState::new_edit("ws".into(), ws);
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            matches!(rows.get(2), Some(super::SecretsRow::SectionSpacer)),
            "row 2 must be a SectionSpacer between workspace section \
             and first role header; got {:?}",
            rows.get(2)
        );
        assert!(
            matches!(rows.get(3), Some(super::SecretsRow::RoleHeader { .. })),
            "row 3 must be the role header right after the spacer; \
             got {:?}",
            rows.get(3)
        );
    }

    #[test]
    fn section_spacer_appears_between_consecutive_agent_sections() {
        let mut a_env = std::collections::BTreeMap::new();
        a_env.insert("LEVEL_A".into(), "1".into());
        let mut b_env = std::collections::BTreeMap::new();
        b_env.insert("LEVEL_B".into(), "2".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-architect".into(),
            WorkspaceRoleOverride {
                env: a_env,
                claude: None,
                codex: None,
            },
        );
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: b_env,
                claude: None,
                codex: None,
            },
        );
        let ws = WorkspaceConfig {
            roles,
            ..WorkspaceConfig::default()
        };
        let editor = EditorState::new_edit("ws".into(), ws);
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            matches!(rows.get(1), Some(super::SecretsRow::SectionSpacer)),
            "spacer expected before the first role header; rows={rows:?}"
        );
        assert!(
            matches!(rows.get(3), Some(super::SecretsRow::SectionSpacer)),
            "spacer expected between consecutive role sections; rows={rows:?}"
        );
        assert!(
            !matches!(rows.last(), Some(super::SecretsRow::SectionSpacer)),
            "no trailing spacer after the final section; rows={rows:?}"
        );
    }

    /// Helper that renders the Secrets tab to a wider (120-column) terminal
    /// so long breadcrumbs (subtitle + section + field) are not truncated.
    fn render_to_dump_wide(editor: &EditorState<'_>) -> String {
        let config = AppConfig::default();
        let backend = TestBackend::new(120, 15);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 120, 15), editor, &config);
        })
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

    /// `OpRef` whose `path` contains the `[subtitle]` disambiguation form.
    /// The subtitle must appear in the rendered output between the item
    /// name and the next " / " separator.
    #[test]
    fn renderer_op_ref_with_subtitle_renders_text() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "Private/Claude[alexey@zhokhov.com]/security/auth token".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so the subtitle and field are not truncated.
        let dump = render_to_dump_wide(&editor);
        // The row must carry the [op] marker (OpRef variant).
        assert!(
            dump.contains("[op]"),
            "OpRef row with subtitle must render `[op]` marker; dump:\n{dump}"
        );
        // Subtitle text must appear in the rendered output.
        assert!(
            dump.contains("alexey@zhokhov.com"),
            "subtitle text must appear in the breadcrumb; dump:\n{dump}"
        );
        // Vault, item, section, and field must all render.
        assert!(dump.contains("Private"), "vault must render; dump:\n{dump}");
        assert!(
            dump.contains("Claude"),
            "item name must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("security"),
            "section must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("auth token"),
            "field must render; dump:\n{dump}"
        );
    }

    /// `OpRef` whose `path` carries an `?attribute=otp` query suffix. The
    /// query must appear in the rendered output after the field name.
    #[test]
    fn renderer_op_ref_with_attribute_query_renders_text() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "OTP".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld?attribute=otp".into(),
                path: "Private/GitHub/one-time password?attribute=otp".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so `?attribute=otp` is not truncated.
        let dump = render_to_dump_wide(&editor);
        // The row must carry the [op] marker.
        assert!(
            dump.contains("[op]"),
            "OpRef row with attribute query must render `[op]` marker; dump:\n{dump}"
        );
        // The query suffix must appear in the output.
        assert!(
            dump.contains("?attribute=otp"),
            "attribute query must appear in breadcrumb; dump:\n{dump}"
        );
        // Field name must also render.
        assert!(
            dump.contains("one-time password"),
            "field must render; dump:\n{dump}"
        );
    }

    /// `OpRef` with BOTH a subtitle disambiguation AND an `?attribute=otp`
    /// query suffix. Asserts that all six visible pieces appear in the
    /// expected left-to-right order: vault → item → subtitle → section →
    /// field → query.
    #[test]
    fn renderer_op_ref_with_subtitle_section_and_query_renders_all() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/sec/fld?attribute=otp".into(),
                path: "Private/Claude[alexey@zhokhov.com]/security/auth token?attribute=otp".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so no piece is truncated.
        let dump = render_to_dump_wide(&editor);

        // All visible pieces must appear in order:
        // vault → item → subtitle → section → field → query.
        let v_pos = dump.find("Private").expect("vault present");
        let i_pos = dump.find("Claude").expect("item present");
        let s_pos = dump.find("alexey@zhokhov.com").expect("subtitle present");
        let sec_pos = dump.find("security").expect("section present");
        let f_pos = dump.find("auth token").expect("field present");
        let q_pos = dump.find("?attribute=otp").expect("query present");
        assert!(v_pos < i_pos, "vault before item");
        assert!(i_pos < s_pos, "item before subtitle");
        assert!(s_pos < sec_pos, "subtitle before section");
        assert!(sec_pos < f_pos, "section before field");
        assert!(f_pos < q_pos, "field before query");
    }

    /// A `Plain` row containing a bare `op://...` string gets NO `[op]`
    /// marker — it renders as a literal masked value, the visual signal
    /// that the operator needs to re-pick it.
    #[test]
    fn renderer_plain_with_bare_op_uri_renders_as_literal_no_breadcrumb() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "op://Vault/Item/Field".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        // Plain rows carrying a legacy op:// string must NOT render the
        // [op] marker — the visual distinction signals the need to re-pick.
        assert!(
            !dump.contains("[op]"),
            "Plain rows must NOT carry [op] marker; dump:\n{dump}"
        );
        // The breadcrumb separators must not appear — this is a plain
        // masked/literal row, not a breadcrumb render.
        assert!(
            !dump.contains(" / Vault / "),
            "Plain op:// strings must not render vault breadcrumb; dump:\n{dump}"
        );
        // The mask glyph must appear (plain row, masked by default).
        assert!(
            dump.contains("●●●"),
            "Plain row must render masked by default; dump:\n{dump}"
        );
    }

    /// Single env var → `label_width` equals key length. Without the explicit
    /// two-space span, the screenshot bug (`CLAUDE_CODE_OAUTH_TOKENPrivate` / ...)
    /// recurs.
    #[test]
    fn renderer_key_value_separator_always_at_least_two_spaces() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "Private/Claude/security/auth token".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so the breadcrumb is not truncated.
        let dump = render_to_dump_wide(&editor);
        assert!(
            dump.contains("CLAUDE_CODE_OAUTH_TOKEN  Private"),
            "expected at least 2 spaces between key and breadcrumb; dump:\n{dump}"
        );
        assert!(
            !dump.contains("CLAUDE_CODE_OAUTH_TOKENPrivate"),
            "no space is the bug; dump:\n{dump}"
        );
    }

    /// `OpRef` whose `path` doesn't parse as a 3- or 4-segment breadcrumb.
    /// The renderer must NOT panic; it shows a re-pick placeholder in the
    /// value column without the `[op]` marker, and must NOT leak the UUID URI.
    #[test]
    fn renderer_op_ref_with_malformed_path_renders_repick_placeholder_no_panic() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "garbage-no-slashes".into(),
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        // Unmask so the placeholder is rendered as text rather than ●●●.
        editor
            .unmasked_rows
            .insert((SecretsScopeTag::Workspace, "TOKEN".into()));

        let dump = render_to_dump_wide(&editor);
        // Malformed path → parse_path_breadcrumb returns None → no [op] marker.
        assert!(!dump.contains("[op]"), "no [op] marker; dump:\n{dump}");
        // Re-pick placeholder must be shown instead of the UUID URI.
        assert!(
            dump.contains("<unparseable path \u{2014} re-pick>"),
            "expected re-pick placeholder; dump:\n{dump}"
        );
        // UUID URI must NOT be visible to the operator.
        assert!(
            !dump.contains("op://abc/def/fld"),
            "UUID URI must NOT leak; dump:\n{dump}"
        );
    }
}

#[cfg(test)]
mod eligible_agents_for_override_tests {
    //! Roles already carrying an override are NOT filtered — the
    //! picker can add more keys to an existing override.
    use super::eligible_agents_for_override;
    use crate::config::{AppConfig, RoleSource};
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    fn ws_with_overrides(allowed: &[&str], override_agents: &[&str]) -> WorkspaceConfig {
        let mut roles = std::collections::BTreeMap::new();
        for a in override_agents {
            let mut env = std::collections::BTreeMap::new();
            env.insert("LOG_LEVEL".into(), "debug".into());
            roles.insert(
                (*a).into(),
                WorkspaceRoleOverride {
                    env,
                    claude: None,
                    codex: None,
                },
            );
        }
        WorkspaceConfig {
            allowed_roles: allowed.iter().map(|s| (*s).into()).collect(),
            roles,
            ..WorkspaceConfig::default()
        }
    }

    fn editor_for(ws: WorkspaceConfig) -> EditorState<'static> {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    #[test]
    fn eligible_agents_returns_allowed_when_list_non_empty() {
        // Non-empty `allowed_roles` is taken at face value — the
        // result matches the workspace's allowed list verbatim.
        let cfg = config_with_agents(&["agent-smith", "agent-brown", "agent-architect"]);
        let editor = editor_for(ws_with_overrides(&["agent-smith"], &[]));
        let eligible = eligible_agents_for_override(&editor, &cfg);
        assert_eq!(eligible, vec!["agent-smith".to_string()]);
    }

    #[test]
    fn eligible_agents_returns_all_registered_when_allowed_empty() {
        // Empty `allowed_roles` is the "all roles allowed" shorthand —
        // every globally-registered role is eligible.
        let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
        let editor = editor_for(ws_with_overrides(&[], &[]));
        let mut eligible = eligible_agents_for_override(&editor, &cfg);
        eligible.sort();
        assert_eq!(
            eligible,
            vec!["agent-brown".to_string(), "agent-smith".to_string()]
        );
    }

    #[test]
    fn eligible_agents_does_not_filter_by_existing_overrides() {
        // Operators may want to add additional keys to an role that
        // already carries some — the picker must therefore include
        // every allowed role regardless of whether `pending.roles`
        // already lists them.
        let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
        let editor = editor_for(ws_with_overrides(
            &["agent-smith", "agent-brown"],
            &["agent-smith"],
        ));
        let mut eligible = eligible_agents_for_override(&editor, &cfg);
        eligible.sort();
        assert_eq!(
            eligible,
            vec!["agent-brown".to_string(), "agent-smith".to_string()],
            "agent-smith already has overrides but must still appear so the operator can add another key to it"
        );
    }

    #[test]
    fn eligible_agents_returns_empty_when_no_allowed_and_no_registered() {
        // Empty `allowed_roles` shorthand AND no registered roles:
        // the picker would be empty, so the caller is expected to
        // short-circuit and not open the modal.
        let cfg = config_with_agents(&[]);
        let editor = editor_for(ws_with_overrides(&[], &[]));
        let eligible = eligible_agents_for_override(&editor, &cfg);
        assert!(eligible.is_empty());
    }
}

#[cfg(test)]
mod parse_path_breadcrumb_tests {
    use super::parse_path_breadcrumb;

    #[test]
    fn parse_path_breadcrumb_3_segment_no_subtitle() {
        let p = parse_path_breadcrumb("Private/Stripe/api key").unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Stripe");
        assert!(p.item_subtitle.is_none());
        assert!(p.section.is_none());
        assert_eq!(p.field, "api key");
        assert!(p.attribute_query.is_none());
    }

    #[test]
    fn parse_path_breadcrumb_3_segment_with_subtitle() {
        let p = parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/auth").unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Claude");
        assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
        assert!(p.section.is_none());
        assert_eq!(p.field, "auth");
    }

    #[test]
    fn parse_path_breadcrumb_4_segment_with_subtitle() {
        let p = parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/security/auth token")
            .unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Claude");
        assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
        assert_eq!(p.section.as_deref(), Some("security"));
        assert_eq!(p.field, "auth token");
    }

    #[test]
    fn parse_path_breadcrumb_with_attribute_query() {
        let p = parse_path_breadcrumb("Private/GitHub/one-time password?attribute=otp").unwrap();
        assert_eq!(p.field, "one-time password");
        assert_eq!(p.attribute_query.as_deref(), Some("?attribute=otp"));
    }

    #[test]
    fn parse_path_breadcrumb_subtitle_containing_brackets() {
        // rfind('[') means the last [...] is the subtitle.
        let p = parse_path_breadcrumb("Private/Claude[has [bracket]]/auth").unwrap();
        assert_eq!(p.item, "Claude[has ");
        assert_eq!(p.item_subtitle.as_deref(), Some("bracket]"));
    }

    #[test]
    fn parse_path_breadcrumb_invalid_too_few_segments() {
        assert!(parse_path_breadcrumb("Private/Item").is_none());
        assert!(parse_path_breadcrumb("Private").is_none());
        assert!(parse_path_breadcrumb("").is_none());
    }

    #[test]
    fn parse_path_breadcrumb_invalid_too_many_segments() {
        // 5+ segments is not a valid 1Password breadcrumb.
        assert!(parse_path_breadcrumb("a/b/c/d/e").is_none());
    }
}
