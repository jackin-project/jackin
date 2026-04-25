//! Editor-stage rendering: full-screen editor with header, tab bar,
//! per-tab body renderers (General / Mounts / Agents / Secrets), and the
//! contextual footer composition that varies with the active tab + cursor.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::state::{EditorMode, EditorState, EditorTab, FieldFocus};
use super::list::{MOUNT_MODE_COL_WIDTH, format_mount_rows, mount_path_width, render_mount_header};
use super::{
    FooterItem, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_footer, render_header,
};
use crate::config::AppConfig;

// ── Editor stage ────────────────────────────────────────────────────

pub fn render_editor(frame: &mut Frame, state: &EditorState<'_>, config: &AppConfig) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(2), // tab strip
            Constraint::Min(8),    // tab body
            Constraint::Length(2), // footer
        ])
        .split(area);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit · {name}"),
        EditorMode::Create => "new workspace".to_string(),
    };
    render_header(frame, chunks[0], &title);

    render_tab_strip(frame, chunks[1], state.active_tab);

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, chunks[2], state),
        EditorTab::Mounts => render_mounts_tab(frame, chunks[2], state),
        EditorTab::Agents => render_agents_tab(frame, chunks[2], state, config),
        EditorTab::Secrets => render_secrets_tab(frame, chunks[2], state, config),
    }

    // Contextual footer: row-specific hints + base stage hints.
    let mut items: Vec<FooterItem> = Vec::new();

    // Row-specific group (may be empty).
    let row_items = contextual_row_items(state);
    if !row_items.is_empty() {
        items.extend(row_items);
        items.push(FooterItem::GroupSep);
    }

    // Save group — label varies with dirty/clean.
    items.push(FooterItem::Key("S"));
    items.push(FooterItem::Text("save workspace"));
    if state.is_dirty() {
        items.push(FooterItem::Dyn(format!(
            "({} changes)",
            state.change_count()
        )));
    }

    // Tab-for-next-tab and ↑↓-for-cursor-move are universal across every
    // editor tab — they don't need to be advertised in the base footer.

    // Exit group — discard if dirty, back if clean.
    items.push(FooterItem::GroupSep);
    items.push(FooterItem::Key("Esc"));
    if state.is_dirty() {
        items.push(FooterItem::Text("discard"));
    } else {
        items.push(FooterItem::Text("back"));
    }

    render_footer(frame, chunks[3], &items);

    // Error banner overlay — top line of the body. Only rendered when
    // `save_flow` is in the `Error` state AND no ErrorPopup modal is up
    // (the popup is the commit-time error surface; the banner is the
    // pre-commit validation surface — they share the `Error` variant but
    // present differently).
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
fn contextual_row_items(state: &EditorState<'_>) -> Vec<FooterItem> {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => {
            // Row indices are uniform across both modes for Enter-affordance
            // purposes. Create mode has rows 0-1; Edit mode also has 2-3
            // (default agent, last used) which are read-only and surface no
            // Enter action either way.
            //   row 0 = Name        (editable in both modes — Enter opens rename)
            //   row 1 = Working dir (editable — Enter opens workdir picker)
            match cursor {
                0 => vec![FooterItem::Key("Enter"), FooterItem::Text("rename")],
                1 => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("pick working directory"),
                ],
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
                            host: super::super::mount_info::GitHost::Github,
                            web_url: Some(_),
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
                items
            } else {
                // Sentinel "+ Add mount" row — both Enter and A invoke the
                // same add-mount flow, so render as a single combined key.
                vec![FooterItem::Key("Enter/A"), FooterItem::Text("add")]
            }
        }
        EditorTab::Agents => vec![
            FooterItem::Key("Space"),
            FooterItem::Text("toggle"),
            FooterItem::Sep,
            FooterItem::Key("D"),
            FooterItem::Text("default"),
        ],
        EditorTab::Secrets => {
            // Row-specific hints depend on which SecretsRow kind the cursor
            // is sitting on. `M` is universal across the tab.
            let rows = secrets_flat_rows(state);
            match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::AgentKeyRow { .. }) => vec![
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
                    FooterItem::Sep,
                    FooterItem::Key("P"),
                    FooterItem::Text("1Password"),
                ],
                Some(SecretsRow::WorkspaceHeader | SecretsRow::AgentHeader { .. }) => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("expand"),
                    FooterItem::Sep,
                    FooterItem::Key("←/→"),
                    FooterItem::Text("collapse/expand"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                    FooterItem::Sep,
                    FooterItem::Key("M"),
                    FooterItem::Text("mask/unmask"),
                ],
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::AgentAddSentinel(_)) => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("add"),
                    FooterItem::Sep,
                    FooterItem::Key("M"),
                    FooterItem::Text("mask/unmask"),
                    FooterItem::Sep,
                    FooterItem::Key("P"),
                    FooterItem::Text("1Password"),
                ],
                None => vec![FooterItem::Key("M"), FooterItem::Text("mask/unmask")],
            }
        }
    }
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Agents, "Agents"),
        (EditorTab::Secrets, "Secrets"),
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

    let is_edit = matches!(&state.mode, EditorMode::Edit { .. });

    let name_dirty = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().is_some_and(|n| n != name),
        EditorMode::Create => false,
    };
    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };

    // In Create mode the row numbering is:
    //   0 = name (editable — Enter opens rename TextInput, pre-filled from prelude)
    //   1 = workdir
    // In Edit mode:
    //   0 = name (editable), 1 = workdir, 2 = default agent (ro), 3 = last used (ro)
    let mut rows: Vec<Line> = Vec::new();

    if is_edit {
        // Edit mode: name is an editable row at index 0.
        rows.push(render_editor_row(0, cursor, "Name", name_value, name_dirty));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "Working dir",
            &workdir_display,
            state.pending.workdir != state.original.workdir,
        ));
        // Default agent — read-only here; set via Agents tab.
        rows.push(render_editor_readonly_row(
            2,
            cursor,
            "Default agent",
            state.pending.default_agent.as_deref().unwrap_or("(none)"),
        ));
        // Last used — read-only.
        rows.push(render_editor_readonly_row(
            3,
            cursor,
            "Last used",
            state.original.last_agent.as_deref().unwrap_or("(none)"),
        ));
    } else {
        // Create mode: name is editable (Enter opens the rename TextInput)
        // but we don't show an `● unsaved` marker because there's no
        // "original" workspace to diff against — the save_count already
        // tracks field-level changes.
        rows.push(render_editor_row(0, cursor, "Name", name_value, false));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "Working dir",
            &workdir_display,
            false,
        ));
        // Hide "Default agent" and "Last used" in Create mode — they have no meaning yet.
    }

    frame.render_widget(Paragraph::new(rows).block(block), area);
}

/// Render a field row with cursor highlight when `row == cursor`.
fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    dirty: bool,
) -> Line<'static> {
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
    if dirty {
        spans.push(Span::styled(
            "    ● unsaved",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn render_editor_readonly_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
) -> Line<'static> {
    let selected = row == cursor;
    let prefix = if selected { "▸ " } else { "  " };
    // Read-only rows: label stays white (bold when focused) like editable
    // rows; value + `(read-only)` suffix render in dim phosphor so the
    // operator can visually skim editable vs fixed fields.
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label:15}"), label_style),
        Span::styled(value.to_string(), Style::default().fg(PHOSPHOR_DIM)),
        Span::styled(
            " (read-only)",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        ),
    ])
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

    lines.extend(rows.iter().enumerate().map(|(i, (path, mode, kind))| {
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

fn render_agents_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    // Status line: "Allowed agents:  [ all ]" or "[ custom ]   (3 of 5 allowed)"
    let is_all = super::super::agent_allow::allows_all_agents(&state.pending);
    let total = config.agents.len();
    let allowed_count = state.pending.allowed_agents.len();

    let badge_text = if is_all { "  all  " } else { "  custom  " };
    let badge_bg = if is_all { PHOSPHOR_GREEN } else { WHITE };
    let badge_style = Style::default()
        .bg(badge_bg)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let mut status_spans = vec![
        Span::styled(
            "  Allowed agents:  ",
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

    // Blank spacer between the status line and the agent rows. The old
    // `allowed?  ·  agent` column header got dropped — the `[x]` / `[ ]`
    // prefix on each row already signals the toggle semantics, so a
    // dedicated header added noise without clarity.
    let mut lines = vec![status_line, Line::from("")];

    // Agent rows. Cursor is 0-based into config.agents (no header offset).
    //
    // `[x]` reflects the *effectively allowed* state, not literal list
    // membership. An empty `allowed_agents` list is the shorthand for
    // "all agents allowed" (matches the `all` badge above) — in that
    // mode every row renders `[x]`. Otherwise only agents named in the
    // list render `[x]`.
    for (i, (agent_name, _)) in config.agents.iter().enumerate() {
        let selected = i == cursor;
        let effectively_allowed =
            super::super::agent_allow::agent_is_effectively_allowed(&state.pending, agent_name);
        let is_default = state.pending.default_agent.as_deref() == Some(agent_name.as_str());
        let check = if effectively_allowed { "[x]" } else { "[ ]" };
        let star = if is_default { "★" } else { " " };
        let prefix = if selected { "▸ " } else { "  " };
        let text = format!("{prefix}{check}  {star} {agent_name}");
        let style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Flat-row model for the Secrets tab. The cursor is a single index into
/// this list; render walks it to draw each row, and input handlers walk
/// it to decide what `Enter` / `D` / `A` / `←` do on the focused row.
#[derive(Debug, Clone)]
pub(in crate::console::manager) enum SecretsRow {
    /// "Workspace env" section header — always present, not expandable
    /// (workspace scope is always visible).
    WorkspaceHeader,
    /// A single workspace-level env key row.
    WorkspaceKeyRow(String),
    /// "+ Add workspace env var" sentinel — always present.
    WorkspaceAddSentinel,
    /// "Agent: NAME" section header. `expanded` mirrors membership in
    /// `editor.secrets_expanded` at the moment the rows were enumerated.
    AgentHeader { agent: String, expanded: bool },
    /// An agent-override env key row — only emitted when the section is
    /// expanded.
    AgentKeyRow { agent: String, key: String },
    /// "+ Add agent-NAME env var" sentinel — only emitted when expanded.
    AgentAddSentinel(String),
}

/// Build the flat row list used by both `render_secrets_tab` (to draw the
/// rows) and the input handlers (to map cursor index → row kind).
///
/// Agent sections render in `BTreeMap` iteration order. Collapsed sections
/// show only the header; expanded sections show header + key rows + add
/// sentinel.
pub(in crate::console::manager) fn secrets_flat_rows(editor: &EditorState<'_>) -> Vec<SecretsRow> {
    let mut rows = vec![SecretsRow::WorkspaceHeader];
    for key in editor.pending.env.keys() {
        rows.push(SecretsRow::WorkspaceKeyRow(key.clone()));
    }
    rows.push(SecretsRow::WorkspaceAddSentinel);
    for agent in editor.pending.agents.keys() {
        let expanded = editor.secrets_expanded.contains(agent);
        rows.push(SecretsRow::AgentHeader {
            agent: agent.clone(),
            expanded,
        });
        if expanded {
            if let Some(ov) = editor.pending.agents.get(agent) {
                for key in ov.env.keys() {
                    rows.push(SecretsRow::AgentKeyRow {
                        agent: agent.clone(),
                        key: key.clone(),
                    });
                }
            }
            rows.push(SecretsRow::AgentAddSentinel(agent.clone()));
        }
    }
    rows
}

/// Number of navigable rows on the Secrets tab. Used by the input
/// handlers' `max_row_for_tab` to clamp the cursor.
#[must_use]
pub(in crate::console::manager) fn secrets_flat_row_count(editor: &EditorState<'_>) -> usize {
    secrets_flat_rows(editor).len()
}

/// Full Secrets-tab render. Reads the flat-row list and walks it once,
/// emitting a `Line` per row. `config` is consumed only for the
/// `(not in registry)` annotation on agent headers.
//
// Match arms per row kind makes the body naturally linear — splitting it
// into per-arm helpers would scatter the table-like structure without
// clarity win. Accept the length for readability.
#[allow(clippy::too_many_lines)]
fn render_secrets_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    let rows = secrets_flat_rows(state);
    let mut lines: Vec<Line> = Vec::with_capacity(rows.len());

    // Label column width — keep identical to General tab so the Secrets
    // tab's visual rhythm matches the rest of the editor.
    let label_width: usize = 22;

    // Workspace env is considered "empty" when no workspace-level keys AND
    // no agent overrides exist. In that case we render a "(no env vars)"
    // dim notice under the header row for the operator's clarity.
    let workspace_empty = state.pending.env.is_empty() && state.pending.agents.is_empty();

    for (i, row) in rows.iter().enumerate() {
        let selected = i == cursor;
        let prefix = if selected { "▸ " } else { "  " };
        match row {
            SecretsRow::WorkspaceHeader => {
                lines.push(Line::from(Span::styled(
                    format!("{prefix}Workspace env"),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                )));
                if workspace_empty {
                    lines.push(Line::from(Span::styled(
                        "    (no env vars)",
                        Style::default()
                            .fg(PHOSPHOR_DIM)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
            SecretsRow::WorkspaceKeyRow(key) => {
                let value = state.pending.env.get(key).cloned().unwrap_or_default();
                let dirty =
                    env_key_is_dirty(state.original.env.get(key), state.pending.env.get(key));
                lines.push(render_secrets_key_line(
                    selected,
                    prefix,
                    key,
                    &value,
                    dirty,
                    state.secrets_masked,
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
                    format!("{prefix}+ Add workspace env var"),
                    style,
                )));
            }
            SecretsRow::AgentHeader { agent, expanded } => {
                let arrow = if *expanded { "▼" } else { "▶" };
                let in_registry = config.agents.contains_key(agent);
                let count = state.pending.agents.get(agent).map_or(0, |o| o.env.len());
                let mut spans = vec![Span::styled(
                    format!("{prefix}{arrow} Agent: {agent}  ({count} vars)"),
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
            SecretsRow::AgentKeyRow { agent, key } => {
                let empty = std::collections::BTreeMap::<String, String>::new();
                let pend_env = state.pending.agents.get(agent).map_or(&empty, |o| &o.env);
                let orig_env = state.original.agents.get(agent).map_or(&empty, |o| &o.env);
                let value = pend_env.get(key).cloned().unwrap_or_default();
                let dirty = env_key_is_dirty(orig_env.get(key), pend_env.get(key));
                lines.push(render_secrets_key_line(
                    selected,
                    prefix,
                    key,
                    &value,
                    dirty,
                    state.secrets_masked,
                    area.width,
                    label_width,
                ));
            }
            SecretsRow::AgentAddSentinel(agent) => {
                let style = if selected {
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}+ Add {agent} env var"),
                    style,
                )));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Diff predicate for a single env key between two maps. Returns `true`
/// when the key was added, removed, or changed.
fn env_key_is_dirty(orig: Option<&String>, pend: Option<&String>) -> bool {
    match (orig, pend) {
        (Some(a), Some(b)) => a != b,
        (None, Some(_)) | (Some(_), None) => true,
        (None, None) => false,
    }
}

/// Render one "KEY  value" row for the Secrets tab with the conventional
/// focus prefix, masking, truncation, and dirty-marker semantics.
#[allow(clippy::too_many_arguments)]
fn render_secrets_key_line(
    selected: bool,
    prefix: &str,
    key: &str,
    value: &str,
    dirty: bool,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> Line<'static> {
    const MASK: &str = "●●●●●●●●●●●";
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
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

    let mut spans = vec![Span::styled(
        format!("{prefix}{key:label_width$}"),
        label_style,
    )];
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
        if value.chars().count() > budget {
            let mut s: String = value.chars().take(budget.saturating_sub(1)).collect();
            s.push('…');
            s
        } else {
            value.to_string()
        }
    };
    spans.push(Span::styled(rendered_value, value_style));
    if dirty {
        spans.push(Span::styled(
            "    ● unsaved",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod contextual_row_items_tests {
    //! Row-specific footer-hint composition for the editor tabs.

    use super::super::FooterItem;
    use super::contextual_row_items;
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
            workdir: String::new(),
            mounts: vec![MountConfig {
                src: src.to_string(),
                dst: src.to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    #[test]
    fn github_mount_row_includes_open_in_github_hint() {
        // Build a synthetic GitHub repo on-disk so `mount_info::inspect`
        // classifies the source as `MountKind::Git { host: Github, web_url: Some }`.
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
        let hint = contextual_row_items(&editor);
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
        let hint = contextual_row_items(&editor);
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
        let hint = contextual_row_items(&editor);
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
        let hint = contextual_row_items(&editor);
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
        // A representative spread: Mounts (data row + sentinel) + Agents.
        // General row 0 Edit + Create uses only `Enter`, which is multi-char.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());

        // Mounts data-row hint.
        let mounts_row = contextual_row_items(&editor);
        assert_hint_hotkeys_uppercase(&mounts_row, "Mounts row 0");

        // Mounts sentinel "+ Add mount" row.
        let mut sentinel_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        sentinel_editor.active_field = FieldFocus::Row(sentinel_editor.pending.mounts.len());
        let sentinel_row = contextual_row_items(&sentinel_editor);
        assert_hint_hotkeys_uppercase(&sentinel_row, "Mounts sentinel");

        // Agents tab uses Space + `*` — both multi-char / non-alpha.
        let mut agents_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        agents_editor.active_tab = EditorTab::Agents;
        let agents_row = contextual_row_items(&agents_editor);
        assert_hint_hotkeys_uppercase(&agents_row, "Agents");
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
    //! Pins the `[x]` / `[ ]` glyph on each agent row to the
    //! *effectively allowed* state, not literal `allowed_agents` list
    //! membership. An empty list is the shorthand for "all allowed",
    //! and every row must render `[x]` in that mode.
    use super::render_agents_tab;
    use crate::config::{AgentSource, AppConfig};
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: names.iter().map(|s| (*s).into()).collect(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.agents.insert((*name).into(), AgentSource::default());
        }
        config
    }

    fn render_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Agents;
        editor.active_field = FieldFocus::Row(0);
        let backend = TestBackend::new(60, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
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
        // Empty `allowed_agents` ⇒ "all" mode ⇒ every row is `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&[]);
        let dump = render_to_dump(ws, &cfg);

        // Every agent name should appear on a line that also carries `[x]`.
        for name in ["alpha", "beta", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("agent `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[x]"),
                "in 'all' mode agent `{name}` row must render `[x]`; got `{line}`"
            );
            assert!(
                !line.contains("[ ]"),
                "in 'all' mode agent `{name}` must not render `[ ]`; got `{line}`"
            );
        }
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
            "listed agent `beta` must render `[x]`; got `{beta_line}`"
        );

        for name in ["alpha", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("agent `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[ ]"),
                "unlisted agent `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
            );
        }
    }
}

#[cfg(test)]
mod secrets_tab_render_tests {
    //! Render-buffer tests for the Secrets tab. Verifies the masking
    //! default, the unmasked literal-value path, and that the flat-row
    //! builder honours `secrets_expanded` for per-agent override sections.
    use super::render_secrets_tab;
    use crate::config::AppConfig;
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{WorkspaceAgentOverride, WorkspaceConfig};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    /// Build an editor sitting on the Secrets tab with a single
    /// workspace-level env key (`DB_URL = postgres://localhost/db`).
    fn editor_with_workspace_env() -> EditorState<'static> {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "postgres://localhost/db".into());
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: Vec::new(),
            default_agent: None,
            last_agent: None,
            env,
            agents: std::collections::BTreeMap::new(),
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    /// Build an editor sitting on the Secrets tab with one agent override
    /// carrying a single env key (`agent-smith`: `LOG_LEVEL = debug`).
    fn editor_with_agent_override() -> EditorState<'static> {
        let mut agent_env = std::collections::BTreeMap::new();
        agent_env.insert("LOG_LEVEL".into(), "debug".into());
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "agent-smith".into(),
            WorkspaceAgentOverride { env: agent_env },
        );
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: Vec::new(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents,
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
        // `new_edit` sets `secrets_masked = true` by default; assert the
        // mask glyph appears and the literal secret value does not.
        let editor = editor_with_workspace_env();
        assert!(
            editor.secrets_masked,
            "new_edit must default secrets_masked to true"
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
        editor.secrets_masked = false;
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
        // the agent section header renders but its `LOG_LEVEL` key row
        // does not.
        let editor = editor_with_agent_override();
        assert!(editor.secrets_expanded.is_empty());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "agent header must render; got:\n{dump}"
        );
        assert!(
            !dump.contains("LOG_LEVEL"),
            "collapsed agent section must omit key rows; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_expanded_agent_shows_key_rows() {
        let mut editor = editor_with_agent_override();
        editor.secrets_expanded.insert("agent-smith".into());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "agent header must still render when expanded; got:\n{dump}"
        );
        assert!(
            dump.contains("LOG_LEVEL"),
            "expanded agent section must show its key rows; got:\n{dump}"
        );
    }
}
