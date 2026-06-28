//! Launch cockpit header rendering.

use jackin_tui::components::brand_header_line;
use jackin_tui::theme::WHITE;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use crate::LaunchView;

/// Top header: the ` jackin❯ ` brand pill and separator (from the shared
/// `brand_header_line` component), then the loading line (`Loading <role> in <path>`).
///
/// Uses `brand_header_line` so the pill styling stays in sync with the console
/// manager and the lookbook — if the brand pill ever changes, the cockpit updates
/// automatically without a separate code path (RULE 10: brand chrome reuse).
pub fn render_cockpit_header(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    // brand_header_line emits: [pill][sep][label]. We want [pill][sep][loading spans],
    // so we take the first two spans (pill + sep) and replace the label with our
    // animated loading line.
    let mut brand_line = brand_header_line("launch");
    // Drop the static label span and append the animated loading spans instead.
    brand_line.spans.pop();
    brand_line.spans.extend(loading_line_spans(view, frozen));
    frame.render_widget(Paragraph::new(brand_line), area);
}

/// The `Loading <role> in <path>` line: one green colour throughout, the role
/// and the path **bold**, with a brightness ripple sweeping left→right so the
/// text reads as actively loading.
fn loading_line_spans(view: &LaunchView, frozen: bool) -> Vec<Span<'static>> {
    let Some(id) = view.identity.as_ref() else {
        return vec![Span::styled(
            "Preparing launch...",
            Style::default().fg(WHITE),
        )];
    };
    let prep = " in ";
    // Flatten to (char, kind): 0 = normal ("Loading" / "in"), 1 = role,
    // 2 = path. The role renders white so it pops; the rest stays green. Role
    // and path are bold. The ripple brightens every glyph uniformly.
    let mut chars: Vec<(char, u8)> = Vec::new();
    for ch in "Loading ".chars() {
        chars.push((ch, 0));
    }
    for ch in id.role.chars() {
        chars.push((ch, 1));
    }
    for ch in prep.chars() {
        chars.push((ch, 0));
    }
    for ch in id.target_label.chars() {
        chars.push((ch, 2));
    }

    let len = chars.len();
    let lerp = |a: u8, b: u8, t: f32| (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)) as u8;
    // A bright band sweeps across the line every ~len+16 frames.
    let period = (len + 16) as f32;
    let peak = (view.frame as f32 % period) - 8.0;
    coalesce_cells(chars.into_iter().enumerate().map(|(i, (ch, kind))| {
        let bright = if frozen {
            0.0
        } else {
            (1.0 - (i as f32 - peak).abs() / 5.0).max(0.0)
        };
        let color = if kind == 0 {
            // "Loading" / "in": green, dim → bright on the ripple.
            Color::Rgb(
                lerp(0, 120, bright),
                lerp(140, 255, bright),
                lerp(30, 120, bright),
            )
        } else {
            // Role + path: white, brightening dim-white → full white.
            let v = lerp(170, 255, bright);
            Color::Rgb(v, v, v)
        };
        let mut style = Style::default().fg(color);
        if kind != 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        (ch, style)
    }))
}

/// Coalesce per-cell `(char, Style)` pairs into the fewest spans: consecutive
/// cells sharing a style merge into one span. Render paths that compute a style
/// per glyph would otherwise allocate one `Span` plus one `String` per
/// character every frame.
fn coalesce_cells(cells: impl IntoIterator<Item = (char, Style)>) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut cur: Option<Style> = None;
    for (ch, style) in cells {
        if cur != Some(style) {
            if let Some(prev) = cur.take() {
                spans.push(Span::styled(std::mem::take(&mut buf), prev));
            }
            cur = Some(style);
        }
        buf.push(ch);
    }
    if let Some(prev) = cur {
        spans.push(Span::styled(buf, prev));
    }
    spans
}
