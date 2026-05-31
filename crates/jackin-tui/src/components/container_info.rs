//! Shared read-only container/session information dialog.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::ModalOutcome;
use crate::ansi;
use crate::components::{Panel, PanelFocus};
use crate::theme::{LINK_BLUE, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone)]
pub struct ContainerInfoRow {
    label: String,
    value: String,
    href: Option<String>,
    copyable: bool,
    emphasised: bool,
}

impl ContainerInfoRow {
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            href: None,
            copyable: false,
            emphasised: false,
        }
    }

    #[must_use]
    pub fn hyperlink(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    #[must_use]
    pub const fn copyable(mut self) -> Self {
        self.copyable = true;
        self
    }

    #[must_use]
    pub const fn emphasised(mut self) -> Self {
        self.emphasised = true;
        self
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn href(&self) -> Option<&str> {
        self.href.as_deref()
    }

    #[must_use]
    pub const fn is_copyable(&self) -> bool {
        self.copyable
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfoState {
    title: String,
    rows: Vec<ContainerInfoRow>,
    copied_row: Option<usize>,
}

impl ContainerInfoState {
    #[must_use]
    pub fn new(title: impl Into<String>, rows: Vec<ContainerInfoRow>) -> Self {
        Self {
            title: title.into(),
            rows,
            copied_row: None,
        }
    }

    #[must_use]
    pub fn rows(&self) -> &[ContainerInfoRow] {
        &self.rows
    }

    pub fn handle_key(&self, key: KeyEvent) -> ModalOutcome<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q' | 'Q') => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }

    pub fn mark_copied(&mut self, row: usize) {
        self.copied_row = Some(row);
    }

    #[must_use]
    pub const fn copied_row(&self) -> Option<usize> {
        self.copied_row
    }
}

#[must_use]
pub fn required_height(state: &ContainerInfoState) -> u16 {
    u16::try_from(state.rows.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .max(7)
}

pub fn render_container_info(frame: &mut Frame<'_>, area: Rect, state: &ContainerInfoState) {
    if area.width < 20 || area.height < 5 {
        return;
    }
    let title = format!(" {} ", state.title);
    let block = Panel::new()
        .title(&title)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let label_width = state
        .rows
        .iter()
        .map(|row| crate::display_cols(&row.label))
        .max()
        .unwrap_or(0);
    let max_rows = usize::from(inner.height.saturating_sub(2));
    for (idx, row) in state.rows.iter().take(max_rows).enumerate() {
        let y = inner
            .y
            .saturating_add(1)
            .saturating_add(u16::try_from(idx).unwrap_or(u16::MAX));
        let row_area = Rect {
            x: inner.x.saturating_add(2),
            y,
            width: inner.width.saturating_sub(4),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(container_info_line(
                row,
                label_width,
                state.copied_row == Some(idx),
            )),
            row_area,
        );
    }
}

#[must_use]
pub fn copy_payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
) -> Option<(usize, String)> {
    value_rects(area, state)
        .into_iter()
        .find(|(idx, rect)| {
            let info_row = &state.rows[*idx];
            info_row.copyable
                && col >= rect.x
                && col < rect.x.saturating_add(rect.width)
                && row >= rect.y
                && row < rect.y.saturating_add(rect.height)
        })
        .map(|(idx, _)| (idx, state.rows[idx].value.clone()))
}

#[must_use]
pub fn hyperlink_overlay(area: Rect, state: &ContainerInfoState) -> Vec<u8> {
    let mut out = Vec::new();
    for (idx, rect) in value_rects(area, state) {
        let row = &state.rows[idx];
        let Some(href) = row.href() else {
            continue;
        };
        ansi::move_to(&mut out, rect.y, rect.x);
        ansi::emit_osc8_open(&mut out, href);
        ansi::fg(&mut out, crate::LINK_BLUE);
        out.extend_from_slice(b"\x1b[1;4m");
        out.extend_from_slice(
            crate::take_display_cols(row.value(), usize::from(rect.width)).as_bytes(),
        );
        ansi::emit_osc8_close(&mut out);
        out.extend_from_slice(ansi::RESET.as_bytes());
    }
    out
}

fn value_rects(area: Rect, state: &ContainerInfoState) -> Vec<(usize, Rect)> {
    if area.width < 20 || area.height < 5 {
        return Vec::new();
    }
    let block = Panel::new().focus(PanelFocus::Focused).block();
    let inner = block.inner(area);
    let label_width = state
        .rows
        .iter()
        .map(|row| crate::display_cols(&row.label))
        .max()
        .unwrap_or(0);
    let value_x = inner
        .x
        .saturating_add(2)
        .saturating_add(u16::try_from(label_width + 3).unwrap_or(u16::MAX));
    let value_width = inner
        .width
        .saturating_sub(4)
        .saturating_sub(u16::try_from(label_width + 3).unwrap_or(u16::MAX));
    let max_rows = usize::from(inner.height.saturating_sub(2));
    state
        .rows
        .iter()
        .take(max_rows)
        .enumerate()
        .map(|(idx, row)| {
            let y = inner
                .y
                .saturating_add(1)
                .saturating_add(u16::try_from(idx).unwrap_or(u16::MAX));
            let drawn_width = u16::try_from(crate::display_cols(row.value())).unwrap_or(u16::MAX);
            (
                idx,
                Rect {
                    x: value_x,
                    y,
                    width: drawn_width.min(value_width),
                    height: 1,
                },
            )
        })
        .collect()
}

fn container_info_line(row: &ContainerInfoRow, label_width: usize, copied: bool) -> Line<'static> {
    let label_style = Style::default().fg(PHOSPHOR_DIM);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let mut value_style = Style::default().fg(if row.href.is_some() {
        LINK_BLUE
    } else if row.emphasised {
        PHOSPHOR_GREEN
    } else {
        WHITE
    });
    if row.emphasised || row.copyable || row.href.is_some() {
        value_style = value_style.add_modifier(Modifier::BOLD);
    }
    if row.href.is_some() {
        value_style = value_style.add_modifier(Modifier::UNDERLINED);
    }
    let mut spans = vec![
        Span::styled(format!("{:<label_width$}", row.label), label_style),
        Span::styled(" : ", sep_style),
        Span::styled(row.value.clone(), value_style),
    ];
    if copied {
        spans.push(Span::styled(
            "  Copied!",
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_rows_with_title_and_link_style() {
        let state = ContainerInfoState::new(
            "Container info",
            vec![
                ContainerInfoRow::new("Container ID", "jk-test")
                    .copyable()
                    .emphasised(),
                ContainerInfoRow::new("Run log", "~/.jackin/run.jsonl")
                    .hyperlink("file:///tmp/run.jsonl"),
            ],
        );
        let backend = TestBackend::new(64, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_container_info(frame, frame.area(), &state))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Container info"));
        assert!(rendered.contains("jk-test"));
        assert!(rendered.contains("~/.jackin/run.jsonl"));
    }

    #[test]
    fn copy_payload_at_hits_copyable_value_column() {
        let state = ContainerInfoState::new(
            "Container info",
            vec![
                ContainerInfoRow::new("Container ID", "jk-test")
                    .copyable()
                    .emphasised(),
                ContainerInfoRow::new("Run log", "~/.jackin/run.jsonl")
                    .hyperlink("file:///tmp/run.jsonl"),
            ],
        );
        let area = Rect::new(0, 0, 64, 10);

        assert_eq!(
            copy_payload_at(area, &state, 18, 2),
            Some((0, "jk-test".to_string()))
        );
        assert_eq!(
            copy_payload_at(area, &state, 18, 3),
            None,
            "hyperlink-only rows are not copy targets"
        );
    }

    #[test]
    fn hyperlink_overlay_emits_osc8_for_link_rows() {
        let state = ContainerInfoState::new(
            "Container info",
            vec![
                ContainerInfoRow::new("Container ID", "jk-test").copyable(),
                ContainerInfoRow::new("Run log", "/tmp/run.jsonl")
                    .hyperlink("file:///tmp/run.jsonl"),
            ],
        );

        let overlay = String::from_utf8(hyperlink_overlay(Rect::new(0, 0, 64, 10), &state))
            .expect("overlay should be utf8");

        assert!(overlay.contains("\u{1b}]8;;file:///tmp/run.jsonl\u{1b}\\"));
        assert!(overlay.contains("/tmp/run.jsonl"));
        assert!(overlay.contains("\u{1b}]8;;\u{1b}\\"));
    }
}
