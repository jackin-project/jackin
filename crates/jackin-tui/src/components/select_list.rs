// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Generic modal filter-picker over labelled string items.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, HighlightSpacing, ListItem, Paragraph, Widget};

use crate::components::FilterInput;
use crate::components::panel::{Panel, PanelFocus};
use crate::components::scrollable_panel::ScrollableList;
use crate::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
use crate::scroll::{cursor_follow_offset, full_cell_thumb, is_scrollable};
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN};
use crate::{HintSpan, ModalOutcome};

const SELECT_LIST_HORIZONTAL_SCROLL_STEP: u16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectListAction {
    NavUp,
    NavDown,
    ScrollLeft,
    ScrollRight,
    ScrollHome,
    DeleteFilter,
    Commit,
    Cancel,
}

const SELECT_LIST_BINDINGS: &[KeyBinding<SelectListAction>] = &[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up)],
        action: SelectListAction::NavUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Down)],
        action: SelectListAction::NavDown,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Left)],
        action: SelectListAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Right)],
        action: SelectListAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Home)],
        action: SelectListAction::ScrollHome,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Backspace)],
        action: SelectListAction::DeleteFilter,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: SelectListAction::Commit,
        hint: Some("select"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc)],
        action: SelectListAction::Cancel,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: None,
    },
];

pub static SELECT_LIST_KEYMAP: Keymap<SelectListAction> = Keymap::new(SELECT_LIST_BINDINGS);

/// Hint spans for the filter-picker: keymap-derived structured keys plus the
/// free-text "type to filter" group that cannot be expressed as a chord.
#[must_use]
pub fn select_list_hint_spans() -> Vec<HintSpan<'static>> {
    let mut spans = SELECT_LIST_KEYMAP.hint_spans();
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Text("type to filter"));
    spans
}

#[derive(Debug)]
pub struct SelectListState {
    items: Vec<String>,
    selected: Option<usize>,
    filter: String,
    filtered: Vec<usize>,
    scroll_x: u16,
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
            scroll_x: 0,
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
        self.scroll_x = 0;
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

    #[must_use]
    pub const fn scroll_x(&self) -> u16 {
        self.scroll_x
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<usize> {
        let chord = KeyChord::from(key);
        if let Some(action) = SELECT_LIST_KEYMAP.dispatch(chord) {
            return match action {
                SelectListAction::NavUp => {
                    self.cycle_select(-1);
                    ModalOutcome::Continue
                }
                SelectListAction::NavDown => {
                    self.cycle_select(1);
                    ModalOutcome::Continue
                }
                SelectListAction::ScrollLeft => {
                    self.scroll_x = self
                        .scroll_x
                        .saturating_sub(SELECT_LIST_HORIZONTAL_SCROLL_STEP);
                    ModalOutcome::Continue
                }
                SelectListAction::ScrollRight => {
                    self.scroll_x = self
                        .scroll_x
                        .saturating_add(SELECT_LIST_HORIZONTAL_SCROLL_STEP);
                    ModalOutcome::Continue
                }
                SelectListAction::ScrollHome => {
                    self.scroll_x = 0;
                    ModalOutcome::Continue
                }
                SelectListAction::DeleteFilter => {
                    if self.filter.pop().is_some() {
                        self.recompute_filtered();
                    }
                    ModalOutcome::Continue
                }
                SelectListAction::Commit => self
                    .selected_index()
                    .map_or(ModalOutcome::Continue, ModalOutcome::Commit),
                SelectListAction::Cancel => ModalOutcome::Cancel,
            };
        }
        // Printable chars not bound to any action flow into the type-to-filter buffer.
        if let KeyCode::Char(ch) = key.code {
            self.filter.push(ch);
            self.recompute_filtered();
        }
        ModalOutcome::Continue
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

fn render_select_list_in(
    area: Rect,
    buf: &mut Buffer,
    state: &SelectListState,
    title: &str,
    context: &[Line<'_>],
) {
    // SelectList is always a modal overlay — always the active container
    // when visible. Use PHOSPHOR_GREEN border per the focus-visible rule.
    // Build the title string first so the borrow lives long enough.
    let title_str = format!(" {title} ");
    let block = Panel::new()
        .title(&title_str)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    Clear.render(area, buf);
    block.render(area, buf);

    let mut constraints = vec![Constraint::Length(1), Constraint::Length(1)];
    if !context.is_empty() {
        constraints.push(Constraint::Length(
            u16::try_from(context.len()).unwrap_or(u16::MAX),
        ));
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    FilterInput::new(&state.filter).render(rows[0], buf);

    let Some(list_area) = rows.last().copied() else {
        return;
    };
    if !context.is_empty() {
        Paragraph::new(context.to_vec()).render(rows[2], buf);
    }

    if state.filtered.is_empty() {
        // Dim centered placeholder so operators can distinguish "empty" from "broken".
        Paragraph::new(Line::from(Span::styled("no matches", crate::theme::DIM)))
            .alignment(Alignment::Center)
            .render(list_area, buf);
        return;
    }
    let viewport_cols = usize::from(list_area.width);
    let content_width = usize::from(state.max_label_width());
    let scroll_x = crate::components::scrollable_panel::effective_offset(
        content_width,
        viewport_cols,
        state.scroll_x,
    );
    let has_horizontal_scroll = is_scrollable(content_width, viewport_cols);
    let list_body_area = if has_horizontal_scroll {
        Rect {
            height: list_area.height.saturating_sub(1),
            ..list_area
        }
    } else {
        list_area
    };
    if list_body_area.height == 0 {
        return;
    }
    let rows: Vec<PickerRow<'_>> = state
        .filtered
        .iter()
        .map(|&item| {
            let label = &state.items[item];
            let skip = usize::from(scroll_x);
            // With no horizontal scroll, keep the ellipsis affordance for
            // over-wide labels. Once scrolled, render the exact window.
            let label_str = if skip == 0 && crate::display_cols(label) > viewport_cols {
                let mut s = crate::display_cols_slice(label, skip, viewport_cols.saturating_sub(1));
                s.push('…');
                s
            } else {
                crate::display_cols_slice(label, skip, viewport_cols)
            };
            PickerRow::Item(ListItem::new(Line::from(Span::styled(
                label_str,
                Style::default().fg(PHOSPHOR_GREEN),
            ))))
        })
        .collect();
    render_picker_list(list_body_area, buf, rows, state.selected);
    if has_horizontal_scroll {
        render_picker_horizontal_scrollbar(list_area, buf, content_width, viewport_cols, scroll_x);
    }
}

pub fn render_select_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &SelectListState,
    title: &str,
    context: &[Line<'_>],
) {
    render_select_list_in(area, frame.buffer_mut(), state, title, context);
}

/// A row in a modal picker list.
#[derive(Debug)]
pub enum PickerRow<'a> {
    /// A selectable item. The caller styles its unselected appearance; the
    /// selected row gets the canonical `PHOSPHOR_GREEN` highlight applied by
    /// `render_picker_list`.
    Item(ListItem<'a>),
    /// Non-selectable section divider rendered as `──── label ────`. Drawn
    /// edge-to-edge across the full list width with the label centered — it
    /// deliberately ignores the 2-col selection gutter so the dashes reach
    /// both dialog borders.
    Separator(String),
}

/// Paint a `──── label ────` section divider across a full list row,
/// edge-to-edge, with the label centered. Dashes use `PHOSPHOR_DARK`; the
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
/// canonical selected-row highlight (`PHOSPHOR_GREEN` background, `PHOSPHOR_DARK`
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
    let offset = offset.min(usize::from(u16::MAX)) as u16;
    ScrollableList::new(items)
        .style(Style::default().bg(crate::theme::DIALOG_SURFACE))
        .highlight_style(highlight)
        .highlight_symbol("\u{25b8} ") // ▸
        .highlight_spacing(HighlightSpacing::Always)
        .offset(offset)
        .selected(selected)
        .render(area, buf);

    // Repaint section dividers edge-to-edge over the gutter the List reserved.
    let offset = usize::from(offset);
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

/// Adapter for picker callers that already build rich Ratatui lines.
///
/// Selection chrome still belongs to `render_picker_list`: callers should pass
/// unselected row content here, without a manual `▸` prefix or selected style.
pub fn render_picker_lines(
    area: Rect,
    buf: &mut Buffer,
    lines: Vec<Line<'_>>,
    selected: Option<usize>,
) {
    let rows = lines
        .into_iter()
        .map(|line| PickerRow::Item(ListItem::new(line)))
        .collect();
    render_picker_list(area, buf, rows, selected);
}

fn render_picker_horizontal_scrollbar(
    list_area: Rect,
    buf: &mut Buffer,
    content_width: usize,
    viewport_cols: usize,
    scroll_x: u16,
) {
    let track_len = usize::from(list_area.width);
    if track_len == 0 {
        return;
    }
    let Some(thumb) = full_cell_thumb(
        content_width,
        viewport_cols,
        list_area.width,
        usize::from(scroll_x),
    ) else {
        return;
    };
    use crate::components::scrollable_panel::{SCROLLBAR_HORIZONTAL_THUMB, SCROLLBAR_TRACK};
    let y = list_area.y + list_area.height.saturating_sub(1);
    for col in 0..list_area.width {
        let in_thumb = col >= thumb.start && col < thumb.start.saturating_add(thumb.len);
        let (sym, style) = if in_thumb {
            (SCROLLBAR_HORIZONTAL_THUMB, crate::theme::GREEN)
        } else {
            (SCROLLBAR_TRACK, Style::default().fg(PHOSPHOR_DARK))
        };
        buf[(list_area.x + col, y)].set_symbol(sym).set_style(style);
    }
}

#[cfg(test)]
mod tests;
