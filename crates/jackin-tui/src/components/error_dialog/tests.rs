//! Tests for `error_dialog`.
use super::*;
use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn enter_dismisses() {
    let state = ErrorPopupState::new("Save failed", "workspace already exists");
    assert!(matches!(
        state.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn required_height_respects_caller_supplied_max() {
    let state = ErrorPopupState::new("Save failed", "word ".repeat(500));
    assert!(required_height(&state, 30, 15) <= 15);
    assert!(required_height(&state, 30, 1) >= 7);
}

#[test]
fn render_single_line_message_is_visible() {
    let state = ErrorPopupState::new("Role not found", "repository not found");
    let area = Rect::new(0, 0, 60, required_height(&state, 56, 25));
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(ErrorDialog::new(&state), area))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    assert!(
        rendered.contains("repository not found"),
        "message should be visible in popup:\n{rendered}"
    );
}
