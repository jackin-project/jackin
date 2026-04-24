//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::super::widgets::{confirm, file_browser, save_discard, text_input, workdir_pick};
use super::state::{
    EditorMode, EditorState, EditorTab, FieldFocus, ManagerStage, ManagerState, Modal,
    WorkspaceSummary,
};
use crate::config::AppConfig;

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

// ── Footer item model ──────────────────────────────────────────────
//
// Structured footer items render with a consistent per-stage styling:
//   - Key(k):    WHITE + BOLD   — the literal hotkey glyph(s)
//   - Text(t):   PHOSPHOR_GREEN — the action label after a key
//   - Dyn(t):    PHOSPHOR_DIM   — free-form dynamic text (e.g. "3 changes")
//   - Sep:       PHOSPHOR_DARK  — single-dot separator between key+label pairs
//   - GroupSep:  (three spaces) — wider gap between logical groups
//
// Call sites build `Vec<FooterItem>` directly so the grouping is explicit,
// then hand it to `render_footer`. A convenience `footer_from_str` parser
// exists for legacy call sites that still own their string literal.

#[derive(Debug, Clone)]
pub(super) enum FooterItem {
    Key(&'static str),
    Text(&'static str),
    Dyn(String),
    Sep,
    GroupSep,
}

pub(super) fn footer_spans(items: &[FooterItem]) -> Vec<Span<'static>> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let dyn_style = Style::default().fg(PHOSPHOR_DIM);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(items.len() * 2);
    for (i, item) in items.iter().enumerate() {
        match item {
            FooterItem::Key(k) => {
                // Key glyph — precede with a space when the previous item was a
                // Text/Dyn so the key stands apart from the preceding label.
                spans.push(Span::styled((*k).to_string(), key_style));
            }
            FooterItem::Text(t) => {
                // Label — precede with a single space so key and label are visually
                // paired (e.g. "Enter launch" not "Enterlaunch").
                spans.push(Span::styled(format!(" {t}"), text_style));
            }
            FooterItem::Dyn(t) => {
                spans.push(Span::styled(format!(" {t}"), dyn_style));
            }
            FooterItem::Sep => {
                spans.push(Span::styled(" \u{b7} ".to_string(), sep_style));
            }
            FooterItem::GroupSep => {
                // Wider gap between logical groups.
                spans.push(Span::raw("   "));
            }
        }
        // Avoid trailing separator after the last item; loop logic handles this naturally
        // because separators are explicit items.
        let _ = i;
    }
    spans
}

fn render_footer(frame: &mut Frame, area: Rect, items: &[FooterItem]) {
    let line = Line::from(footer_spans(items));
    let p = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

pub fn render(frame: &mut Frame, state: &ManagerState<'_>, config: &AppConfig) {
    // Phase 1: render the base stage (Editor full-screen OR List chrome).
    if let ManagerStage::Editor(editor) = &state.stage {
        render_editor(frame, editor, config);
    } else {
        // List / CreatePrelude / ConfirmDelete share the list-like chrome.
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(10),   // body
                Constraint::Length(2), // footer
            ])
            .split(area);

        render_header(frame, chunks[0], "workspaces");

        if matches!(&state.stage, ManagerStage::List) {
            render_list_body(frame, chunks[1], state, config);
        }

        let footer_items: Vec<FooterItem> = match &state.stage {
            ManagerStage::List => vec![
                // Navigation group
                FooterItem::Key("\u{2191}\u{2193}"),
                FooterItem::Sep,
                FooterItem::Key("Enter"),
                FooterItem::Text("launch"),
                FooterItem::GroupSep,
                // Per-row actions
                FooterItem::Key("e"),
                FooterItem::Text("edit"),
                FooterItem::Sep,
                FooterItem::Key("n"),
                FooterItem::Text("new"),
                FooterItem::Sep,
                FooterItem::Key("d"),
                FooterItem::Text("delete"),
                FooterItem::GroupSep,
                // Exit
                FooterItem::Key("q"),
                FooterItem::Text("quit"),
            ],
            ManagerStage::CreatePrelude(_) => vec![
                FooterItem::Dyn("Create workspace — follow the prompts".to_string()),
                FooterItem::GroupSep,
                FooterItem::Key("Esc"),
                FooterItem::Text("cancel"),
            ],
            ManagerStage::ConfirmDelete { .. } => vec![
                FooterItem::Key("Y"),
                FooterItem::Text("yes"),
                FooterItem::Sep,
                FooterItem::Key("N"),
                FooterItem::Text("no"),
                FooterItem::GroupSep,
                FooterItem::Key("Esc"),
                FooterItem::Text("cancel"),
            ],
            ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
        };
        render_footer(frame, chunks[2], &footer_items);
    }

    // Phase 2: overlay any active modal.
    match &state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = &editor.modal {
                render_modal(frame, modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = &prelude.modal {
                render_modal(frame, modal);
            }
        }
        ManagerStage::ConfirmDelete {
            state: confirm_state,
            ..
        } => {
            // ConfirmState is a top-level field on the variant, not wrapped
            // in Modal::Confirm, so render it directly.
            let area = frame.area();
            let modal_area = centered_rect_fixed(area, 60, 7);
            super::super::widgets::confirm::render(frame, modal_area, confirm_state);
        }
        ManagerStage::List => {}
    }
}

fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(
            "JACKIN",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     · "),
        Span::styled(title.to_string(), Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_list_body(frame: &mut Frame, area: Rect, state: &ManagerState<'_>, config: &AppConfig) {
    let is_sentinel = state.selected >= state.workspaces.len();

    // Always split 45/55 so the right pane stays visible even when the
    // cursor is on "+ New workspace". On the sentinel we render an empty
    // bordered pane (same border style as the details pane) instead of the
    // General/Mounts/Agents blocks.
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let list_area = columns[0];

    if is_sentinel {
        render_sentinel_description_pane(frame, columns[1]);
    } else if let Some(ws) = state.workspaces.get(state.selected) {
        render_details_pane(frame, columns[1], ws, config);
    }

    // Left: list of workspaces + [+ New workspace] sentinel.
    let mut items: Vec<ListItem> = state
        .workspaces
        .iter()
        .map(|w| ListItem::new(Line::from(w.name.as_str())))
        .collect();
    items.push(ListItem::new(Line::from(Span::styled(
        "+ New workspace",
        Style::default().fg(WHITE),
    ))));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PHOSPHOR_DARK)),
        )
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");

    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    frame.render_stateful_widget(list, list_area, &mut ls);

    // Toast overlay — rendered last so it appears on top.
    if let Some(toast) = &state.toast {
        render_toast(frame, area, toast);
    }
}

fn render_toast(frame: &mut Frame, area: Rect, toast: &super::state::Toast) {
    use super::state::ToastKind;
    let elapsed = toast.shown_at.elapsed();
    // Auto-expire after 3 seconds — caller should clear before that,
    // but defensively skip rendering if we're past.
    if elapsed > std::time::Duration::from_secs(3) {
        return;
    }

    let (prefix, color) = match toast.kind {
        ToastKind::Success => ("✓ ", PHOSPHOR_GREEN),
        ToastKind::Error => ("✗ ", Color::Rgb(255, 94, 122)),
    };
    let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    // Shimmer: first 400ms is bright-white flicker, then settles.
    if elapsed < std::time::Duration::from_millis(400) {
        style = style.fg(WHITE);
    }
    let line = Line::from(Span::styled(format!("{}{}", prefix, toast.message), style));
    let banner_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(ratatui::widgets::Clear, banner_area);
    frame.render_widget(Paragraph::new(line), banner_area);
}

/// Build aligned 3-column mount rows: (`path_display`, mode, `kind_label`).
fn format_mount_rows(
    mounts: &[crate::workspace::MountConfig],
) -> Vec<(String, &'static str, String)> {
    mounts
        .iter()
        .map(|m| {
            let src = crate::tui::shorten_home(&m.src);
            let dst = crate::tui::shorten_home(&m.dst);
            let path = if m.src == m.dst {
                src
            } else {
                format!("{src} \u{2192} {dst}")
            };
            let mode: &'static str = if m.readonly { "ro" } else { "rw" };
            let kind = super::mount_info::inspect(&m.src).label();
            (path, mode, kind)
        })
        .collect()
}

/// Width of the `mode` column, including one trailing space that separates it
/// from the `type` column. Also matches the `mode` label in the header.
const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Compute the width used for the `path` column so that header and data rows
/// align. Derived from both the "path" header label and the widest row path,
/// with a minimum floor so short-path tables still look tabular.
fn mount_path_width(rows: &[(String, &str, String)]) -> usize {
    rows.iter()
        .map(|(p, _, _)| p.chars().count())
        .max()
        .unwrap_or(0)
        .max(10) // floor so a single-row mount still has a clear column
        .max("path".len())
}

/// Header row for the mount table. `path_w` must come from
/// [`mount_path_width`] over the same `rows` used for the data lines so the
/// columns line up.
fn render_mount_header(path_w: usize) -> Line<'static> {
    // Format: "  <path padded to path_w>  <mode padded to MODE_W>type"
    // Leading two-space gutter matches the data-row format.
    let mode_col = format!("{:<mw$}", "mode", mw = MOUNT_MODE_COL_WIDTH);
    Line::from(Span::styled(
        format!("  {path:<path_w$}  {mode_col}type", path = "path"),
        Style::default().fg(WHITE),
    ))
}

/// Render aligned mount rows as `Line`s (no selection prefix). `path_w` is
/// passed in so the header and data rows share the same column boundary.
fn render_mount_lines(rows: &[(String, &str, String)], path_w: usize) -> Vec<Line<'static>> {
    rows.iter()
        .map(|(path, mode, kind)| {
            Line::from(vec![
                Span::raw(format!("  {path:<path_w$}  ")),
                Span::styled(
                    format!("{mode:<mw$}", mw = MOUNT_MODE_COL_WIDTH),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
                Span::styled(
                    kind.clone(),
                    Style::default()
                        .fg(PHOSPHOR_DIM)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])
        })
        .collect()
}

fn render_details_pane(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary, config: &AppConfig) {
    let ws_config = config.workspaces.get(&ws.name);
    let mounts = ws_config.map_or(&[][..], |w| w.mounts.as_slice());

    // Mount rows needed: 1 header row + N mounts (min 1 for "(none)") + 2 borders.
    // Clamp to a reasonable maximum so a workspace with many mounts doesn't eat the screen.
    let mount_data_rows = if mounts.is_empty() { 1 } else { mounts.len() };
    let mount_block_height = (mount_data_rows + 2 + 1).min(12) as u16; // +1 header, +2 borders

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),                  // General: workdir + last + 2 borders
            Constraint::Length(mount_block_height), // Mounts: header + N rows + 2 borders
            Constraint::Min(3),                     // Agents: takes remaining space
        ])
        .split(area);

    render_general_subpanel(frame, rows[0], ws);
    render_mounts_subpanel(frame, rows[1], mounts);
    render_agents_subpanel(frame, rows[2], ws_config, config);
}

/// Right-pane description shown when the cursor is on the "+ New workspace"
/// sentinel. Explains what a workspace is and why the operator might create
/// one — compacted from `docs/src/content/docs/guides/workspaces.mdx`
/// sections "What is a workspace?" + "Why save a workspace?".
fn render_sentinel_description_pane(frame: &mut Frame, area: Rect) {
    // Two stacked sub-panels so the section titles render as block titles
    // with the same PHOSPHOR_DARK border used by General/Mounts/Agents.
    // The "What is a workspace?" intro is short (fits in 4 rows); the
    // rest of the area hosts the bullet list + closing hint.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // "What is a workspace?" intro (2 text rows + 2 borders + 1 pad)
            Constraint::Min(9),    // "Why create one?" bullets + hint
        ])
        .split(area);

    let intro_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " What is a workspace? ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let intro_lines = vec![
        Line::from(Span::styled(
            "  A workspace saves a project boundary once so you",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  can launch agents into it from anywhere \u{2014} without",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  retyping mount paths.",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
    ];
    frame.render_widget(Paragraph::new(intro_lines).block(intro_block), rows[0]);

    let why_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Why create one? ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let bullet_style = Style::default().fg(PHOSPHOR_GREEN);
    let bullets = [
        "Name a project once, launch from any cwd",
        "Keep extra mounts consistent across sessions",
        "Reuse one boundary with different agent classes",
        "Set a default agent or restrict which classes apply",
        "Let `jackin launch` auto-detect and preselect it",
    ];
    let mut why_lines: Vec<Line<'static>> = bullets
        .iter()
        .map(|b| Line::from(Span::styled(format!("  \u{2022} {b}"), bullet_style)))
        .collect();
    why_lines.push(Line::from(""));
    why_lines.push(Line::from(Span::styled(
        "  Press Enter to start the setup wizard.",
        Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC),
    )));
    frame.render_widget(Paragraph::new(why_lines).block(why_block), rows[1]);
}

fn render_general_subpanel(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " General ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let lines = vec![
        Line::from(vec![
            Span::styled("workdir   ", Style::default().fg(WHITE)),
            Span::raw(crate::tui::shorten_home(&ws.workdir)),
        ]),
        Line::from(vec![
            Span::styled("last      ", Style::default().fg(WHITE)),
            Span::raw(
                ws.last_agent
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
            ),
        ]),
    ];

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_mounts_subpanel(frame: &mut Frame, area: Rect, mounts: &[crate::workspace::MountConfig]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Mounts ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let mut lines: Vec<Line> = Vec::new();

    if mounts.is_empty() {
        // No data rows — fall back to the minimum column width so the header
        // still shows sensible column boundaries.
        lines.push(render_mount_header(mount_path_width(&[])));
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    } else {
        // TODO: labeled_hyperlink() emits OSC 8 ESC sequences which ratatui's
        // Paragraph widget may strip or render as garbage (it doesn't pass raw
        // bytes through). Until there is a raw-terminal-write path, fall back
        // to label() (plain text). The hyperlink infrastructure is wired up in
        // MountKind::labeled_hyperlink() for future use.
        let rows = format_mount_rows(mounts);
        let path_w = mount_path_width(&rows);
        lines.push(render_mount_header(path_w));
        lines.extend(render_mount_lines(&rows, path_w));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_agents_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Agents ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let allowed = ws_config.map_or(&[][..], |w| w.allowed_agents.as_slice());

    let mut lines: Vec<Line> = Vec::new();

    if allowed.is_empty() {
        lines.push(Line::from(Span::styled(
            "  any agent",
            Style::default()
                .fg(Color::Rgb(180, 255, 180))
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        let default = ws_config.and_then(|w| w.default_agent.as_deref());
        // TODO: agent names could link to the agent's source repository on
        // GitHub via OSC 8 hyperlinks, but ratatui's Paragraph widget strips
        // those escape sequences. Until there is a raw-terminal-write path,
        // fall back to plain text — same limitation as render_mounts_subpanel's
        // labeled_hyperlink() TODO above.
        // Show only allowed agents that exist in the global config (consistent
        // with the editor view). Fall back to listing all allowed names if the
        // agent is no longer registered globally.
        for agent in allowed {
            let star = if Some(agent.as_str()) == default {
                "\u{2605} "
            } else {
                "  "
            };
            let style = if config.agents.contains_key(agent) {
                Style::default().fg(PHOSPHOR_GREEN)
            } else {
                Style::default().fg(PHOSPHOR_DIM)
            };
            lines.push(Line::from(Span::styled(format!("  {star}{agent}"), style)));
        }
    }

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

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
        EditorTab::Secrets => render_secrets_stub(frame, chunks[2]),
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
    items.push(FooterItem::Key("s"));
    if state.is_dirty() {
        items.push(FooterItem::Text("save workspace"));
        items.push(FooterItem::Dyn(format!(
            "({} changes)",
            state.change_count()
        )));
    } else {
        items.push(FooterItem::Text("save workspace"));
    }

    // Navigation group.
    items.push(FooterItem::GroupSep);
    items.push(FooterItem::Key("Tab"));
    items.push(FooterItem::Text("next"));
    items.push(FooterItem::Sep);
    items.push(FooterItem::Key("\u{2191}\u{2193}"));

    // Exit group — discard if dirty, back if clean.
    items.push(FooterItem::GroupSep);
    items.push(FooterItem::Key("Esc"));
    if state.is_dirty() {
        items.push(FooterItem::Text("discard"));
    } else {
        items.push(FooterItem::Text("back"));
    }

    render_footer(frame, chunks[3], &items);

    // Error banner overlay — top line of the body.
    if let Some(err) = &state.error_banner {
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
            // Row indices depend on mode:
            //   Create: 0 = workdir  (name is read-only display in Create)
            //   Edit:   0 = name, 1 = workdir, 2 = default agent (ro), 3 = last used (ro)
            match &state.mode {
                EditorMode::Create => match cursor {
                    0 => vec![FooterItem::Key("Enter"), FooterItem::Text("pick workdir")],
                    _ => Vec::new(),
                },
                EditorMode::Edit { .. } => match cursor {
                    0 => vec![FooterItem::Key("Enter"), FooterItem::Text("rename")],
                    1 => vec![FooterItem::Key("Enter"), FooterItem::Text("pick workdir")],
                    _ => Vec::new(), // default agent and last used are read-only
                },
            }
        }
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            if cursor < mount_count {
                vec![
                    FooterItem::Key("d"),
                    FooterItem::Text("remove"),
                    FooterItem::Sep,
                    FooterItem::Key("a"),
                    FooterItem::Text("add"),
                ]
            } else {
                // Sentinel "+ Add mount" row
                vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("add"),
                    FooterItem::Sep,
                    FooterItem::Key("a"),
                    FooterItem::Text("add"),
                ]
            }
        }
        EditorTab::Agents => vec![
            FooterItem::Key("Space"),
            FooterItem::Text("toggle"),
            FooterItem::Sep,
            FooterItem::Key("*"),
            FooterItem::Text("set default"),
        ],
        EditorTab::Secrets => Vec::new(),
    }
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Agents, "Agents"),
        (EditorTab::Secrets, "Secrets ⏳"),
    ];
    let mut spans = Vec::new();
    for (tab, label) in labels {
        let style = if tab == active {
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if tab == EditorTab::Secrets {
            Style::default()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(Modifier::ITALIC)
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
    //   0 = name (read-only display — name comes from prelude)
    //   1 = workdir
    // In Edit mode:
    //   0 = name (editable), 1 = workdir, 2 = default agent (ro), 3 = last used (ro)
    let mut rows: Vec<Line> = Vec::new();

    if is_edit {
        // Edit mode: name is an editable row at index 0.
        rows.push(render_editor_row(0, cursor, "name", name_value, name_dirty));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "workdir",
            &workdir_display,
            state.pending.workdir != state.original.workdir,
        ));
        // default agent — read-only here; set via Agents tab.
        rows.push(render_editor_readonly_row(
            2,
            cursor,
            "default agent",
            state.pending.default_agent.as_deref().unwrap_or("(none)"),
        ));
        // last used — read-only.
        rows.push(render_editor_readonly_row(
            3,
            cursor,
            "last used",
            state.original.last_agent.as_deref().unwrap_or("(none)"),
        ));
    } else {
        // Create mode: name is display-only (collected by prelude), workdir is the first editable row.
        rows.push(render_editor_readonly_row(0, cursor, "name", name_value));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "workdir",
            &workdir_display,
            false,
        ));
        // Hide "default agent" and "last used" in Create mode — they have no meaning yet.
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
                format!("{mode:<mw$}", mw = MOUNT_MODE_COL_WIDTH),
                Style::default().fg(PHOSPHOR_DIM),
            ),
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
    let is_all = state.pending.allowed_agents.is_empty();
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

    // Column header
    let header = Line::from(Span::styled(
        "  allowed?  ·  agent",
        Style::default().fg(WHITE),
    ));

    let mut lines = vec![status_line, header];

    // Agent rows. Cursor is 0-based into config.agents (no header offset).
    for (i, (agent_name, _)) in config.agents.iter().enumerate() {
        let selected = i == cursor;
        let allowed = state.pending.allowed_agents.contains(agent_name);
        let is_default = state.pending.default_agent.as_deref() == Some(agent_name.as_str());
        let check = if allowed { "[x]" } else { "[ ]" };
        let star = if is_default { "★" } else { " " };
        let prefix = if selected { "▸ " } else { "  " };
        let text = format!("{prefix}{check}    {star} {agent_name}");
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

fn render_secrets_stub(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Secrets management lands in PR 3 of this series.",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        )),
    ];
    frame.render_widget(Paragraph::new(body).block(block), area);
}

// ── Modal dispatcher ────────────────────────────────────────────────

pub fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    // Size by variant: single-line inputs get a compact overlay;
    // lists get a taller one.
    let (pct_w, height_rows) = match modal {
        Modal::TextInput { .. } => (60, 5), // label + input + hint = 5 rows
        // Confirm height varies with prompt length (e.g. the mount-collapse
        // prompt lists each child/parent pair on its own line).
        Modal::Confirm { state, .. } => (60, confirm::required_height(state)),
        Modal::SaveDiscardCancel { .. } => (70, 7), // three buttons — a bit wider
        Modal::FileBrowser { .. } => (70, 70), // dialog-sized — 70%×70% lets chrome show around it
        Modal::WorkdirPick { .. } => (60, 12), // ~6 choices + title + hint
    };
    let modal_area = centered_rect_fixed(area, pct_w, height_rows);
    match modal {
        Modal::TextInput { state, .. } => text_input::render(frame, modal_area, state),
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => confirm::render(frame, modal_area, state),
        Modal::SaveDiscardCancel { state } => save_discard::render(frame, modal_area, state),
    }
}

/// Like `centered_rect` but takes a fixed number of rows for the height.
/// `pct_w` is still a percentage of the outer width. Rows are clamped to fit.
fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod footer_tests {
    use super::{FOOTER_KEY, FOOTER_SEP, FOOTER_TEXT, FooterItem, footer_spans};

    // Sanity — the exported style colors match the palette.
    #[test]
    fn styling_colors_match_palette() {
        let key = FOOTER_KEY;
        let text = FOOTER_TEXT;
        let sep = FOOTER_SEP;
        assert_eq!(key.fg, Some(super::WHITE));
        assert_eq!(text.fg, Some(super::PHOSPHOR_GREEN));
        assert_eq!(sep.fg, Some(super::PHOSPHOR_DARK));
    }

    #[test]
    fn key_and_text_render_with_distinct_styles() {
        let items = vec![FooterItem::Key("Enter"), FooterItem::Text("launch")];
        let spans = footer_spans(&items);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "Enter");
        assert_eq!(spans[0].style.fg, Some(super::WHITE));
        assert_eq!(spans[1].content.as_ref(), " launch");
        assert_eq!(spans[1].style.fg, Some(super::PHOSPHOR_GREEN));
    }

    #[test]
    fn sep_renders_with_phosphor_dark() {
        let items = vec![
            FooterItem::Key("e"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("n"),
            FooterItem::Text("new"),
        ];
        let spans = footer_spans(&items);
        // third item is the Sep
        assert_eq!(spans[2].content.as_ref(), " \u{b7} ");
        assert_eq!(spans[2].style.fg, Some(super::PHOSPHOR_DARK));
    }

    #[test]
    fn group_sep_renders_as_three_raw_spaces() {
        let items = vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("launch"),
            FooterItem::GroupSep,
            FooterItem::Key("q"),
            FooterItem::Text("quit"),
        ];
        let spans = footer_spans(&items);
        assert_eq!(spans[2].content.as_ref(), "   ");
        // GroupSep is styled with a plain ratatui::Style::default() — no fg set.
        assert_eq!(spans[2].style.fg, None);
    }

    #[test]
    fn dyn_item_uses_phosphor_dim() {
        let items = vec![FooterItem::Dyn("3 changes".to_string())];
        let spans = footer_spans(&items);
        assert_eq!(spans[0].content.as_ref(), " 3 changes");
        assert_eq!(spans[0].style.fg, Some(super::PHOSPHOR_DIM));
    }

    // Per-stage smoke tests — the List footer should have all six keys styled
    // as WHITE+BOLD and two GroupSep separators.
    #[test]
    fn list_footer_items_have_expected_structure() {
        let items: Vec<FooterItem> = vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Sep,
            FooterItem::Key("Enter"),
            FooterItem::Text("launch"),
            FooterItem::GroupSep,
            FooterItem::Key("e"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("n"),
            FooterItem::Text("new"),
            FooterItem::Sep,
            FooterItem::Key("d"),
            FooterItem::Text("delete"),
            FooterItem::GroupSep,
            FooterItem::Key("q"),
            FooterItem::Text("quit"),
        ];
        let spans = footer_spans(&items);
        // Every Key should be styled WHITE + BOLD; count them.
        let key_count = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::WHITE))
            .count();
        assert_eq!(key_count, 6, "↑↓, Enter, e, n, d, q");
        // Every Text should be styled PHOSPHOR_GREEN; count them.
        let text_count = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::PHOSPHOR_GREEN))
            .count();
        assert_eq!(text_count, 5, "launch, edit, new, delete, quit");
        // GroupSep count (content == "   ", no fg).
        let group_sep_count = spans
            .iter()
            .filter(|s| s.content.as_ref() == "   " && s.style.fg.is_none())
            .count();
        assert_eq!(group_sep_count, 2, "nav | per-row | exit");
    }

    #[test]
    fn confirm_delete_footer_items_have_expected_structure() {
        let items: Vec<FooterItem> = vec![
            FooterItem::Key("Y"),
            FooterItem::Text("yes"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("no"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ];
        let spans = footer_spans(&items);
        let keys: Vec<&str> = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::WHITE))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(keys, vec!["Y", "N", "Esc"]);
    }
}

// Re-export the per-item Styles used in tests so assertions don't need to
// recompute them from the palette.
#[cfg(test)]
const FOOTER_KEY: ratatui::style::Style = ratatui::style::Style::new()
    .fg(WHITE)
    .add_modifier(ratatui::style::Modifier::BOLD);
#[cfg(test)]
const FOOTER_TEXT: ratatui::style::Style = ratatui::style::Style::new().fg(PHOSPHOR_GREEN);
#[cfg(test)]
const FOOTER_SEP: ratatui::style::Style = ratatui::style::Style::new().fg(PHOSPHOR_DARK);

#[cfg(test)]
mod mount_table_tests {
    use super::{MOUNT_MODE_COL_WIDTH, mount_path_width, render_mount_header, render_mount_lines};

    /// Collapse a `Line` into a single plain string (concat of all span contents).
    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    /// Return the character index of the start of the `mode` column (i.e. the
    /// "m" in "mode" for the header, or the first char of "ro"/"rw" for a data
    /// row). Both are found at: `"  " + path_w + "  "` — so the index equals
    /// `2 + path_w + 2` for a header and for data rows that have no selection
    /// prefix (and the selection prefix is always two chars too — "▸ " or
    /// "  " — so the column boundary is stable).
    fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
        let s = line_text(line);
        // The mode column is the first two-letter "rw"/"ro" after the gap,
        // or the literal "mode" for the header. Scan for the first non-space
        // character after the gap-of-two-spaces that follows the path.
        // Simpler: find the offset of the two-space gap before mode.
        // Header: "  path<pad>  mode<pad>type"
        // Data:   "  path<pad>  rw<pad>type"
        // In both cases the left edge of "mode"/"rw" is exactly 2 + path_w + 2
        // from the start — we recover it by scanning for the first non-space
        // char at position >= 4 (past the left gutter + at least one path char).
        // Instead, just look for the substring "  m" (mode header) or "  r"
        // (data row, always "rw"/"ro" starting with r).
        for (i, c) in s.chars().enumerate() {
            if i < 4 {
                continue;
            }
            if c == 'm' || c == 'r' {
                // Make sure this is preceded by the two-space gap — the first
                // such occurrence past the left gutter is the column boundary.
                let prev_two: String = s.chars().skip(i.saturating_sub(2)).take(2).collect();
                if prev_two == "  " {
                    return i;
                }
            }
        }
        panic!("mode column not found in line: {s:?}");
    }

    #[test]
    fn header_and_data_rows_share_path_column_width() {
        // Short path + long path forces path_w to be the length of the long one.
        let rows: Vec<(String, &str, String)> = vec![
            ("~/short".into(), "rw", "git · main".into()),
            (
                "~/Projects/very/deeply/nested/directory".into(),
                "ro",
                "dir".into(),
            ),
        ];
        let path_w = mount_path_width(&rows);
        assert!(path_w >= "~/Projects/very/deeply/nested/directory".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);

        let header_mode_col = mode_col_start(&header);
        let data0_mode_col = mode_col_start(&data[0]);
        let data1_mode_col = mode_col_start(&data[1]);

        assert_eq!(
            header_mode_col, data0_mode_col,
            "header 'mode' column must align with data row 0"
        );
        assert_eq!(
            header_mode_col, data1_mode_col,
            "header 'mode' column must align with data row 1"
        );
    }

    #[test]
    fn single_row_still_uses_minimum_column_width() {
        // Single short mount — path_w should stay at the floor so the
        // table is still visibly tabular.
        let rows: Vec<(String, &str, String)> = vec![(
            "~/Projects/ChainArgos/blockchain-nodes".into(),
            "rw",
            "git · main".into(),
        )];
        let path_w = mount_path_width(&rows);
        assert_eq!(path_w, "~/Projects/ChainArgos/blockchain-nodes".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        assert_eq!(mode_col_start(&header), mode_col_start(&data[0]));
    }

    #[test]
    fn empty_rows_uses_floor_for_header() {
        // Empty case: header should still render with the floor width
        // (so the 'type' column is at least `4 + 10 + 2 + 4 = 20`).
        let path_w = mount_path_width(&[]);
        assert_eq!(path_w, 10);
        let header = render_mount_header(path_w);
        // "  path      <2 pad>  mode<4-w pad>type"
        let expected = format!(
            "  {path:<path_w$}  {mode:<mw$}type",
            path = "path",
            mode = "mode",
            path_w = path_w,
            mw = MOUNT_MODE_COL_WIDTH,
        );
        let s = line_text(&header);
        assert_eq!(s, expected);
    }
}
