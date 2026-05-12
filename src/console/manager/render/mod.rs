//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::state::{ManagerListRow, ManagerStage, ManagerState};
use crate::config::AppConfig;

pub mod editor;
pub(super) mod global_mounts;
pub(super) mod list;
pub(super) mod modal;

// Re-export the shared modal geometry helper so `manager::input::mouse` can
// reach it via `super::super::render::modal_outer_rect`.
pub(super) use modal::modal_outer_rect;
// Re-export the editor entry point so input handlers can redraw the editor
// while a modal is being dismissed (see `input::mod`).
pub use editor::render_editor;

pub(super) use crate::console::widgets::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

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

/// How many rows the footer needs to display all `items` within `width` columns.
/// Minimum 1. Callers use this to size the footer area before running layout.
#[must_use]
pub(super) fn footer_height(items: &[FooterItem], width: u16) -> u16 {
    footer_lines(items, width).len().max(1) as u16
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect, items: &[FooterItem]) {
    let lines = footer_lines(items, area.width);
    let p = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

/// Pack footer items into wrapped lines that fit within `width` columns.
///
/// Items are first split into "chunks" at every `Sep` and `GroupSep` boundary.
/// Chunks are then greedily packed onto lines: if the next chunk (plus a
/// separator) would overflow the line, it starts a new line. A `GroupSep`
/// between two adjacent chunks on the same line renders as three spaces;
/// a `Sep` renders as ` · `. Both separators take 3 columns.
fn footer_lines(items: &[FooterItem], width: u16) -> Vec<Line<'static>> {
    // A chunk = one logical hint unit (key + optional label), with the separator
    // flavor that should precede it when it follows another chunk on the same line.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SepKind {
        Group,
        Dot,
    }

    struct Chunk {
        spans: Vec<Span<'static>>,
        width: usize,
        sep: SepKind,
    }

    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let dyn_style = Style::default().fg(PHOSPHOR_DIM);

    // Build chunks by accumulating spans until a Sep or GroupSep is hit.
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut cur_spans: Vec<Span<'static>> = Vec::new();
    let mut cur_w: usize = 0;
    let mut next_sep = SepKind::Group;

    let flush =
        |chunks: &mut Vec<Chunk>, spans: &mut Vec<Span<'static>>, w: &mut usize, sep: SepKind| {
            if !spans.is_empty() {
                chunks.push(Chunk {
                    spans: std::mem::take(spans),
                    width: *w,
                    sep,
                });
                *w = 0;
            }
        };

    for item in items {
        match item {
            FooterItem::Key(k) => {
                cur_w += k.chars().count();
                cur_spans.push(Span::styled((*k).to_string(), key_style));
            }
            FooterItem::Text(t) => {
                cur_w += 1 + t.chars().count();
                cur_spans.push(Span::styled(format!(" {t}"), text_style));
            }
            FooterItem::Dyn(t) => {
                cur_w += 1 + t.chars().count();
                cur_spans.push(Span::styled(format!(" {t}"), dyn_style));
            }
            FooterItem::Sep => {
                flush(&mut chunks, &mut cur_spans, &mut cur_w, next_sep);
                next_sep = SepKind::Dot;
            }
            FooterItem::GroupSep => {
                flush(&mut chunks, &mut cur_spans, &mut cur_w, next_sep);
                next_sep = SepKind::Group;
            }
        }
    }
    flush(&mut chunks, &mut cur_spans, &mut cur_w, next_sep);

    // Greedy line-packing: chunks go on the current line if they fit;
    // otherwise start a new line. Separator costs 3 columns on same line.
    let max_w = width as usize;

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut line_spans: Vec<Span<'static>> = Vec::new();
    let mut line_w: usize = 0;

    for chunk in &chunks {
        let needed = if line_spans.is_empty() {
            chunk.width
        } else {
            3 + chunk.width
        };

        if !line_spans.is_empty() && line_w + needed > max_w {
            lines.push(Line::from(std::mem::take(&mut line_spans)));
            line_w = 0;
        }

        if !line_spans.is_empty() {
            match chunk.sep {
                SepKind::Dot => line_spans.push(Span::styled(" \u{b7} ".to_string(), sep_style)),
                SepKind::Group => line_spans.push(Span::raw("   ")),
            }
            line_w += 3;
        }

        line_spans.extend(chunk.spans.iter().cloned());
        line_w += chunk.width;
    }

    if !line_spans.is_empty() {
        lines.push(Line::from(line_spans));
    }

    if lines.is_empty() {
        lines.push(Line::raw(""));
    }

    lines
}

pub(super) fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum()
}

#[cfg(test)]
mod footer_wrap_tests {
    use super::*;

    fn text_content(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn short_footer_fits_on_one_line() {
        let items = vec![
            FooterItem::Key("S"),
            FooterItem::Text("save"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("back"),
        ];
        let lines = footer_lines(&items, 80);
        assert_eq!(lines.len(), 1, "should fit on one line at 80 cols");
    }

    #[test]
    fn long_footer_wraps_to_two_lines() {
        // Construct items that definitely exceed a narrow terminal.
        let items = vec![
            FooterItem::Key("↑↓"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("D"),
            FooterItem::Text("remove"),
            FooterItem::Sep,
            FooterItem::Key("A"),
            FooterItem::Text("add"),
            FooterItem::Sep,
            FooterItem::Key("R"),
            FooterItem::Text("toggle ro/rw"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("rename"),
            FooterItem::GroupSep,
            FooterItem::Key("Tab"),
            FooterItem::Text("switch tab"),
            FooterItem::GroupSep,
            FooterItem::Key("S"),
            FooterItem::Text("save settings"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("back"),
        ];
        let lines = footer_lines(&items, 60);
        assert!(lines.len() > 1, "should wrap at 60 cols; lines={lines:?}");
        // Every line should fit within 60 chars.
        for line in &lines {
            let w = line_width(line);
            assert!(w <= 60, "line width {w} exceeds 60 cols: {line:?}");
        }
    }

    #[test]
    fn footer_height_matches_line_count() {
        let items = vec![FooterItem::Key("S"), FooterItem::Text("save")];
        assert_eq!(footer_height(&items, 80), 1);
    }

    #[test]
    fn empty_items_produce_one_blank_line() {
        let lines = footer_lines(&[], 80);
        assert_eq!(lines.len(), 1);
        let content = text_content(&lines);
        assert_eq!(content[0], "");
    }
}

pub(super) fn max_line_width(lines: &[Line<'_>]) -> usize {
    lines.iter().map(line_width).max().unwrap_or(0)
}

pub(super) fn render_horizontal_scrollbar(
    frame: &mut Frame,
    block_area: Rect,
    content_width: usize,
    scroll_x: u16,
) {
    let viewport = block_area.width.saturating_sub(2) as usize;
    if viewport == 0 || content_width <= viewport {
        return;
    }
    let position = scrollbar_position(content_width, viewport, scroll_x);
    let mut state = ScrollbarState::new(content_width)
        .position(position)
        .viewport_content_length(viewport);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("·"))
        .thumb_symbol("━")
        .track_style(Style::default().fg(PHOSPHOR_DARK))
        .thumb_style(Style::default().fg(PHOSPHOR_DIM));
    let area = Rect {
        x: block_area.x + 1,
        y: block_area.y + block_area.height.saturating_sub(1),
        width: block_area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

pub(super) fn effective_scroll_x(content_width: usize, viewport: usize, scroll_x: u16) -> u16 {
    if viewport == 0 || content_width <= viewport {
        0
    } else {
        scroll_x.min(
            content_width
                .saturating_sub(viewport)
                .min(usize::from(u16::MAX)) as u16,
        )
    }
}

pub(super) fn effective_scroll_y(content_height: usize, viewport_h: usize, scroll_y: u16) -> u16 {
    if viewport_h == 0 || content_height <= viewport_h {
        0
    } else {
        scroll_y.min(
            content_height
                .saturating_sub(viewport_h)
                .min(usize::from(u16::MAX)) as u16,
        )
    }
}

pub(super) fn clamp_scroll_x(content_width: usize, viewport: usize, scroll_x: &mut u16) -> u16 {
    let effective = effective_scroll_x(content_width, viewport, *scroll_x);
    *scroll_x = effective;
    effective
}

/// Adjust stored `scroll_y` so the cursor row stays inside the viewport.
/// Returns the effective (clamped, cursor-following) `scroll_y` to use for rendering.
pub(super) fn follow_cursor_y(
    cursor: usize,
    content_height: usize,
    viewport_h: usize,
    stored_scroll_y: u16,
) -> u16 {
    if viewport_h == 0 {
        return 0;
    }
    let max_scroll = content_height.saturating_sub(viewport_h);
    let raw = if cursor < stored_scroll_y as usize {
        cursor as u16
    } else if content_height > viewport_h && cursor >= stored_scroll_y as usize + viewport_h {
        (cursor + 1 - viewport_h) as u16
    } else {
        stored_scroll_y
    };
    raw.min(max_scroll as u16)
}

/// Adjust `scroll_y` so `cursor` stays in the editor/settings content viewport.
///
/// The chrome constant 9 = header 3 + tab strip 2 + footer 2 + block borders 2.
/// `usize::MAX` is passed as `content_height` because the rendered line count is
/// not known at input-dispatch time; it is large enough that `follow_cursor_y`'s
/// upper clamp (`raw.min(max_scroll)`) never fires — the `as u16` truncation in
/// the caller means the effective ceiling is 65 535, well above any real viewport.
pub(super) fn cursor_scroll_for_panel(
    cursor: usize,
    scroll_y: u16,
    term: ratatui::layout::Rect,
) -> u16 {
    let viewport_h = (term.height.saturating_sub(9) as usize).max(1);
    follow_cursor_y(cursor, usize::MAX, viewport_h, scroll_y)
}

pub(super) fn render_vertical_scrollbar(
    frame: &mut Frame,
    block_area: Rect,
    content_height: usize,
    scroll_y: u16,
) {
    let viewport = block_area.height.saturating_sub(2) as usize;
    if viewport == 0 || content_height <= viewport {
        return;
    }
    let position = scrollbar_position(content_height, viewport, scroll_y);
    let mut state = ScrollbarState::new(content_height)
        .position(position)
        .viewport_content_length(viewport);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("·"))
        .thumb_symbol("█")
        .track_style(Style::default().fg(PHOSPHOR_DARK))
        .thumb_style(Style::default().fg(PHOSPHOR_DIM));
    let area = Rect {
        x: block_area.x + block_area.width.saturating_sub(1),
        y: block_area.y + 1,
        width: 1,
        height: block_area.height.saturating_sub(2),
    };
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

/// Render lines inside a bordered scrollable block.
///
/// Border is `PHOSPHOR_GREEN` when `focused`, `PHOSPHOR_DARK` otherwise.
/// Optional `title` renders `WHITE + BOLD` in the top border.
/// Clamps `*scroll_x` and `*scroll_y` to their effective maximums in-place
/// so callers never accumulate stale overshoot from past scroll events.
pub(super) fn render_scrollable_block(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: &mut u16,
    scroll_y: &mut u16,
    focused: bool,
    title: Option<&str>,
) {
    let border_color = if focused {
        PHOSPHOR_GREEN
    } else {
        PHOSPHOR_DARK
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    if let Some(t) = title {
        block = block.title(Span::styled(
            t,
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    let content_width = max_line_width(&lines);
    let content_height = lines.len();
    let viewport_w = area.width.saturating_sub(2) as usize;
    let viewport_h = area.height.saturating_sub(2) as usize;
    let eff_x = effective_scroll_x(content_width, viewport_w, *scroll_x);
    let eff_y = effective_scroll_y(content_height, viewport_h, *scroll_y);
    *scroll_x = eff_x;
    *scroll_y = eff_y;
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(PHOSPHOR_GREEN))
            .scroll((eff_y, eff_x)),
        area,
    );
    render_horizontal_scrollbar(frame, area, content_width, eff_x);
    render_vertical_scrollbar(frame, area, content_height, eff_y);
}

fn scrollbar_position(content_width: usize, viewport: usize, scroll_x: u16) -> usize {
    let scroll_x = usize::from(effective_scroll_x(content_width, viewport, scroll_x));
    let max_scroll = content_width.saturating_sub(viewport);
    scroll_x
        .saturating_mul(content_width.saturating_sub(1))
        .checked_div(max_scroll)
        .unwrap_or(0)
}

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let area = frame.area();
    state.cached_term_size = area;
    if let ManagerStage::Editor(editor) = &mut state.stage {
        clamp_editor_scroll_for_frame(area, editor);
        editor::render_editor(frame, editor, config, state.op_available);
    } else if let ManagerStage::Settings(settings) = &mut state.stage {
        clamp_global_mounts_scroll_for_frame(area, &mut settings.mounts);
        global_mounts::render_settings(frame, settings, state.op_available);
    } else {
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
            clamp_list_scroll_for_area(chunks[1], state, config, cwd);
            list::render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<FooterItem> = match &state.stage {
            ManagerStage::List => {
                if state.inline_agent_picker.is_some() {
                    let mut items = vec![
                        FooterItem::Key("\u{2191}\u{2193}"),
                        FooterItem::Sep,
                        FooterItem::Key("Enter"),
                        FooterItem::Text("launch"),
                        FooterItem::GroupSep,
                        FooterItem::Key("Esc"),
                        FooterItem::Text("return to workspaces"),
                    ];
                    if state.list_scroll_focus.is_some() {
                        items.push(FooterItem::GroupSep);
                        items.push(FooterItem::Key("←/→"));
                        items.push(FooterItem::Text("scroll block"));
                    }
                    items
                } else if state.inline_role_picker.is_some() {
                    let mut items = vec![
                        FooterItem::Key("\u{2191}\u{2193}"),
                        FooterItem::Sep,
                        FooterItem::Key("Enter"),
                        FooterItem::Text("launch"),
                        FooterItem::GroupSep,
                        FooterItem::Key("Esc"),
                        FooterItem::Text("return to workspaces"),
                    ];
                    if state.list_scroll_focus.is_some() {
                        items.push(FooterItem::GroupSep);
                        items.push(FooterItem::Key("←/→"));
                        items.push(FooterItem::Text("scroll block"));
                    }
                    items.push(FooterItem::GroupSep);
                    items.push(FooterItem::Key("Q"));
                    items.push(FooterItem::Text("quit"));
                    items
                } else {
                    // Hidden on current-dir and "+ New workspace" rows because
                    // they have no workspace config.
                    let show_open_hint =
                        matches!(state.selected_row(), ManagerListRow::SavedWorkspace(_))
                            && state
                                .selected_workspace_summary()
                                .and_then(|s| config.workspaces.get(&s.name))
                                .is_some_and(|ws| {
                                    !super::github_mounts::resolve_for_workspace(ws).is_empty()
                                });

                    let is_saved =
                        matches!(state.selected_row(), ManagerListRow::SavedWorkspace(_));
                    let scroll_focused = state.list_scroll_focus.is_some();

                    // When a scrollable block is active, ↑↓/←→ scroll it.
                    // When no block is focused, ↑↓ navigate the workspace list.
                    let mut items: Vec<FooterItem> = if scroll_focused {
                        vec![
                            FooterItem::Key("\u{2191}\u{2193}/\u{2190}\u{2192}"),
                            FooterItem::Text("scroll block"),
                            FooterItem::GroupSep,
                            FooterItem::Key("Enter"),
                            FooterItem::Text("launch"),
                            FooterItem::GroupSep,
                        ]
                    } else {
                        vec![
                            FooterItem::Key("\u{2191}\u{2193}"),
                            FooterItem::Sep,
                            FooterItem::Key("Enter"),
                            FooterItem::Text("launch"),
                            FooterItem::GroupSep,
                        ]
                    };
                    if is_saved {
                        items.extend([
                            FooterItem::Key("E"),
                            FooterItem::Text("edit"),
                            FooterItem::Sep,
                        ]);
                    }
                    items.extend([FooterItem::Key("N"), FooterItem::Text("new")]);
                    if is_saved {
                        items.extend([
                            FooterItem::Sep,
                            FooterItem::Key("D"),
                            FooterItem::Text("delete"),
                        ]);
                    }
                    items.extend([
                        FooterItem::Sep,
                        FooterItem::Key("S"),
                        FooterItem::Text("settings"),
                    ]);
                    if show_open_hint {
                        items.push(FooterItem::Sep);
                        items.push(FooterItem::Key("O"));
                        items.push(FooterItem::Text("open in GitHub"));
                    }
                    items.push(FooterItem::GroupSep);
                    items.push(FooterItem::Key("Q"));
                    items.push(FooterItem::Text("quit"));
                    items
                }
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
            ManagerStage::Settings(_) => unreachable!("Settings has its own render path"),
        };
        render_footer(frame, chunks[2], &footer_items);
    }

    // List-anchored modal lives on `ManagerState`, not on a stage
    // variant, so the borrow splits separately from stage-anchored
    // modals.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &mut state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &mut state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &mut editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &mut prelude.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = centered_rect_fixed(area, 60, 7);
                super::super::widgets::confirm::render(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
            ManagerStage::Settings(settings) => {
                if let Some(modal) = &mut settings.mounts.modal {
                    global_mounts::render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &mut settings.env.modal {
                    global_mounts::render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &mut settings.auth.modal {
                    global_mounts::render_settings_auth_modal(frame, modal);
                }
            }
        }
    }
}

fn clamp_editor_scroll_for_frame(area: Rect, editor: &mut super::state::EditorState<'_>) {
    if editor.active_tab != super::state::EditorTab::Mounts {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);
    clamp_scroll_x(
        list::workspace_mounts_content_width(&editor.pending.mounts),
        chunks[2].width.saturating_sub(2) as usize,
        &mut editor.workspace_mounts_scroll_x,
    );
}

fn clamp_global_mounts_scroll_for_frame(
    area: Rect,
    global: &mut super::state::GlobalMountsState<'_>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    clamp_scroll_x(
        global_mounts::global_mounts_content_width(&global.pending),
        chunks[2].width.saturating_sub(2) as usize,
        &mut global.scroll_x,
    );
}

fn clamp_list_scroll_for_area(
    area: Rect,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let viewport = columns[1].width.saturating_sub(2) as usize;

    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            let cwd = cwd.display().to_string();
            let mounts = [crate::workspace::MountConfig {
                src: cwd.clone(),
                dst: cwd,
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }];
            clamp_scroll_x(
                list::workspace_mounts_content_width(&mounts),
                viewport,
                &mut state.list_mounts_scroll_x,
            );
            state.list_global_mounts_scroll_x = 0;
            state.list_role_global_mounts_scroll_x = 0;
        }
        ManagerListRow::SavedWorkspace(i) => {
            let Some(summary) = state.workspaces.get(i) else {
                return;
            };
            let Some(workspace) = config.workspaces.get(&summary.name) else {
                return;
            };
            clamp_scroll_x(
                list::workspace_mounts_content_width(&workspace.mounts),
                viewport,
                &mut state.list_mounts_scroll_x,
            );
            let picker_role = state.inline_role_picker.as_ref().and_then(|picker| {
                picker
                    .list_state
                    .selected
                    .and_then(|idx| picker.filtered.get(idx).cloned())
            });
            let global_rows = global_rows_for(config, picker_role.as_ref());
            let (global, scoped) = partition_mounts_by_scope(&global_rows);
            clamp_scroll_x(
                list::global_mounts_content_width(&global),
                viewport,
                &mut state.list_global_mounts_scroll_x,
            );
            clamp_scroll_x(
                list::global_mounts_content_width(&scoped),
                viewport,
                &mut state.list_role_global_mounts_scroll_x,
            );
        }
        ManagerListRow::NewWorkspace => {
            state.list_mounts_scroll_x = 0;
            state.list_global_mounts_scroll_x = 0;
            state.list_role_global_mounts_scroll_x = 0;
        }
    }

    // Fix 1: Clear stale scroll focus when the focused block no longer
    // overflows after a terminal resize. Checked every render frame so the
    // green border disappears as soon as the content fits in the viewport.
    if state
        .list_scroll_focus
        .is_some_and(|f| !focused_block_still_scrollable(f, columns[1], state, config, cwd))
    {
        state.list_scroll_focus = None;
    }

    // Clamp left-pane name scroll to valid range.
    let left_viewport_w = columns[0].width.saturating_sub(2) as usize;
    if left_viewport_w == 0 {
        state.list_names_scroll_x = 0;
    } else {
        let name_content_w = list_names_content_width(state);
        if name_content_w <= left_viewport_w {
            state.list_names_scroll_x = 0;
            state.list_names_focused = false;
        } else {
            let max = (name_content_w - left_viewport_w) as u16;
            if state.list_names_scroll_x > max {
                state.list_names_scroll_x = max;
            }
        }
    }
}

/// Compute the maximum content width of the left-pane workspace name list.
fn list_names_content_width(state: &ManagerState<'_>) -> usize {
    // Each row: "▸ " (2) + name. "Current directory" = 17, "+ New workspace" = 15.
    let cwd_w = 2 + "Current directory".len();
    let sentinel_w = 2 + "+ New workspace".len();
    let max_ws = state
        .workspaces
        .iter()
        .map(|w| 2 + w.name.len())
        .max()
        .unwrap_or(0);
    cwd_w.max(sentinel_w).max(max_ws)
}

fn workspace_mounts_scrollable(
    mounts: &[crate::workspace::MountConfig],
    viewport_w: usize,
) -> bool {
    let w = list::workspace_mounts_content_width(mounts);
    let data_rows: usize = mounts
        .iter()
        .map(|m| if m.src == m.dst { 1 } else { 2 })
        .sum();
    let content_h = 1 + data_rows.max(1);
    let viewport_h = list::mount_block_height(mounts) as usize - 2;
    w > viewport_w || content_h > viewport_h
}

/// Returns `true` when the focused block still overflows the right pane
/// (either horizontally or vertically) after a resize. Used to clear
/// `list_scroll_focus` when the terminal grows large enough that the
/// content fits without scrolling.
fn focused_block_still_scrollable(
    focus: super::state::MountScrollFocus,
    right_pane: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> bool {
    use super::state::{ManagerListRow, MountScrollFocus};
    let viewport_w = right_pane.width.saturating_sub(2) as usize;

    match focus {
        MountScrollFocus::Workspace => match state.selected_row() {
            ManagerListRow::CurrentDirectory => {
                let cwd_str = cwd.display().to_string();
                let m = crate::workspace::MountConfig {
                    src: cwd_str.clone(),
                    dst: cwd_str,
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                };
                workspace_mounts_scrollable(std::slice::from_ref(&m), viewport_w)
            }
            ManagerListRow::SavedWorkspace(i) => {
                let Some(s) = state.workspaces.get(i) else {
                    return false;
                };
                let Some(ws) = config.workspaces.get(&s.name) else {
                    return false;
                };
                workspace_mounts_scrollable(ws.mounts.as_slice(), viewport_w)
            }
            ManagerListRow::NewWorkspace => false,
        },
        MountScrollFocus::Global | MountScrollFocus::RoleGlobal => {
            // Global mounts change rarely; treat as always scrollable when focused
            // to avoid computing the full mount list width here.
            true
        }
        MountScrollFocus::Roles => {
            let ws_config = match state.selected_row() {
                ManagerListRow::SavedWorkspace(i) => state
                    .workspaces
                    .get(i)
                    .and_then(|s| config.workspaces.get(&s.name)),
                ManagerListRow::CurrentDirectory | ManagerListRow::NewWorkspace => None,
            };
            let agent_count = list::agents_block_agent_count(ws_config, config);
            let roles_w = list::agents_block_content_width(ws_config, config);
            let roles_h = 2 + agent_count;
            let block_h = list::agents_block_height(agent_count) as usize;
            let viewport_h = block_h.saturating_sub(2);
            roles_w > viewport_w || roles_h > viewport_h
        }
    }
}

/// `None` role → unscoped rows only; `Some(role)` → merged scoped + unscoped.
pub(super) fn global_rows_for(
    config: &AppConfig,
    picker_role: Option<&crate::selector::RoleSelector>,
) -> Vec<crate::config::GlobalMountRow> {
    picker_role.map_or_else(
        || {
            config
                .list_mount_rows()
                .into_iter()
                .filter(|row| row.scope.is_none())
                .collect()
        },
        |role| config.resolve_mount_rows(role),
    )
}

pub(super) fn partition_mounts_by_scope(
    rows: &[crate::config::GlobalMountRow],
) -> (
    Vec<crate::workspace::MountConfig>,
    Vec<crate::workspace::MountConfig>,
) {
    let mut global = Vec::new();
    let mut scoped = Vec::new();
    for row in rows {
        if row.scope.is_none() {
            global.push(row.mount.clone());
        } else {
            scoped.push(row.mount.clone());
        }
    }
    (global, scoped)
}

pub(super) fn render_header(frame: &mut Frame, area: Rect, title: &str) {
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

/// Like `centered_rect` but takes a fixed number of rows for the height.
/// `pct_w` is still a percentage of the outer width. Rows are clamped to fit.
pub(super) fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
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
mod horizontal_scrollbar_tests {
    use super::{clamp_scroll_x, scrollbar_position};

    #[test]
    fn text_scroll_end_maps_to_scrollbar_end() {
        assert_eq!(scrollbar_position(100, 60, 0), 0);
        assert_eq!(scrollbar_position(100, 60, 20), 49);
        assert_eq!(scrollbar_position(100, 60, 40), 99);
    }

    #[test]
    fn scrollbar_position_clamps_overscroll_to_end() {
        assert_eq!(scrollbar_position(100, 60, 400), 99);
    }

    #[test]
    fn stored_scroll_offset_clamps_to_visible_end() {
        let mut scroll_x = 400;

        let effective = clamp_scroll_x(100, 60, &mut scroll_x);

        assert_eq!(effective, 40);
        assert_eq!(scroll_x, 40);

        scroll_x = scroll_x.saturating_sub(8);
        assert_eq!(scroll_x, 32);
    }
}

#[cfg(test)]
mod footer_tests {
    use super::{FOOTER_KEY, FOOTER_SEP, FOOTER_TEXT, FooterItem, footer_lines};

    // Use a wide terminal width so items stay on one line in these unit tests.
    const WIDE: u16 = 200;

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
        let lines = footer_lines(&items, WIDE);
        let spans = &lines[0].spans;
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
        let lines = footer_lines(&items, WIDE);
        let spans = &lines[0].spans;
        // spans: [E, edit, " · ", N, new]
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
        let lines = footer_lines(&items, WIDE);
        let spans = &lines[0].spans;
        // spans: [Enter, launch, "   ", Q, quit]
        assert_eq!(spans[2].content.as_ref(), "   ");
        assert_eq!(spans[2].style.fg, None);
    }

    #[test]
    fn dyn_item_uses_phosphor_dim() {
        let items = vec![FooterItem::Dyn("3 changes".to_string())];
        let lines = footer_lines(&items, WIDE);
        let spans = &lines[0].spans;
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
        let lines = footer_lines(&items, WIDE);
        let spans: Vec<_> = lines.iter().flat_map(|l| l.spans.iter()).collect();
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
        let lines = footer_lines(&items, WIDE);
        let spans: Vec<_> = lines.iter().flat_map(|l| l.spans.iter()).collect();
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
mod header_branding_tests {
    //! Pins the product-name rendering convention: the top-of-screen
    //! header must display the name as lowercase + trailing apostrophe
    //! (`jackin'`) in every user-facing string. All-caps `JACKIN` and
    //! apostrophe-less `jackin` are both disallowed for display text —
    //! though `jackin` without an apostrophe still appears in CLI-command
    //! references rendered in backticks (e.g. `` `jackin console` ``), in
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
        let dump: String = buf
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

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
