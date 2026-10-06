//! jackin❯ brand header composition.
//!
//! Brand-proof template (the mechanism every later brand composition —
//! launch rain/warp/rail, the capsule pill — copies at its owning phase):
//!
//! - The look gate is a zero-tolerance PNG crop of row 0 per non-modal
//!   console stage view, baselined in the dedicated brand-crop directory
//!   (`tui/view/baselines/png/brand/`) and blessed only through
//!   `JACKIN_BLESS_BRAND_PNGS=1` — never by a surrounding screen's re-bless.
//! - The 12 literal-RGB span tests (3 here, plus the launch header/rail and
//!   capsule chrome duplicates) are the standing value-level gate and are
//!   never edited to match new output.
//! - A brand-crop diff outside an intended brand change is a parity break:
//!   STOP for operator review, never re-bless silently; re-bless only as a
//!   deliberate, reviewed act like any baseline.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use termrock::text::LinePlacement;

#[derive(Debug, Clone, Copy)]
struct BrandHeader<'a> {
    label: &'a str,
}

impl Widget for BrandHeader<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let mut scratch = String::new();
        termrock::text::paint_line_overflow(
            buffer,
            area,
            &brand_header_line(self.label),
            Style::default(),
            LinePlacement::clipped("…"),
            &mut scratch,
        );
    }
}

fn brand_header_line(label: &str) -> Line<'static> {
    let block = Style::default()
        .bg(jackin_tui::tokens::BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);
    // The chevron/separator/label pin jackin❯-owned brand constants: head's
    // palette recolored the roles they used to read, and the brand look is an
    // invariant across the bump.
    Line::from(vec![
        Span::styled(" jackin", block.fg(jackin_tui::tokens::INK)),
        Span::styled("❯", block.fg(jackin_tui::tokens::BRAND_CHEVRON)),
        Span::styled(" ", block),
        Span::styled(
            " · ",
            Style::default().fg(jackin_tui::tokens::BRAND_SEPARATOR),
        ),
        Span::styled(
            label.to_owned(),
            Style::default().fg(jackin_tui::tokens::BRAND_LABEL),
        ),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader { label }, area);
}

#[cfg(test)]
mod tests;
