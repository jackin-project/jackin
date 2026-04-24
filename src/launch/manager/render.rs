//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::super::widgets::{
    confirm, confirm_save, error_popup, file_browser, github_picker, mount_dst_choice,
    save_discard, text_input, workdir_pick,
};
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

pub fn render(
    frame: &mut Frame,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
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
            render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<FooterItem> = match &state.stage {
            ManagerStage::List => {
                // Surface "o open in GitHub" on rows whose workspace has at
                // least one GitHub-hosted mount with a resolvable web URL.
                // Current-dir row 0 and the "+ New workspace" sentinel skip
                // the hint entirely.
                let saved_count = state.workspaces.len();
                let sentinel_idx = saved_count + 1;
                let show_open_hint = state.selected >= 1
                    && state.selected < sentinel_idx
                    && state
                        .workspaces
                        .get(state.selected - 1)
                        .and_then(|s| config.workspaces.get(&s.name))
                        .is_some_and(|ws| {
                            !super::input::resolve_github_mounts_for_workspace(ws).is_empty()
                        });

                let mut items = vec![
                    // Navigation group
                    FooterItem::Key("\u{2191}\u{2193}"),
                    FooterItem::Sep,
                    FooterItem::Key("Enter"),
                    FooterItem::Text("launch"),
                    FooterItem::GroupSep,
                    // Per-row actions
                    FooterItem::Key("E"),
                    FooterItem::Text("edit"),
                    FooterItem::Sep,
                    FooterItem::Key("N"),
                    FooterItem::Text("new"),
                    FooterItem::Sep,
                    FooterItem::Key("D"),
                    FooterItem::Text("delete"),
                ];
                if show_open_hint {
                    items.push(FooterItem::Sep);
                    items.push(FooterItem::Key("O"));
                    items.push(FooterItem::Text("open in GitHub"));
                }
                items.push(FooterItem::GroupSep);
                // Exit
                items.push(FooterItem::Key("Q"));
                items.push(FooterItem::Text("quit"));
                items
            }
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
        ManagerStage::List => {
            // List-level modals (e.g. Modal::GithubPicker opened via `o`
            // on a workspace row) are anchored on ManagerState, not on a
            // stage variant. Render them last so they overlay the list.
            if let Some(modal) = &state.list_modal {
                render_modal(frame, modal);
            }
        }
    }
}

fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(
            "jackin'",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     · "),
        Span::styled(title.to_string(), Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_list_body(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // Row layout (mirrors ManagerState::from_config / handle_list_key):
    //   0                 → synthetic "Current directory"
    //   1..=saved_count   → saved workspaces (saved_index = selected - 1)
    //   saved_count + 1   → "+ New workspace" sentinel
    let saved_count = state.workspaces.len();
    let sentinel_idx = saved_count + 1;
    let is_current_dir = state.selected == 0;
    let is_sentinel = state.selected == sentinel_idx;

    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   row 0             → current-dir details
    //   saved rows        → saved-workspace details
    //   sentinel          → description-of-what-a-workspace-is pane
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let list_area = columns[0];

    if is_current_dir {
        render_current_dir_details_pane(frame, columns[1], cwd);
    } else if is_sentinel {
        render_sentinel_description_pane(frame, columns[1]);
    } else if let Some(ws) = state.workspaces.get(state.selected - 1) {
        render_details_pane(frame, columns[1], ws, config);
    }

    // Left: [Current directory] + saved workspaces + [+ New workspace].
    // The cwd path itself is shown on the right-pane `workdir` line; keep the
    // list row label short to avoid duplicate visual load.
    let mut items: Vec<ListItem> = Vec::with_capacity(saved_count + 2);
    items.push(ListItem::new(Line::from(Span::styled(
        "Current directory",
        Style::default().fg(WHITE),
    ))));
    items.extend(
        state
            .workspaces
            .iter()
            .map(|w| ListItem::new(Line::from(w.name.as_str()))),
    );
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

/// Width of the `Mode` column. Pinned to the length of the header label
/// "Mode" (4 chars) so the data-row values "rw"/"ro" pad to the same width.
/// Without the pad, `rw`/`ro` (2 chars) would render 2 chars short of the
/// header's 4, shifting the `Type` column left by 2 on every data row.
/// Both the header and data rows emit a two-space gutter after this column
/// before the `Type` column so "Mode" and "Type" never run together.
const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Compute the width used for the `Path` column so that header and data rows
/// align. Derived from both the "Path" header label and the widest row path,
/// with a minimum floor so short-path tables still look tabular.
fn mount_path_width(rows: &[(String, &str, String)]) -> usize {
    rows.iter()
        .map(|(p, _, _)| p.chars().count())
        .max()
        .unwrap_or(0)
        .max(10) // floor so a single-row mount still has a clear column
        .max("Path".len())
}

/// Header row for the mount table. `path_w` must come from
/// [`mount_path_width`] over the same `rows` used for the data lines so the
/// columns line up.
fn render_mount_header(path_w: usize) -> Line<'static> {
    // Format: "  <path padded to path_w>  <mode padded>  Type"
    // Leading two-space gutter + two-space gap between Mode and Type both
    // match the data-row format — so "Mode" never runs into "Type".
    let mode_col = format!("{:<mw$}", "Mode", mw = MOUNT_MODE_COL_WIDTH);
    Line::from(Span::styled(
        format!("  {path:<path_w$}  {mode_col}  Type", path = "Path"),
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
                    format!("{mode:<MOUNT_MODE_COL_WIDTH$}"),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
                // Two-space gap before the type column — matches the header.
                Span::raw("  "),
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

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // General: workdir + 2 borders (Last used moved to Agents)
            Constraint::Length(mount_block_height(mounts)), // Mounts: header + N rows + 2 borders
            Constraint::Min(5),    // Agents: last_used + blank + "any agent"/list + 2 borders
        ])
        .split(area);

    render_general_subpanel(frame, rows[0], ws);
    render_mounts_subpanel(frame, rows[1], mounts);
    render_agents_subpanel(frame, rows[2], ws_config, config);
}

/// Exact row count a Mounts sub-panel needs to render `mounts` without
/// leaving a phantom empty row inside the block. Layout: 2 borders + 1
/// header row + N data rows (minimum 1 for the "(none)" placeholder when
/// `mounts` is empty). Clamped to a reasonable maximum so a workspace with
/// many mounts can't eat the full right pane.
///
/// Shared by `render_details_pane` and `render_current_dir_details_pane`
/// so both produce identically-tight blocks.
fn mount_block_height(mounts: &[crate::workspace::MountConfig]) -> u16 {
    let data_rows = if mounts.is_empty() { 1 } else { mounts.len() };
    (data_rows + 2 + 1).min(12) as u16 // +1 header, +2 borders
}

/// Right-pane details shown when the cursor is on the synthetic "Current
/// directory" row (row 0). Summarises the cwd workspace that would be
/// launched: workdir + the auto-mount derived from cwd + "any agent".
///
/// Keeps the General/Mounts/Agents three-block vertical layout of
/// `render_details_pane` so operators see a familiar shape, but uses
/// a dedicated General block (no "last used" row — not meaningful for
/// a non-persistent launch) with the " Current directory " title.
fn render_current_dir_details_pane(frame: &mut Frame, area: Rect, cwd: &std::path::Path) {
    let cwd_str = cwd.display().to_string();
    let workdir_short = crate::tui::shorten_home(&cwd_str);

    // The single auto-mount mirrors `workspace::current_dir_workspace`:
    // src = dst = cwd, rw. Built here as a local so we can pass it into
    // the shared `render_mounts_subpanel` helper.
    let mounts = [crate::workspace::MountConfig {
        src: cwd_str.clone(),
        dst: cwd_str,
        readonly: false,
    }];

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                           // General: workdir + 2 borders
            Constraint::Length(mount_block_height(&mounts)), // Mounts: header + N rows + 2 borders
            Constraint::Min(5), // Agents: last_used + blank + "any agent" + 2 borders
        ])
        .split(area);

    // General — titled the same as the saved-workspace pane so the three
    // sub-panel titles (General / Mounts / Agents) match across both panes.
    // The "Current directory" signpost is already visible as the left-list
    // row label, so repeating it here was redundant.
    let general_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " General ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    // Two-space prefix keeps the label aligned with Mounts and Agents — see
    // `render_general_subpanel` for the shared convention.
    let general_lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(WHITE)),
        Span::raw(workdir_short),
    ])];
    frame.render_widget(
        Paragraph::new(general_lines)
            .block(general_block)
            .style(Style::default().fg(PHOSPHOR_GREEN)),
        rows[0],
    );

    render_mounts_subpanel(frame, rows[1], &mounts);

    // Agents block — reuse the empty-allowed-list branch of the shared
    // renderer by passing `ws_config = None`, which falls through to the
    // "any agent" italic-light-green path.
    render_agents_subpanel(frame, rows[2], None, &AppConfig::default());
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

    // Each content row is prefixed with two spaces to match the Mounts and
    // Agents sub-panels (see `SUBPANEL_CONTENT_INDENT`). Without the prefix the
    // label sat flush against the block's left border, breaking column
    // alignment with the other two blocks in the same pane.
    //
    // The `Last used` row used to live here; it now sits at the top of the
    // Agents sub-panel where it semantically belongs (agent-identity data,
    // not path/workspace-identity data).
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(WHITE)),
        Span::raw(crate::tui::shorten_home(&ws.workdir)),
    ])];

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

/// Number of leading spaces every content row in the General / Mounts /
/// Agents sub-panels is prefixed with, so the first visible character lines
/// up across all three blocks (at `border_col + SUBPANEL_CONTENT_INDENT`).
/// Pinned by `subpanel_content_column_alignment` in the visual regression
/// tests.
#[cfg(test)]
const SUBPANEL_CONTENT_INDENT: usize = 2;

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
        // Plain-text labels — the operator uses the `o` key on a selected
        // mount row to open the GitHub URL in a real browser instead.
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

    // `Last used` row — moved from the General sub-panel so the right pane's
    // agent-identity data all lives under the Agents header. Always rendered
    // at the top of the Agents block, whether the value is a real agent name
    // (phosphor-green) or the `(none)` placeholder (phosphor-dim).
    //
    // A blank line follows so the value visually detaches from the allow list.
    let last = ws_config.and_then(|w| w.last_agent.as_deref());
    let (value_text, value_style): (String, Style) = last.map_or_else(
        || ("(none)".to_string(), Style::default().fg(PHOSPHOR_DIM)),
        |name| (name.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
    );
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Last used   ", Style::default().fg(WHITE)),
        Span::styled(value_text, value_style),
    ]));
    lines.push(Line::from(""));

    if allowed.is_empty() {
        lines.push(Line::from(Span::styled(
            "  any agent",
            Style::default()
                .fg(Color::Rgb(180, 255, 180))
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        let default = ws_config.and_then(|w| w.default_agent.as_deref());
        // Show only allowed agents that exist in the global config (consistent
        // with the editor view). Fall back to listing all allowed names if the
        // agent is no longer registered globally. Agent name always starts at
        // `SUBPANEL_CONTENT_INDENT` (col 2 from border); default agents get a
        // trailing star on their own span so the name keeps the phosphor-green
        // base color and the star gets its own low-chrome style.
        let name_style = |agent: &str| {
            if config.agents.contains_key(agent) {
                Style::default().fg(PHOSPHOR_GREEN)
            } else {
                Style::default().fg(PHOSPHOR_DIM)
            }
        };
        let star_style = Style::default().fg(PHOSPHOR_DIM);
        for agent in allowed {
            let is_default = Some(agent.as_str()) == default;
            let mut spans = vec![Span::styled(format!("  {agent}"), name_style(agent))];
            if is_default {
                spans.push(Span::styled(" \u{2605}", star_style));
            }
            lines.push(Line::from(spans));
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
                        super::mount_info::inspect(&m.src),
                        super::mount_info::MountKind::Git {
                            host: super::mount_info::GitHost::Github,
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
        let effectively_allowed = state.pending.allowed_agents.is_empty()
            || state.pending.allowed_agents.contains(agent_name);
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
        // TextInput layout: 2 borders + top pad + input + spacer + hint = 6 rows.
        Modal::TextInput { .. } => (60, 6),
        // Confirm height varies with prompt length (e.g. the mount-collapse
        // prompt lists each child/parent pair on its own line).
        Modal::Confirm { state, .. } => (60, confirm::required_height(state)),
        Modal::SaveDiscardCancel { .. } => (70, 7), // three buttons — a bit wider
        // File browser: compact overlay — 70% width, 22 rows (~20 visible
        // entries + banner + nav hint). Rows are an absolute count, not a
        // percentage — centered_rect_fixed takes rows for the height arg.
        Modal::FileBrowser { .. } => (70, 22),
        Modal::WorkdirPick { .. } => (60, 12), // ~6 choices + title + hint
        // Title bar + path + blank + explanation + blank + buttons + blank + hint = 9
        // plus 2 borders handled by centered_rect_fixed; widen to 80% so the
        // explanation sentence fits comfortably on one line.
        Modal::MountDstChoice { .. } => (80, 9),
        // GithubPicker: scale rows with repo count (choices + canonical
        // chrome: top pad + spacer + hint + 2 borders = 5), capped at 15
        // so a sprawling monorepo can't consume the viewport.
        Modal::GithubPicker { state } => {
            let rows = (state.choices.len() as u16).saturating_add(5).min(15);
            (60, rows)
        }
        // ConfirmSave: 80% width, height grows with line count. Clamped
        // to screen height by `centered_rect_fixed`.
        Modal::ConfirmSave { state } => (80, confirm_save::required_height(state).min(area.height)),
        // ErrorPopup: 60% width, word-wrapped message. Height capped at
        // 15 so even a novella error message can't blot out the screen.
        Modal::ErrorPopup { state } => {
            // Estimate inner width from outer width: 60% of frame, minus
            // 2 border columns, minus 2-column left gutter for safety.
            let inner_width = (area.width * 60 / 100).saturating_sub(4);
            (60, error_popup::required_height(state, inner_width))
        }
    };
    let modal_area = centered_rect_fixed(area, pct_w, height_rows);
    match modal {
        Modal::TextInput { state, .. } => text_input::render(frame, modal_area, state),
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => confirm::render(frame, modal_area, state),
        Modal::SaveDiscardCancel { state } => save_discard::render(frame, modal_area, state),
        Modal::MountDstChoice { state, .. } => {
            mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => github_picker::render(frame, modal_area, state),
        Modal::ConfirmSave { state } => confirm_save::render(frame, modal_area, state),
        Modal::ErrorPopup { state } => error_popup::render(frame, modal_area, state),
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
            FooterItem::Key("E"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("N"),
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
            FooterItem::Key("Q"),
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
            FooterItem::Key("E"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("new"),
            FooterItem::Sep,
            FooterItem::Key("D"),
            FooterItem::Text("delete"),
            FooterItem::GroupSep,
            FooterItem::Key("Q"),
            FooterItem::Text("quit"),
        ];
        let spans = footer_spans(&items);
        // Every Key should be styled WHITE + BOLD; count them.
        let key_count = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::WHITE))
            .count();
        assert_eq!(key_count, 6, "↑↓, Enter, E, N, D, Q");
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

    /// Return the character index of the start of the `Mode` column (i.e. the
    /// "M" in "Mode" for the header, or the first char of "ro"/"rw" for a data
    /// row). Both are found at: `"  " + path_w + "  "` — so the index equals
    /// `2 + path_w + 2` for a header and for data rows that have no selection
    /// prefix (and the selection prefix is always two chars too — "▸ " or
    /// "  " — so the column boundary is stable).
    fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
        let s = line_text(line);
        // The Mode column is the first two-letter "rw"/"ro" after the gap,
        // or the literal "Mode" for the header. Scan for the first non-space
        // character after the gap-of-two-spaces that follows the path.
        // Simpler: find the offset of the two-space gap before Mode.
        // Header: "  Path<pad>  Mode<pad>Type"
        // Data:   "  path<pad>  rw<pad>type"
        // In both cases the left edge of "Mode"/"rw" is exactly 2 + path_w + 2
        // from the start — we recover it by scanning for the first non-space
        // char at position >= 4 (past the left gutter + at least one path char).
        // Instead, just look for the substring "  M" (Mode header) or "  r"
        // (data row, always "rw"/"ro" starting with r).
        for (i, c) in s.chars().enumerate() {
            if i < 4 {
                continue;
            }
            if c == 'M' || c == 'r' {
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
        // Empty case: header should still render with the floor width and
        // include the two-space gap between "Mode" and "Type".
        let path_w = mount_path_width(&[]);
        assert_eq!(path_w, 10);
        let header = render_mount_header(path_w);
        // "  <path padded>  <mode padded>  Type"
        let expected = format!(
            "  {path:<path_w$}  {mode:<mw$}  Type",
            path = "Path",
            mode = "Mode",
            path_w = path_w,
            mw = MOUNT_MODE_COL_WIDTH,
        );
        let s = line_text(&header);
        assert_eq!(s, expected);
    }

    #[test]
    fn header_has_two_space_gap_between_mode_and_type() {
        // Regression for the "Mode Type" spacing bug: header must emit a
        // literal two-space gap between the `Mode` column and the `Type`
        // label, mirroring the gap data rows emit between `rw`/`ro` and the
        // kind. Additionally pins the type-column alignment: the `Type`
        // header label must start at the same character offset as the data
        // row's kind label (e.g. "folder") — without padding `rw`/`ro` to
        // the full width of "Mode", the kind column would render 2 chars
        // left of the header.
        let rows: Vec<(String, &str, String)> = vec![("~/p".into(), "rw", "folder".into())];
        let path_w = mount_path_width(&rows);
        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        let header_text = line_text(&header);
        let data_text = line_text(&data[0]);
        assert!(
            header_text.contains("Mode  Type"),
            "expected 'Mode  Type' (two spaces between Mode and Type); got {header_text:?}"
        );
        let header_type_offset = header_text.find("Type").expect("header has 'Type'");
        let data_kind_offset = data_text.find("folder").expect("data row has 'folder'");
        assert_eq!(
            header_type_offset, data_kind_offset,
            "Type column misaligned: header at {header_type_offset}, data at {data_kind_offset}"
        );
    }
}

#[cfg(test)]
mod contextual_row_items_tests {
    //! Row-specific footer-hint composition for the editor tabs.

    use super::{EditorState, FieldFocus, FooterItem, contextual_row_items};
    use crate::launch::manager::state::EditorTab;
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
mod mount_block_height_tests {
    //! Pins the Mounts sub-panel height formula shared by
    //! `render_details_pane` and `render_current_dir_details_pane`. Guards
    //! against the "phantom empty row" regression where a fixed
    //! `Constraint::Length(5)` over-allocated by 1 for a single-mount
    //! current-directory workspace.
    use super::mount_block_height;
    use crate::workspace::MountConfig;

    fn mount(path: &str) -> MountConfig {
        MountConfig {
            src: path.into(),
            dst: path.into(),
            readonly: false,
        }
    }

    #[test]
    fn empty_mounts_reserves_row_for_none_placeholder() {
        // 0 data rows + "(none)" placeholder (1 row) + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[]), 4);
    }

    #[test]
    fn single_mount_fits_in_four_rows() {
        // Regression: the current-dir pane used to hard-code `Length(5)`
        // which left an extra empty line inside the block. Correct total
        // for a 1-mount workspace is 1 data + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[mount("/tmp/a")]), 4);
    }

    #[test]
    fn multiple_mounts_scale_linearly() {
        assert_eq!(mount_block_height(&[mount("/tmp/a"), mount("/tmp/b")]), 5);
        assert_eq!(
            mount_block_height(&[mount("/a"), mount("/b"), mount("/c")]),
            6
        );
    }

    #[test]
    fn many_mounts_clamp_to_twelve() {
        let mounts: Vec<MountConfig> = (0..20).map(|i| mount(&format!("/m/{i}"))).collect();
        assert_eq!(mount_block_height(&mounts), 12);
    }
}

#[cfg(test)]
mod subpanel_padding_tests {
    //! Visual regression tests pinning the leading-padding convention shared
    //! by the General / Mounts / Agents sub-panels. All three render content
    //! rows starting at the same column so the first visible character of
    //! row 0 (i.e. the first row *inside* the block border) lines up across
    //! the three blocks, giving the right pane a tidy left edge.
    use super::{
        SUBPANEL_CONTENT_INDENT, render_agents_subpanel, render_general_subpanel,
        render_mounts_subpanel,
    };
    use crate::config::AppConfig;
    use crate::launch::manager::state::WorkspaceSummary;
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    /// Scan the first content row inside a sub-panel block (y = 1, skipping
    /// the top border at y = 0) for the first cell holding a printable
    /// non-space character, skipping the left vertical border. Returns the
    /// offset of that character *from the left border* — i.e. the indent —
    /// so values can be compared against `SUBPANEL_CONTENT_INDENT` directly.
    fn first_content_indent(terminal: &Terminal<TestBackend>) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        // Locate the left border column first so the returned value is the
        // relative indent, not the absolute column.
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, 1)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            let sym = buf[(x, 1)].symbol();
            if sym.is_empty() || sym == " " {
                continue;
            }
            return Some((x - border_x - 1) as usize);
        }
        None
    }

    fn summary() -> WorkspaceSummary {
        WorkspaceSummary {
            name: "demo".into(),
            workdir: "/tmp/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_agent_count: 0,
            default_agent: None,
            last_agent: None,
        }
    }

    fn ws_config_with_allowed(names: &[&str], default: Option<&str>) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: "/tmp/demo".into(),
            mounts: vec![],
            allowed_agents: names.iter().map(|s| (*s).into()).collect(),
            default_agent: default.map(String::from),
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    /// The first visible character of row 0 inside each sub-panel block
    /// must sit at the shared `SUBPANEL_CONTENT_INDENT`. Without the General
    /// block's two-space prefix the `w` of `workdir` rendered at column 1
    /// (flush with the border) while Mounts/Agents rendered at column 2.
    #[test]
    fn subpanel_content_column_alignment() {
        // General
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 40, 4), &summary());
        })
        .unwrap();
        let general_col = first_content_indent(&term).expect("general has content");

        // Mounts
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_mounts_subpanel(f, Rect::new(0, 0, 40, 4), &[]);
        })
        .unwrap();
        let mounts_col = first_content_indent(&term).expect("mounts has content");

        // Agents, "any agent" branch (no allowed list)
        let cfg = AppConfig::default();
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 4), None, &cfg);
        })
        .unwrap();
        let agents_any_col = first_content_indent(&term).expect("agents 'any' has content");

        assert_eq!(
            general_col, SUBPANEL_CONTENT_INDENT,
            "General first char at col {general_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            mounts_col, SUBPANEL_CONTENT_INDENT,
            "Mounts first char at col {mounts_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            agents_any_col, SUBPANEL_CONTENT_INDENT,
            "Agents (any) first char at col {agents_any_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
    }

    /// Scan row `y` inside a sub-panel block for the first cell whose
    /// symbol equals `needle`, returning the offset from the left border.
    /// Used to locate the trailing star glyph on a default-agent row.
    fn find_symbol_indent(terminal: &Terminal<TestBackend>, y: u16, needle: &str) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            if buf[(x, y)].symbol() == needle {
                return Some((x - border_x - 1) as usize);
            }
        }
        None
    }

    /// Scan row `y` for the last printable non-space/border cell and
    /// return its relative offset from the left border. Used to confirm
    /// a non-default row has no trailing suffix past the name.
    fn last_printable_indent(terminal: &Terminal<TestBackend>, y: u16) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let right_border_x = ((border_x + 1)..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let mut last: Option<usize> = None;
        for x in (border_x + 1)..right_border_x {
            let sym = buf[(x, y)].symbol();
            if !sym.is_empty() && sym != " " {
                last = Some((x - border_x - 1) as usize);
            }
        }
        last
    }

    /// Non-default agent rows render the name starting at
    /// `SUBPANEL_CONTENT_INDENT` (col 2 from the border). With the
    /// trailing-star convention no glyph precedes the name.
    ///
    /// After the "Last used" relocation, the Agents sub-panel lays out:
    ///   y=0 top border
    ///   y=1 `  Last used   …`
    ///   y=2 blank spacer
    ///   y=3 first agent row (here: alpha, the default)
    ///   y=4 second agent row (here: beta, the non-default)
    #[test]
    fn agents_subpanel_non_default_agent_name_starts_at_col_2() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());
        cfg.agents
            .insert("beta".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 7);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 7), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the beta row (y=4).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on beta row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("beta row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "non-default agent name should start at col {SUBPANEL_CONTENT_INDENT}, got {name_col}"
        );

        // And there must be no trailing star on the non-default row.
        let last_col = last_printable_indent(&term, 4).expect("beta row has content");
        // `beta` is 4 chars starting at col 2 ⇒ last printable at col 5.
        // A trailing star would push last_col to col 7 (space + star).
        assert_eq!(
            last_col,
            SUBPANEL_CONTENT_INDENT + "beta".len() - 1,
            "non-default agent row must have no trailing suffix past the name",
        );
    }

    /// Default agent row carries a trailing star glyph positioned after
    /// the agent name (separated by a space), not a leading star.
    ///
    /// Agents sub-panel layout after the `Last used` relocation: top
    /// border at y=0, `Last used` at y=1, blank at y=2, first agent row
    /// at y=3. For a single-allowed workspace that agent IS the default.
    #[test]
    fn agents_subpanel_default_agent_has_trailing_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        let star_col = find_symbol_indent(&term, 3, "\u{2605}")
            .expect("default agent row should contain a star glyph");
        let expected = SUBPANEL_CONTENT_INDENT + "alpha".len() + 1;
        assert_eq!(
            star_col, expected,
            "default agent star should trail the name at col {expected}, got {star_col}"
        );
    }

    /// Default agent row's name column matches non-default rows (and the
    /// `SUBPANEL_CONTENT_INDENT` convention). The trailing star must not
    /// shift the name right.
    ///
    /// `first_content_indent` scans y=1; that's now the `Last used` row,
    /// whose label also starts at `SUBPANEL_CONTENT_INDENT`. The invariant
    /// the test pins (every content row starts at col 2) still holds —
    /// what we're confirming is that the block's leading indent is
    /// consistent. We check the agent row explicitly to guard against the
    /// trailing-star breaking the name-column alignment.
    #[test]
    fn agents_subpanel_default_agent_name_starts_at_col_2_regardless_of_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the alpha row (y=3).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on alpha row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("alpha row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "default agent name should start at col {SUBPANEL_CONTENT_INDENT} even with the trailing star, got {name_col}"
        );
    }

    // ── Last-used relocation: General → Agents sub-panel ───────────────

    /// The General sub-panel no longer shows `Last used` — it only renders
    /// `Working dir`. Guards against a regression that reintroduces the row
    /// and grows the block back to 4 rows.
    #[test]
    fn general_subpanel_no_longer_shows_last_used() {
        let mut s = summary();
        s.last_agent = Some("alpha".into());

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 60, 4), &s);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "General sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    /// The Agents sub-panel renders `Last used   <agent>` at the top, above
    /// the blank spacer and the allow list.
    #[test]
    fn agents_subpanel_shows_last_used_at_top() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.last_agent = Some("beta".into());
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(60, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 6), Some(&ws), &cfg);
        })
        .unwrap();

        // y=1 is the first content row — expect `Last used   beta`.
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, 1)].symbol());
        }
        assert!(
            row.contains("Last used"),
            "Agents row 1 must hold `Last used`; got {row:?}"
        );
        assert!(
            row.contains("beta"),
            "Agents row 1 must hold the last-used agent name; got {row:?}"
        );
    }

    /// When `last_agent` is `None`, the Last-used row displays the
    /// `(none)` placeholder. Pinned separately so the phrasing is stable.
    #[test]
    fn last_used_none_renders_placeholder() {
        let ws = ws_config_with_allowed(&[], None); // last_agent defaults to None
        let cfg = AppConfig::default();

        let backend = TestBackend::new(60, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 6), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, 1)].symbol());
        }
        assert!(
            row.contains("Last used"),
            "Agents row 1 must hold `Last used` even when the value is (none); got {row:?}"
        );
        assert!(
            row.contains("(none)"),
            "Last-used placeholder should be `(none)`; got {row:?}"
        );
    }

    /// Current-directory pane also shows the Last-used row in its Agents
    /// block for structural consistency, even though the synthetic cwd
    /// workspace always has `last_agent = None` → renders `(none)`.
    /// Passing `ws_config = None` exercises the same code path the
    /// current-dir pane uses.
    #[test]
    fn current_dir_agents_subpanel_shows_last_used_none() {
        let cfg = AppConfig::default();

        let backend = TestBackend::new(60, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 6), None, &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, 1)].symbol());
        }
        assert!(
            row.contains("Last used") && row.contains("(none)"),
            "current-dir Agents block must still show `Last used   (none)`; got {row:?}"
        );
    }
}

#[cfg(test)]
mod header_branding_tests {
    //! Pins the product-name rendering convention: the top-of-screen
    //! header must display the name as lowercase + trailing apostrophe
    //! (`jackin'`) in every user-facing string. All-caps `JACKIN` and
    //! apostrophe-less `jackin` are both disallowed for display text —
    //! though `jackin` without an apostrophe still appears in CLI-command
    //! references rendered in backticks (e.g. `` `jackin launch` ``), in
    //! filesystem paths like `~/.jackin/`, and in URLs, all of which are
    //! intentionally exempt and not audited here.
    use super::render_header;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn tui_header_uses_lowercase_jackin_with_apostrophe() {
        let backend = TestBackend::new(40, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_header(f, Rect::new(0, 0, 40, 1), "workspaces");
        })
        .unwrap();

        let buf = term.backend().buffer();
        let dump: String = buf.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            dump.contains("jackin'"),
            "header must render 'jackin'' (lowercase + trailing apostrophe); got {dump:?}"
        );
        assert!(
            !dump.contains("JACKIN"),
            "header must not render 'JACKIN' (uppercase); got {dump:?}"
        );
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
    use crate::launch::manager::state::{EditorState, EditorTab, FieldFocus};
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
