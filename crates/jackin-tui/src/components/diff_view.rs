//! Read-only diff view component used by the D24 Inspect surface.
//!
//! Two modes: side-by-side (modified files, before │ after) and single-pane
//! (added / untracked / deleted). Uses `similar::TextDiff` for hunk computation.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use similar::{ChangeTag, TextDiff};

use crate::components::scrollable_panel::{effective_offset, render_scrollable_block_at};
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN};

/// Background colour for removed lines in side-by-side mode.
const DIFF_REMOVED_BG: Color = Color::Rgb(60, 20, 20);
/// Background colour for added lines in side-by-side mode.
const DIFF_ADDED_BG: Color = Color::Rgb(20, 50, 20);

/// A single paired row in side-by-side mode.
/// `None` on either side means the other side had no matching counterpart.
#[derive(Debug, Clone)]
struct SideBySideRow {
    left: Option<(ChangeTag, String)>,
    right: Option<(ChangeTag, String)>,
}

/// Pre-computed rows ready for rendering.
#[derive(Debug, Clone)]
enum DiffRows {
    SideBySide {
        rows: Vec<SideBySideRow>,
        before_label: String,
        after_label: String,
    },
    SinglePane {
        lines: Vec<(ChangeTag, String)>,
        label: String,
    },
}

/// Variant for single-pane mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinglePaneKind {
    Added,
    Deleted,
    Untracked,
}

/// State for the `diff_view` component.
///
/// Construct with [`DiffViewState::side_by_side`] (modified files) or
/// [`DiffViewState::single_pane`] (added / deleted / untracked). Call
/// [`render_diff_view`] each frame; use [`DiffViewState::scroll_up`] /
/// [`DiffViewState::scroll_down`] to update scroll from key events.
#[derive(Debug, Clone)]
pub struct DiffViewState {
    rows: DiffRows,
    /// Private so the only mutators are the scroll methods and the in-module
    /// render clamp — external callers go through [`DiffViewState::scroll_y`] /
    /// [`DiffViewState::set_scroll_y`] rather than writing the field directly.
    scroll_y: u16,
}

impl DiffViewState {
    /// Compute a side-by-side diff between `before` and `after`.
    ///
    /// Equal context lines appear on both sides; removed lines land on the
    /// left with an empty right; added lines land on the right with an empty left.
    #[must_use]
    #[allow(
        clippy::excessive_nesting,
        reason = "Diff-pairing state machine with per-change-tag (Equal / Delete \
                  / Insert) branching, paired-row flushing, and equal-block \
                  transition tracking. The state machine nesting is the diff \
                  pairing algorithm — extracting per-tag sub-helpers would re- \
                  pass mutable row / removed / inserted / equal / is_equal_block \
                  state across fn boundaries."
    )]
    pub fn side_by_side(before: &str, after: &str, before_label: &str, after_label: &str) -> Self {
        let diff = TextDiff::from_lines(before, after);
        let mut rows: Vec<SideBySideRow> = Vec::new();

        for group in diff.grouped_ops(3) {
            if !rows.is_empty() {
                // Blank separator between hunks.
                rows.push(SideBySideRow {
                    left: None,
                    right: None,
                });
            }
            for op in &group {
                // Collect removed and inserted lines separately, then pair them.
                let mut removed: Vec<String> = Vec::new();
                let mut inserted: Vec<String> = Vec::new();
                let mut equal: Vec<String> = Vec::new();
                let mut is_equal_block = true;

                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches('\n').to_owned();
                    match change.tag() {
                        ChangeTag::Equal => {
                            if !is_equal_block {
                                // Flush paired removed/inserted before continuing equal
                                pair_into(&mut rows, &mut removed, &mut inserted);
                                is_equal_block = true;
                            }
                            equal.push(text);
                        }
                        ChangeTag::Delete => {
                            if is_equal_block {
                                flush_equal(&mut rows, &mut equal);
                            }
                            is_equal_block = false;
                            removed.push(text);
                        }
                        ChangeTag::Insert => {
                            if is_equal_block {
                                flush_equal(&mut rows, &mut equal);
                            }
                            is_equal_block = false;
                            inserted.push(text);
                        }
                    }
                }
                // Flush trailing equal
                flush_equal(&mut rows, &mut equal);
                pair_into(&mut rows, &mut removed, &mut inserted);
            }
        }

        Self {
            rows: DiffRows::SideBySide {
                rows,
                before_label: before_label.to_owned(),
                after_label: after_label.to_owned(),
            },
            scroll_y: 0,
        }
    }

    /// Single-pane view for added, deleted, or untracked file content.
    #[must_use]
    pub fn single_pane(content: &str, kind: SinglePaneKind, label: &str) -> Self {
        let tag = match kind {
            SinglePaneKind::Added | SinglePaneKind::Untracked => ChangeTag::Insert,
            SinglePaneKind::Deleted => ChangeTag::Delete,
        };
        let lines: Vec<(ChangeTag, String)> =
            content.lines().map(|l| (tag, l.to_owned())).collect();
        Self {
            rows: DiffRows::SinglePane {
                lines,
                label: label.to_owned(),
            },
            scroll_y: 0,
        }
    }

    /// Total row count (height of virtual content).
    #[must_use]
    pub fn total_rows(&self) -> usize {
        match &self.rows {
            DiffRows::SideBySide { rows, .. } => rows.len(),
            DiffRows::SinglePane { lines, .. } => lines.len(),
        }
    }

    /// Current vertical scroll offset (rows from the top).
    #[must_use]
    pub fn scroll_y(&self) -> u16 {
        self.scroll_y
    }

    /// Set the vertical scroll offset. The render pass clamps it to the content
    /// height, so callers restoring a saved offset need not pre-clamp.
    pub fn set_scroll_y(&mut self, scroll_y: u16) {
        self.scroll_y = scroll_y;
    }

    /// Scroll up one line.
    pub fn scroll_up(&mut self) {
        self.scroll_y = self.scroll_y.saturating_sub(1);
    }

    /// Scroll down one line (clamped to total rows).
    pub fn scroll_down(&mut self) {
        let max = self.total_rows().saturating_sub(1) as u16;
        self.scroll_y = self.scroll_y.saturating_add(1).min(max);
    }

    /// Page up (half the viewport height).
    pub fn page_up(&mut self, viewport_height: u16) {
        let step = (viewport_height / 2).max(1);
        self.scroll_y = self.scroll_y.saturating_sub(step);
    }

    /// Page down (half the viewport height).
    pub fn page_down(&mut self, viewport_height: u16) {
        let max = self.total_rows().saturating_sub(1) as u16;
        let step = (viewport_height / 2).max(1);
        self.scroll_y = self.scroll_y.saturating_add(step).min(max);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> crate::ModalOutcome<()> {
        match key.code {
            KeyCode::Esc => crate::ModalOutcome::Cancel,
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.scroll_up();
                crate::ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.scroll_down();
                crate::ModalOutcome::Continue
            }
            KeyCode::PageUp => {
                self.page_up(20);
                crate::ModalOutcome::Continue
            }
            KeyCode::PageDown => {
                self.page_down(20);
                crate::ModalOutcome::Continue
            }
            _ => crate::ModalOutcome::Continue,
        }
    }
}

/// Pair removed/inserted lines into side-by-side rows.
fn flush_equal(rows: &mut Vec<SideBySideRow>, equal: &mut Vec<String>) {
    for e in equal.drain(..) {
        rows.push(SideBySideRow {
            left: Some((ChangeTag::Equal, e.clone())),
            right: Some((ChangeTag::Equal, e)),
        });
    }
}

fn pair_into(rows: &mut Vec<SideBySideRow>, removed: &mut Vec<String>, inserted: &mut Vec<String>) {
    let count = removed.len().max(inserted.len());
    for i in 0..count {
        rows.push(SideBySideRow {
            left: removed.get(i).map(|s| (ChangeTag::Delete, s.clone())),
            right: inserted.get(i).map(|s| (ChangeTag::Insert, s.clone())),
        });
    }
    removed.clear();
    inserted.clear();
}

fn change_style(tag: ChangeTag) -> Style {
    match tag {
        ChangeTag::Equal => Style::default().fg(PHOSPHOR_GREEN),
        ChangeTag::Delete => Style::default().fg(Color::Red).bg(DIFF_REMOVED_BG),
        ChangeTag::Insert => Style::default().fg(Color::Green).bg(DIFF_ADDED_BG),
    }
}

fn change_prefix(tag: ChangeTag) -> &'static str {
    match tag {
        ChangeTag::Equal => " ",
        ChangeTag::Delete => "-",
        ChangeTag::Insert => "+",
    }
}

fn render_single_pane_lines(lines: &[(ChangeTag, String)]) -> Vec<Line<'static>> {
    lines
        .iter()
        .map(|(tag, text)| {
            let style = change_style(*tag);
            let prefix = change_prefix(*tag);
            Line::styled(format!("{prefix} {text}"), style)
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

fn render_side_by_side_pane(rows: &[SideBySideRow], side: Side, label: &str) -> Vec<Line<'static>> {
    let mut result = vec![
        Line::styled(
            format!(" {label}"),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(PHOSPHOR_GREEN),
        ),
        Line::styled("\u{2500}".repeat(80), Style::default().fg(PHOSPHOR_DARK)),
    ];

    for row in rows {
        let cell = match side {
            Side::Left => &row.left,
            Side::Right => &row.right,
        };
        match cell {
            Some((tag, text)) => {
                let style = change_style(*tag);
                let prefix = change_prefix(*tag);
                result.push(Line::styled(format!("{prefix} {text}"), style));
            }
            None => {
                result.push(Line::styled(
                    "~",
                    Style::default()
                        .fg(PHOSPHOR_DARK)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }
    }
    result
}

/// Render the diff view into `area`. The `state.scroll_y` is clamped to valid
/// range on each render call.
#[allow(
    clippy::excessive_nesting,
    reason = "Diff-view renderer: per-mode (SideBySide vs Unified) branches with \
              per-pane row/label/styling nested through the Ratatui buffer draw. \
              Extracting per-mode helpers would require re-borrowing the frame + \
              state across fn boundaries and obscure the per-mode buffer layout."
)]
pub fn render_diff_view(frame: &mut Frame<'_>, area: Rect, state: &mut DiffViewState) {
    // Borrow `rows` immutably for the render and yield the clamped offset; the
    // single `scroll_y` write happens after the borrow ends. Avoids cloning the
    // entire materialized diff on every frame just to dodge the borrow.
    let scroll_y = state.scroll_y;
    let eff_y = match &state.rows {
        DiffRows::SideBySide {
            rows,
            before_label,
            after_label,
        } => {
            let halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let left_lines = render_side_by_side_pane(rows, Side::Left, before_label);
            let right_lines = render_side_by_side_pane(rows, Side::Right, after_label);
            let content_h = left_lines.len();
            let vp_h = halves[0].height.saturating_sub(2) as usize;
            let eff_y = effective_offset(content_h, vp_h, scroll_y);
            render_scrollable_block_at(frame, halves[0], left_lines, 0, eff_y, false, None);
            render_scrollable_block_at(frame, halves[1], right_lines, 0, eff_y, false, None);
            eff_y
        }
        DiffRows::SinglePane { lines, label } => {
            let rendered = render_single_pane_lines(lines);
            let content_h = rendered.len();
            let vp_h = area.height.saturating_sub(2) as usize;
            let eff_y = effective_offset(content_h, vp_h, scroll_y);
            render_scrollable_block_at(frame, area, rendered, 0, eff_y, true, Some(label));
            eff_y
        }
    };
    state.scroll_y = eff_y;
}

/// Hint-bar spans for the diff view (scroll keys).
#[must_use]
pub fn diff_view_hint_spans() -> Vec<crate::HintSpan<'static>> {
    vec![
        crate::HintSpan::Key("↑↓"),
        crate::HintSpan::Text("scroll"),
        crate::HintSpan::Sep,
        crate::HintSpan::Key("PgUp PgDn"),
        crate::HintSpan::Text("page"),
    ]
}

#[cfg(test)]
mod tests;
