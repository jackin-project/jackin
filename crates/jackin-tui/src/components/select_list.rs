//! Generic modal filter-picker over labelled string items.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Widget};

use crate::ModalOutcome;
use crate::components::FilterInput;
use crate::components::panel::{Panel, PanelFocus};
use crate::scroll::{cursor_follow_offset, full_cell_thumb, is_scrollable};
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

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
}

impl<'a> SelectList<'a> {
    #[must_use]
    pub const fn new(state: &'a SelectListState, title: &'a str) -> Self {
        Self {
            state,
            title,
            context: &[],
        }
    }

    #[must_use]
    pub const fn context(mut self, context: &'a [Line<'a>]) -> Self {
        self.context = context;
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

        let lines: Vec<Line<'_>> = self
            .state
            .filtered
            .iter()
            .enumerate()
            .map(|(row, &item)| {
                let is_selected = Some(row) == self.state.selected;
                let prefix = if is_selected { "\u{25b8} " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(PHOSPHOR_GREEN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                Line::from(vec![Span::styled(
                    format!("{prefix}{}", self.state.items[item]),
                    style,
                )])
            })
            .collect();
        render_selected_lines(list_area, buf, lines, self.state.selected);
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

fn render_selected_lines(
    area: Rect,
    buf: &mut Buffer,
    lines: Vec<Line<'_>>,
    selected: Option<usize>,
) {
    let viewport = usize::from(area.height);
    let total = lines.len();
    let offset = cursor_follow_offset(selected.unwrap_or(0), total, viewport, 0);
    let visible: Text<'_> = lines.into_iter().skip(offset).take(viewport).collect();
    Paragraph::new(visible).render(area, buf);

    if is_scrollable(total, viewport)
        && let Some(thumb) = full_cell_thumb(total, viewport, area.height, offset)
    {
        let x = area.x + area.width.saturating_sub(1);
        for row in 0..area.height {
            let style = if row >= thumb.start && row < thumb.start.saturating_add(thumb.len) {
                Style::default().fg(PHOSPHOR_GREEN)
            } else {
                Style::default().fg(PHOSPHOR_DARK)
            };
            buf[(x, area.y + row)].set_symbol("█").set_style(style);
        }
    }
}
