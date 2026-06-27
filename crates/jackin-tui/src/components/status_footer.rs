//! White bottom status footer component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::display_cols;
use crate::theme::{DANGER_RED, DEBUG_AMBER, LINK_BLUE, WHITE, faded};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusFooterHover {
    pub left: bool,
    pub usage: bool,
    pub right: bool,
    /// Whether the pointer is over the debug chip (inverts chip colors on hover).
    pub right_debug: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusRightGroup<'a> {
    pub usage: Option<&'a str>,
    /// Monetary spend chunk (`$53/$300`). Placed between `run_id` and
    /// `container` so it claims width before the container name — under
    /// pressure the container drops first and spend stays visible.
    pub spend: Option<&'a str>,
    pub container: &'a str,
    pub run_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRightChunk {
    pub text: String,
    /// One-based inclusive start column, matching terminal mouse coordinates.
    pub start: u16,
    /// One-based exclusive end column.
    pub end: u16,
}

impl StatusRightChunk {
    #[must_use]
    pub const fn contains(&self, col: u16) -> bool {
        col >= self.start && col < self.end
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusRightGroupLayout {
    pub usage: Option<StatusRightChunk>,
    pub spend: Option<StatusRightChunk>,
    pub container: Option<StatusRightChunk>,
    pub run_id: Option<StatusRightChunk>,
}

impl StatusRightGroupLayout {
    #[must_use]
    pub fn start(&self, fallback: usize) -> usize {
        // Leftmost present chunk, in left-to-right order:
        // usage | container | spend | run_id.
        self.usage
            .as_ref()
            .or(self.container.as_ref())
            .or(self.spend.as_ref())
            .or(self.run_id.as_ref())
            .map_or(fallback, |chunk| usize::from(chunk.start))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StatusFooter<'a> {
    left: &'a str,
    right: StatusRightGroup<'a>,
    alpha: f32,
    hover: StatusFooterHover,
}

#[must_use]
pub fn status_right_group_layout(
    term_cols: u16,
    right: StatusRightGroup<'_>,
) -> StatusRightGroupLayout {
    if term_cols == 0 {
        return StatusRightGroupLayout::default();
    }
    let term_cols = usize::from(term_cols);
    let mut cursor = term_cols.saturating_add(1);
    let run_id = place_status_right_chunk(
        &mut cursor,
        term_cols,
        right
            .run_id
            .filter(|run_id| !run_id.is_empty())
            .map(|run_id| format!(" {run_id} ")),
    );
    // Spend is placed before the container so it survives width pressure first.
    let spend = place_status_right_chunk(
        &mut cursor,
        term_cols,
        right
            .spend
            .filter(|spend| !spend.is_empty())
            .map(|spend| format!(" {spend} ")),
    );
    let container = place_status_right_chunk(&mut cursor, term_cols, {
        (!right.container.is_empty()).then(|| format!(" {} ", right.container))
    });
    let usage = place_usage_status_chunk(&mut cursor, term_cols, right.usage);
    StatusRightGroupLayout {
        usage,
        spend,
        container,
        run_id,
    }
}

fn place_usage_status_chunk(
    cursor: &mut usize,
    term_cols: usize,
    usage: Option<&str>,
) -> Option<StatusRightChunk> {
    let usage = usage.filter(|usage| !usage.is_empty())?;
    let full = format!(" {usage} ");
    let compact_usage = compact_usage_status_label(usage);
    let compact = format!(" {compact_usage} ");
    if display_cols(&full) < cursor.saturating_sub(1) {
        place_status_right_chunk(cursor, term_cols, Some(full))
    } else if display_cols(&compact) < cursor.saturating_sub(1) {
        place_status_right_chunk(cursor, term_cols, Some(compact))
    } else {
        None
    }
}

fn place_status_right_chunk(
    cursor: &mut usize,
    term_cols: usize,
    chunk: Option<String>,
) -> Option<StatusRightChunk> {
    let chunk = chunk?;
    let cols = display_cols(&chunk);
    if cols == 0 || cols + 2 >= term_cols || cols >= cursor.saturating_sub(1) {
        return None;
    }
    let start = cursor.saturating_sub(cols);
    let end = start.saturating_add(cols);
    *cursor = start;
    Some(StatusRightChunk {
        text: chunk,
        start: u16::try_from(start).unwrap_or(u16::MAX),
        end: u16::try_from(end).unwrap_or(u16::MAX),
    })
}

impl<'a> StatusFooter<'a> {
    #[must_use]
    pub const fn new(left: &'a str) -> Self {
        Self {
            left,
            right: StatusRightGroup {
                usage: None,
                spend: None,
                container: "",
                run_id: None,
            },
            alpha: 1.0,
            hover: StatusFooterHover {
                left: false,
                usage: false,
                right: false,
                right_debug: false,
            },
        }
    }

    #[must_use]
    pub const fn right(mut self, right: &'a str) -> Self {
        self.right.container = right;
        self
    }

    #[must_use]
    pub const fn right_debug(mut self, right_debug: Option<&'a str>) -> Self {
        self.right.run_id = right_debug;
        self
    }

    #[must_use]
    pub const fn right_group(mut self, right: StatusRightGroup<'a>) -> Self {
        self.right = right;
        self
    }

    #[must_use]
    pub const fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    #[must_use]
    pub const fn left_hover(mut self, left_hover: bool) -> Self {
        self.hover.left = left_hover;
        self
    }

    #[must_use]
    pub const fn right_hover(mut self, right_hover: bool) -> Self {
        self.hover.right = right_hover;
        self
    }

    #[must_use]
    pub const fn usage_hover(mut self, hovered: bool) -> Self {
        self.hover.usage = hovered;
        self
    }

    #[must_use]
    pub const fn right_debug_hover(mut self, hovered: bool) -> Self {
        self.hover.right_debug = hovered;
        self
    }
}

impl Widget for StatusFooter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::default()
            .style(
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(Color::Black),
            )
            .render(area, buf);

        let right_layout = status_right_group_layout(area.width, self.right);
        let mut right_spans: Vec<Span<'static>> = Vec::new();
        if let Some(usage) = right_layout.usage {
            right_spans.push(Span::styled(
                usage.text,
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(faded(
                        if self.hover.usage {
                            DEBUG_AMBER
                        } else {
                            Color::Black
                        },
                        self.alpha,
                    ))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(container) = right_layout.container {
            right_spans.push(Span::styled(
                container.text,
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(faded(
                        if self.hover.right {
                            DEBUG_AMBER
                        } else {
                            LINK_BLUE
                        },
                        self.alpha,
                    ))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(spend) = right_layout.spend {
            right_spans.push(Span::styled(
                spend.text,
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(faded(Color::Black, self.alpha))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(debug) = right_layout.run_id {
            // Canonical debug chip: DANGER_RED background, white text — identical to
            // the console's render_debug_bar so the operator sees the same chip on
            // every surface. Inverted on hover (white bg, red text) for clickability cue.
            let (chip_bg, chip_fg) = if self.hover.right_debug {
                (WHITE, DANGER_RED)
            } else {
                (DANGER_RED, WHITE)
            };
            right_spans.push(Span::styled(
                debug.text,
                Style::default()
                    .bg(faded(chip_bg, self.alpha))
                    .fg(faded(chip_fg, self.alpha))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let right_width = u16::try_from(
            right_spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>(),
        )
        .unwrap_or(u16::MAX);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(right_width)])
            .split(area);

        let activity_fg = if self.hover.left {
            faded(LINK_BLUE, self.alpha)
        } else {
            Color::Black
        };
        let left_spans = vec![
            Span::raw(" "),
            Span::styled(
                self.left.to_owned(),
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(activity_fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        let activity = Line::from(left_spans);
        Paragraph::new(activity).render(cols[0], buf);

        if !right_spans.is_empty() {
            Paragraph::new(Line::from(right_spans))
                .alignment(Alignment::Right)
                .render(cols[1], buf);
        }
    }
}

pub fn render_status_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    left: &str,
    right: &str,
    right_debug: Option<&str>,
    alpha: f32,
    hover: StatusFooterHover,
) {
    frame.render_widget(
        StatusFooter::new(left)
            .right(right)
            .right_debug(right_debug)
            .alpha(alpha)
            .left_hover(hover.left)
            .usage_hover(hover.usage)
            .right_hover(hover.right)
            .right_debug_hover(hover.right_debug),
        area,
    );
}

pub fn render_status_footer_right_group(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    left: &str,
    right: StatusRightGroup<'_>,
    alpha: f32,
    hover: StatusFooterHover,
) {
    frame.render_widget(
        StatusFooter::new(left)
            .right_group(right)
            .alpha(alpha)
            .left_hover(hover.left)
            .usage_hover(hover.usage)
            .right_hover(hover.right)
            .right_debug_hover(hover.right_debug),
        area,
    );
}

#[must_use]
pub fn compact_usage_status_label(label: &str) -> String {
    let parts = label
        .split(" · ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let remaining = parts
        .iter()
        .find(|part| part.starts_with("Session ") || part.starts_with("5-hour "))
        .or_else(|| parts.iter().find(|part| part.contains('%')))
        .map(|part| (*part).to_owned());
    let state = parts
        .iter()
        .rev()
        .find_map(|part| usage_lifecycle_word(part));
    match (remaining, state) {
        (Some(remaining), Some(state)) => format!("{remaining} · {state}"),
        (Some(remaining), None) => remaining,
        (None, Some(state)) => state.to_owned(),
        (None, None) => label
            .split_whitespace()
            .next()
            .unwrap_or("usage")
            .to_owned(),
    }
}

fn usage_lifecycle_word(part: &str) -> Option<&'static str> {
    let lower = part.to_ascii_lowercase();
    [
        "login",
        "secret",
        "stale",
        "unsupported",
        "unavailable",
        "error",
    ]
    .into_iter()
    .find(|word| lower.contains(word))
}

#[must_use]
pub fn status_footer_right_chip_rect(
    area: Rect,
    right: &str,
    right_debug: Option<&str>,
) -> Option<Rect> {
    status_right_group_rect(
        area,
        status_right_group_layout(
            area.width,
            StatusRightGroup {
                usage: None,
                spend: None,
                container: right,
                run_id: right_debug,
            },
        )
        .container,
    )
}

/// Return the rect of the **debug chip** (`right_debug`) on the status bar,
/// regardless of whether the instance-id chip (`right`) is present.
///
/// Use this instead of `status_footer_right_chip_rect` when the caller only
/// shows the debug chip (no instance chip), as on the console debug bar where
/// `right` is empty.
#[must_use]
pub fn status_footer_debug_chip_rect(area: Rect, right_debug: &str) -> Option<Rect> {
    status_right_group_rect(
        area,
        status_right_group_layout(
            area.width,
            StatusRightGroup {
                usage: None,
                spend: None,
                container: "",
                run_id: Some(right_debug),
            },
        )
        .run_id,
    )
}

fn status_right_group_rect(area: Rect, chunk: Option<StatusRightChunk>) -> Option<Rect> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let chunk = chunk?;
    let x_offset = chunk.start.saturating_sub(1);
    let width = chunk.end.saturating_sub(chunk.start);
    Some(Rect {
        x: area.x.saturating_add(x_offset),
        y: area.y,
        width: width.min(area.width),
        height: area.height,
    })
}

#[cfg(test)]
mod tests;
