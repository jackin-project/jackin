//! Tests for `socket_backend`.
use ratatui::{
    Terminal,
    backend::{Backend, ClearType},
    layout::{Position, Rect},
    style::{Color, Modifier},
    text::Span,
    widgets::Paragraph,
};

use super::{CellStyle, SocketBackend};
use jackin_term::DamageGrid;

#[test]
fn backend_renders_text_to_output_buffer() {
    let backend = SocketBackend::new(10, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new(Span::raw("hi")), area);
        })
        .unwrap();
    let output = terminal.backend_mut().take_output();
    // Each character gets its own cursor-positioning sequence; verify
    // both letters appear in the output.
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains('h') && text.contains('i'),
        "expected 'h' and 'i' in output: {text:?}"
    );
}

#[test]
fn resize_updates_reported_size() {
    let mut backend = SocketBackend::new(80, 24);
    backend.current_style = CellStyle {
        fg: Color::Red,
        bg: Color::Blue,
        modifiers: Modifier::BOLD,
    };
    backend.resize(120, 40);
    let size = backend.size().unwrap();
    assert_eq!(size.width, 120);
    assert_eq!(size.height, 40);
    assert_eq!(backend.current_style, CellStyle::default());
}

#[test]
fn full_screen_clear_resets_style_before_erasing() {
    let mut backend = SocketBackend::new(80, 24);
    backend.current_style = CellStyle {
        fg: Color::Black,
        bg: jackin_tui::theme::PHOSPHOR_GREEN,
        modifiers: Modifier::BOLD,
    };

    backend.clear_region(ClearType::All).unwrap();

    let output = backend.take_output();
    assert!(
        output.starts_with(b"\x1b[0m\x1b[2J\x1b[H"),
        "screen erase must reset SGR first so BCE does not clear with green background: {:?}",
        String::from_utf8_lossy(&output)
    );
    assert_eq!(backend.current_style, CellStyle::default());
}

#[test]
fn take_output_drains_buffer() {
    let backend = SocketBackend::new(10, 1);
    let terminal = Terminal::new(backend).unwrap();
    drop(terminal); // do not call draw
    let mut backend = SocketBackend::new(10, 1);
    // Push directly for simplicity.
    backend.output.extend_from_slice(b"hello");
    let first = backend.take_output();
    let second = backend.take_output();
    assert_eq!(first, b"hello");
    assert!(second.is_empty());
}

#[test]
fn drain_output_into_preserves_backend_capacity() {
    let mut backend = SocketBackend::new(10, 1);
    backend.output.extend_from_slice(b"hello");
    let capacity = backend.output.capacity();
    let mut output = Vec::new();

    backend.drain_output_into(&mut output);

    assert_eq!(output, b"hello");
    assert!(backend.output.is_empty());
    assert_eq!(backend.output.capacity(), capacity);
}

#[test]
fn cursor_movement_uses_1_based_coords() {
    let backend = SocketBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(Span::styled("X", ratatui::style::Style::default())),
                Rect::new(2, 3, 1, 1),
            );
        })
        .unwrap();
    let output = terminal.backend_mut().take_output();
    let text = String::from_utf8_lossy(&output);
    // Row 3 (0-based) → row 4 (1-based), col 2 → col 3
    assert!(
        text.contains("\x1b[4;3H") || text.contains("\x1b[4;1H"),
        "expected cursor at row 4: {text:?}"
    );
}

#[test]
fn cursor_movement_encodes_four_digit_coords() {
    let mut backend = SocketBackend::new(1200, 1200);

    backend
        .set_cursor_position(Position { x: 1000, y: 999 })
        .unwrap();

    assert_eq!(backend.take_output(), b"\x1b[1000;1001H");
}

#[test]
fn grid_patch_encoder_emits_only_changed_cell_span() {
    let mut grid = DamageGrid::new(3, 12, 100);
    let mut backend = SocketBackend::new(12, 3);
    let area = Rect::new(0, 0, 12, 3);

    grid.process(b"\x1b[1;1Halpha\x1b[2;1Hbeta");
    {
        let patch = grid.dump_dirty_patch();
        backend.draw_grid_patch(area, &patch);
    }
    backend.take_output();

    grid.process(b"\x1b[2;3HZ");
    {
        let patch = grid.dump_dirty_patch();
        assert_eq!(patch.changed_cell_count(), 1);
        backend.draw_grid_patch(area, &patch);
    }

    let output = backend.take_output();
    assert_eq!(String::from_utf8_lossy(&output), "\x1b[2;3HZ");
}
