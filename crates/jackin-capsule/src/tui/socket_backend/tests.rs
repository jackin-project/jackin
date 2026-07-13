// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `socket_backend`.
use ratatui::{
    Terminal,
    backend::{Backend, ClearType},
    layout::{Position, Rect},
    style::{Color, Modifier},
    text::Span,
    widgets::Paragraph,
};

use super::{CellStyle, SgrMetadata, SocketBackend};

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
fn suppressed_clear_resets_style_without_screen_erase() {
    let mut backend = SocketBackend::new(80, 24);
    backend.current_style = CellStyle {
        fg: Color::Red,
        bg: Color::Blue,
        modifiers: Modifier::BOLD,
    };

    backend.begin_clear_suppression();
    backend.clear_region(ClearType::All).unwrap();
    // Sustained: a width-shrink resize clears twice; both stay byte-silent.
    backend.clear_region(ClearType::All).unwrap();
    assert!(
        backend.take_output().is_empty(),
        "suppressed clears must not emit bytes"
    );
    assert_eq!(backend.current_style, CellStyle::default());

    // After lifting suppression, the next clear erases again.
    backend.end_clear_suppression();
    backend.clear_region(ClearType::All).unwrap();
    let output = backend.take_output();
    assert!(
        output.windows(4).any(|w| w == b"\x1b[2J"),
        "unsuppressed clear must erase: {:?}",
        String::from_utf8_lossy(&output)
    );
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
fn backend_emits_extended_visible_sgr_modifiers() {
    let mut backend = SocketBackend::new(10, 1);
    backend.apply_style(
        CellStyle {
            fg: Color::Reset,
            bg: Color::Reset,
            modifiers: Modifier::CROSSED_OUT
                | Modifier::SLOW_BLINK
                | Modifier::RAPID_BLINK
                | Modifier::HIDDEN,
        },
        SgrMetadata::default(),
    );

    let output = backend.take_output();
    let text = String::from_utf8_lossy(&output);
    for sgr in ["\x1b[5m", "\x1b[6m", "\x1b[8m", "\x1b[9m"] {
        assert!(text.contains(sgr), "missing {sgr:?} in {text:?}");
    }
}

/// Render a single styled cell through a one-cell SGR region and return the
/// raw backend output. Shared by the frame-SGR-metadata tests.
fn frame_sgr_output(metadata: SgrMetadata, span: Span<'_>) -> Vec<u8> {
    let backend = SocketBackend::new(10, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .backend_mut()
        .set_sgr_regions(vec![(Rect::new(0, 0, 1, 1), metadata)]);
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new(span), frame.area());
        })
        .unwrap();
    terminal.backend_mut().take_output()
}

#[test]
fn backend_emits_frame_sgr_metadata() {
    let output = frame_sgr_output(
        SgrMetadata {
            underline_style: jackin_term::UnderlineStyle::Curly,
            underline_color: jackin_term::Color::Rgb(12, 34, 56),
            overline: true,
        },
        Span::raw("x"),
    );
    assert_eq!(
        output, b"\x1b[1;1H\x1b[0m\x1b[4:3m\x1b[58;2;12;34;56m\x1b[53mx\x1b[?25l",
        "SGR metadata wire output changed"
    );
    let text = String::from_utf8_lossy(&output);
    for sgr in ["\x1b[4:3m", "\x1b[58;2;12;34;56m", "\x1b[53m"] {
        assert!(text.contains(sgr), "missing {sgr:?} in {text:?}");
    }
}

#[test]
fn backend_emits_indexed_color_sgr() {
    // 256-color fg/bg (`write_color_sgr`) and an indexed underline color
    // (`write_sgr_metadata`) all route through `push_indexed_color_tail`;
    // assert the `38;5;`/`48;5;`/`58;5;` forms emit.
    let output = frame_sgr_output(
        SgrMetadata {
            underline_color: jackin_term::Color::Idx(200),
            ..SgrMetadata::default()
        },
        Span::styled(
            "x",
            ratatui::style::Style::default()
                .fg(Color::Indexed(208))
                .bg(Color::Indexed(17)),
        ),
    );
    let text = String::from_utf8_lossy(&output);
    for sgr in ["\x1b[38;5;208m", "\x1b[48;5;17m", "\x1b[58;5;200m"] {
        assert!(text.contains(sgr), "missing {sgr:?} in {text:?}");
    }
}

#[test]
fn backend_emits_osc8_metadata_wire_snapshot() {
    let backend = SocketBackend::new(10, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.backend_mut().set_hyperlink_regions(vec![(
        Rect::new(0, 0, 1, 1),
        "https://example.test".to_owned(),
    )]);
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new(Span::raw("x")), frame.area());
        })
        .unwrap();

    assert_eq!(
        terminal.backend_mut().take_output(),
        b"\x1b]8;;https://example.test\x1b\\\x1b[1;1Hx\x1b]8;;\x1b\\\x1b[?25l",
        "OSC 8 metadata wire output changed"
    );
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
