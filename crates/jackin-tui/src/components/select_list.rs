//! Generic modal filter-picker over labelled string items.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Widget};

use crate::ModalOutcome;
use crate::components::FilterInput;
use crate::components::panel::{Panel, PanelFocus};
use crate::scroll::{cursor_follow_offset, full_cell_thumb, is_scrollable};
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN};

#[derive(Debug)]
pub struct SelectListState {
    items: Vec<String>,
    selected: Option<usize>,
    filter: String,
    filtered: Vec<usize>,
}

impl SelectListState {
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        Self {
            selected: (!filtered.is_empty()).then_some(0),
            items,
            filter: String::new(),
            filtered,
        }
    }

    /// Set an initial filter string. Recomputes the visible-item list immediately.
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self.recompute_filtered();
        self
    }

    fn recompute_filtered(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, label)| needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        self.selected = (!self.filtered.is_empty()).then_some(0);
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn max_label_width(&self) -> u16 {
        self.items
            .iter()
            .map(|label| label.chars().count())
            .max()
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
            .and_then(|row| self.filtered.get(row).copied())
    }

    pub fn select_index(&mut self, index: usize) {
        if let Some(row) = self
            .filtered
            .iter()
            .position(|candidate| *candidate == index)
        {
            self.selected = Some(row);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<usize> {
        match key.code {
            KeyCode::Up => {
                self.cycle_select(-1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                self.cycle_select(1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute_filtered();
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => self
                .selected_index()
                .map_or(ModalOutcome::Continue, ModalOutcome::Commit),
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                self.filter.push(ch);
                self.recompute_filtered();
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn cycle_select(&mut self, delta: i32) {
        let count = self.filtered.len();
        if count == 0 {
            return;
        }
        let current = self.selected.unwrap_or(0);
        self.selected = Some(if delta < 0 {
            if current == 0 { count - 1 } else { current - 1 }
        } else if current + 1 >= count {
            0
        } else {
            current + 1
        });
    }
}

pub struct SelectList<'a> {
    state: &'a SelectListState,
    title: &'a str,
    context: &'a [Line<'a>],
    empty_label: &'a str,
}

impl<'a> SelectList<'a> {
    #[must_use]
    pub const fn new(state: &'a SelectListState, title: &'a str) -> Self {
        Self {
            state,
            title,
            context: &[],
            empty_label: "no matches",
        }
    }

    #[must_use]
    pub const fn context(mut self, context: &'a [Line<'a>]) -> Self {
        self.context = context;
        self
    }

    /// Override the placeholder shown when the list has no items (empty or filtered-to-nothing).
    #[must_use]
    pub const fn empty_label(mut self, label: &'a str) -> Self {
        self.empty_label = label;
        self
    }
}

impl Widget for SelectList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // SelectList is always a modal overlay — always the active container
        // when visible. Use PHOSPHOR_GREEN border per the focus-visible rule.
        // Build the title string first so the borrow lives long enough.
        let title_str = format!(" {} ", self.title);
        let block = Panel::new()
            .title(&title_str)
            .focus(PanelFocus::Focused)
            .block();
        let inner = block.inner(area);
        Clear.render(area, buf);
        block.render(area, buf);

        let mut constraints = vec![Constraint::Length(1), Constraint::Length(1)];
        if !self.context.is_empty() {
            constraints.push(Constraint::Length(
                u16::try_from(self.context.len()).unwrap_or(u16::MAX),
            ));
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Min(1));
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        FilterInput::new(&self.state.filter).render(rows[0], buf);

        let Some(list_area) = rows.last().copied() else {
            return;
        };
        if !self.context.is_empty() {
            Paragraph::new(self.context.to_vec()).render(rows[2], buf);
        }

        if self.state.filtered.is_empty() {
            // Distinguish "genuinely empty" (no items) from "filtered to nothing".
            let placeholder = if self.state.items.is_empty() {
                self.empty_label
            } else {
                "no matches"
            };
            // Dim centered placeholder so operators can distinguish "empty" from "broken".
            Paragraph::new(Line::from(Span::styled(
                placeholder.to_string(),
                crate::theme::DIM,
            )))
            .alignment(Alignment::Center)
            .render(list_area, buf);
            return;
        }
        let viewport_cols = usize::from(list_area.width);
        let rows: Vec<PickerRow<'_>> = self
            .state
            .filtered
            .iter()
            .map(|&item| {
                let label = &self.state.items[item];
                // Truncate labels wider than the viewport with an ellipsis so wide
                // rows are legible rather than silently clipping at the border.
                let label_str = if crate::display_cols(label) > viewport_cols {
                    let mut s = crate::take_display_cols(label, viewport_cols.saturating_sub(1));
                    s.push('…');
                    s
                } else {
                    label.clone()
                };
                PickerRow::Item(ListItem::new(Line::from(Span::styled(
                    label_str,
                    Style::default().fg(PHOSPHOR_GREEN),
                ))))
            })
            .collect();
        render_picker_list(list_area, buf, rows, self.state.selected);
    }
}

pub fn render_select_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &SelectListState,
    title: &str,
    context: &[Line<'_>],
) {
    frame.render_widget(SelectList::new(state, title).context(context), area);
}

/// A row in a modal picker list.
pub enum PickerRow<'a> {
    /// A selectable item. The caller styles its unselected appearance; the
    /// selected row gets the canonical PHOSPHOR_GREEN highlight applied by
    /// `render_picker_list`.
    Item(ListItem<'a>),
    /// Non-selectable section divider rendered as `──── label ────`. Drawn
    /// edge-to-edge across the full list width with the label centered — it
    /// deliberately ignores the 2-col selection gutter so the dashes reach
    /// both dialog borders.
    Separator(String),
}

/// Paint a `──── label ────` section divider across a full list row,
/// edge-to-edge, with the label centered. Dashes use PHOSPHOR_DARK; the
/// label is DIM. Shared so the capsule pickers and any future sectioned
/// host list draw identical dividers.
fn write_section_separator(buf: &mut Buffer, area: Rect, y: u16, label: &str) {
    let width = usize::from(area.width);
    if width == 0 {
        return;
    }
    let label_disp = if label.is_empty() {
        String::new()
    } else {
        format!(" {label} ")
    };
    let label_cols = crate::display_cols(&label_disp).min(width);
    let dashes = width - label_cols;
    let left = dashes / 2;
    let right = dashes - left;
    let mut spans = Vec::with_capacity(3);
    if left > 0 {
        spans.push(Span::styled(
            "\u{2500}".repeat(left),
            Style::default().fg(PHOSPHOR_DARK),
        ));
    }
    if !label_disp.is_empty() {
        spans.push(Span::styled(label_disp, crate::theme::DIM));
    }
    if right > 0 {
        spans.push(Span::styled(
            "\u{2500}".repeat(right),
            Style::default().fg(PHOSPHOR_DARK),
        ));
    }
    let row_area = Rect {
        x: area.x,
        y,
        width: area.width,
        height: 1,
    };
    Paragraph::new(Line::from(spans)).render(row_area, buf);
}

/// Render a vertical picker list into `area`: a ratatui `List` with the
/// canonical selected-row highlight (PHOSPHOR_GREEN background, PHOSPHOR_DARK
/// text, bold, `▸ ` cursor) plus a right-edge scroll thumb. Shared so every
/// modal list — the capsule menu/pickers and the host console — gets the same
/// look from one place. Callers pass pre-built `PickerRow`s (style the
/// unselected item rows themselves) and the selected row index.
///
/// `PickerRow::Separator` rows are repainted edge-to-edge after the `List`
/// draws, overwriting the gutter the List reserves so section dividers span
/// the full width with a centered label.
pub fn render_picker_list(
    area: Rect,
    buf: &mut Buffer,
    rows: Vec<PickerRow<'_>>,
    selected: Option<usize>,
) {
    let total = rows.len();
    let viewport = usize::from(area.height);
    let offset = cursor_follow_offset(selected.unwrap_or(0), total, viewport, 0);

    // Record separator rows + labels before the items are consumed so the
    // post-pass can repaint them full-width over the List's gutter.
    let separators: Vec<(usize, String)> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| match row {
            PickerRow::Separator(label) => Some((i, label.clone())),
            PickerRow::Item(_) => None,
        })
        .collect();
    let items: Vec<ListItem<'_>> = rows
        .into_iter()
        .map(|row| match row {
            PickerRow::Item(item) => item,
            // Placeholder — write_section_separator overwrites this row.
            PickerRow::Separator(_) => ListItem::new(""),
        })
        .collect();

    // Canonical modal-list look (matches the legacy raw dialog 1:1): the whole
    // list sits on the dark dialog surface, the selected row inverts to a
    // PHOSPHOR_GREEN bar with black bold text and a `▸` cursor.
    let highlight = Style::default()
        .bg(PHOSPHOR_GREEN)
        .fg(crate::theme::color(crate::BLACK))
        .add_modifier(Modifier::BOLD);
    let mut state = ListState::default()
        .with_offset(offset)
        .with_selected(selected);
    let list = List::new(items)
        .style(Style::default().bg(crate::theme::DIALOG_SURFACE))
        .highlight_style(highlight)
        .highlight_symbol("\u{25b8} ") // ▸
        .highlight_spacing(HighlightSpacing::Always);
    ratatui::widgets::StatefulWidget::render(list, area, buf, &mut state);

    // Repaint section dividers edge-to-edge over the gutter the List reserved.
    for (i, label) in separators {
        if i < offset || i >= offset + viewport {
            continue;
        }
        let y = area.y + u16::try_from(i - offset).unwrap_or(0);
        write_section_separator(buf, area, y, &label);
    }

    if is_scrollable(total, viewport)
        && let Some(thumb) = full_cell_thumb(total, viewport, area.height, offset)
    {
        // Drawn after the dividers so the thumb column always wins. Same glyphs
        // as the shared FixedScrollbar (Line style): `┃` thumb over the dim `·`
        // track, so picker scrollbars match every other bar in the TUI.
        use crate::components::scrollable_panel::{SCROLLBAR_TRACK, ScrollbarStyle};
        let thumb_sym = ScrollbarStyle::Line.vertical_thumb();
        let x = area.x + area.width.saturating_sub(1);
        for row in 0..area.height {
            let in_thumb = row >= thumb.start && row < thumb.start.saturating_add(thumb.len);
            let (sym, style) = if in_thumb {
                (thumb_sym, crate::theme::GREEN)
            } else {
                (SCROLLBAR_TRACK, Style::default().fg(PHOSPHOR_DARK))
            };
            buf[(x, area.y + row)].set_symbol(sym).set_style(style);
        }
    }
}

#[cfg(test)]
mod picker_list_tests {
    use super::{PickerRow, render_picker_list};
    use crate::theme::PHOSPHOR_DARK;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::ListItem;

    fn row_symbols(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn separator_spans_full_width_and_centers_label() {
        let width = 24u16;
        let area = Rect {
            x: 0,
            y: 0,
            width,
            height: 4,
        };
        let mut buf = Buffer::empty(area);
        let rows = vec![
            PickerRow::Separator("agents".to_string()),
            PickerRow::Item(ListItem::new("Claude")),
            PickerRow::Item(ListItem::new("Codex")),
        ];
        render_picker_list(area, &mut buf, rows, Some(1));

        let sep = row_symbols(&buf, 0, width);
        // Edge-to-edge: dashes reach both borders, ignoring the 2-col gutter.
        assert!(
            sep.starts_with('\u{2500}') && sep.ends_with('\u{2500}'),
            "divider must span full width: {sep:?}"
        );
        assert!(sep.contains("agents"), "label present: {sep:?}");
        // Label centered: leading and trailing dash runs match (±1 for odd
        // remainder). Count by character, not byte — dashes are multi-byte.
        let chars: Vec<char> = sep.chars().collect();
        let left = chars.iter().take_while(|c| **c == '\u{2500}').count();
        let right = chars.iter().rev().take_while(|c| **c == '\u{2500}').count();
        assert!(
            left.abs_diff(right) <= 1,
            "label not centered: left={left} right={right} in {sep:?}"
        );

        // The dashes are PHOSPHOR_DARK.
        assert_eq!(buf[(0u16, 0u16)].fg, PHOSPHOR_DARK);
    }

    #[test]
    fn item_rows_keep_selection_gutter() {
        let width = 20u16;
        let area = Rect {
            x: 0,
            y: 0,
            width,
            height: 3,
        };
        let mut buf = Buffer::empty(area);
        let rows = vec![
            PickerRow::Item(ListItem::new("Claude")),
            PickerRow::Item(ListItem::new("Codex")),
        ];
        render_picker_list(area, &mut buf, rows, Some(0));
        // Selected row 0 shows the ▸ cursor in the reserved gutter.
        assert_eq!(buf[(0u16, 0u16)].symbol(), "\u{25b8}");
    }
}
