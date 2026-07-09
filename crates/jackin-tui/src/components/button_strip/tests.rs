//! Tests for `button_strip`.
use super::{ButtonStrip, ButtonStripItem, button_strip_line};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

#[test]
fn button_strip_pads_and_separates_labels() {
    let items = [ButtonStripItem::new("Save"), ButtonStripItem::new("Cancel")];

    let line = button_strip_line(&items, 0, "    ");

    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    // Each label gets 2-space padding on each side; gap is 4 spaces.
    // "  Save  " + "    " + "  Cancel  " = 2+4+2+4+2+6+2 chars.
    assert_eq!(text, "  Save        Cancel  ");
}

#[test]
fn button_rects_match_rendered_button_cells() {
    let items = [ButtonStripItem::new("Save"), ButtonStripItem::new("Cancel")];
    let area = Rect::new(0, 0, 40, 1);
    let strip = ButtonStrip::new(&items);
    let rects = strip.button_rects(area);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();

    term.draw(|frame| frame.render_widget(strip, area)).unwrap();
    let buf = term.backend().buffer();

    for (rect, label) in rects.iter().zip(["Save", "Cancel"]) {
        let rendered = (rect.x..rect.x + rect.width)
            .map(|x| buf[(x, rect.y)].symbol())
            .collect::<String>();
        assert!(
            rendered.contains(label),
            "button {label} must render inside its rect {rect:?}: {rendered:?}"
        );
    }
}

#[test]
fn button_rects_honor_custom_gap() {
    let items = [ButtonStripItem::new("A"), ButtonStripItem::new("B")];
    let rects = ButtonStrip::new(&items)
        .gap("  ")
        .button_rects(Rect::new(0, 0, 20, 1));

    assert_eq!(rects.len(), 2);
    assert_eq!(rects[1].x - (rects[0].x + rects[0].width), 2);
}
