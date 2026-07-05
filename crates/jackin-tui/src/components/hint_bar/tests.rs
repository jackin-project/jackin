//! Tests for `hint_bar`.
use super::*;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn line_styles_keys_and_text_distinctly() {
    let spans = [
        HintSpan::Key("Esc"),
        HintSpan::Text("close"),
        HintSpan::GroupSep,
        HintSpan::Key("↑↓"),
        HintSpan::Text("scroll"),
    ];
    let rendered = line(&spans);
    let joined: String = rendered.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "Esc close   ↑↓ scroll");
    assert!(
        rendered.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(
        !rendered.spans[1]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn styled_hint_spans_identity_matches_line() {
    let spans = [
        HintSpan::Key("Esc"),
        HintSpan::Text("close"),
        HintSpan::Sep,
        HintSpan::DynKey("Ctrl-\\".to_owned()),
        HintSpan::Dyn("menu".to_owned()),
        HintSpan::GroupSep,
        HintSpan::Key("q"),
        HintSpan::Text("quit"),
    ];
    let direct = styled_hint_spans(&spans, |color| color);
    let via_line = line(&spans);

    assert_eq!(direct.len(), via_line.spans.len());
    for (direct, via_line) in direct.iter().zip(&via_line.spans) {
        assert_eq!(direct.content, via_line.content);
        assert_eq!(direct.style, via_line.style);
    }
}

#[test]
fn wrapped_long_wraps_within_width() {
    let items = [
        HintSpan::Key("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::Sep,
        HintSpan::Key("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        HintSpan::Key("R"),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::GroupSep,
        HintSpan::Key("⇥"),
        HintSpan::Text("switch tab"),
        HintSpan::GroupSep,
        HintSpan::Key("S"),
        HintSpan::Text("save settings"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("back"),
    ];
    let lines = wrapped_lines(&items, 60);
    assert!(lines.len() > 1, "should wrap at 60 cols: {lines:?}");
    for line in &lines {
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert!(width <= 60, "line width {width} exceeds 60: {line:?}");
    }
}

#[test]
fn widget_centers_single_row_hint() {
    let backend = TestBackend::new(24, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let items = [HintSpan::Key("Esc"), HintSpan::Text("close")];
    terminal
        .draw(|frame| frame.render_widget(HintBar::new(&items), frame.area()))
        .unwrap();
    let row: String = (0..24)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
        .collect();
    assert!(row.contains("Esc close"), "hint missing: {row:?}");
    assert!(
        row.starts_with("       "),
        "hint should be centered: {row:?}"
    );
}
