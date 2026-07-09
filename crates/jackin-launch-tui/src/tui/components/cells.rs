//! Per-cell rendering utilities shared by launch-local animated components.

use ratatui::style::Style;
use ratatui::text::Span;

/// Coalesce per-cell `(char, Style)` pairs into the fewest spans.
pub(crate) fn coalesce_cells<I>(cells: I) -> Vec<Span<'static>>
where
    I: IntoIterator<Item = (char, Style)>,
{
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (ch, style) in cells {
        if let Some(last) = spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push(ch);
            continue;
        }
        spans.push(Span::styled(ch.to_string(), style));
    }
    spans
}
