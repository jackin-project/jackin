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

#[test]
fn render_single_line_message_has_one_blank_row_before_ok() {
    let state = ErrorPopupState::new(
        "Load role failed",
        "Repository is not available, or you do not have access.",
    );
    let area = Rect::new(0, 0, 90, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(ErrorDialog::new(&state), area))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let row_string = |y| {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let message_y = (0..buffer.area.height)
        .find(|y| row_string(*y).contains("Repository is not available"))
        .expect("message row should render");
    let ok_y = (0..buffer.area.height)
        .find(|y| row_string(*y).contains("OK"))
        .expect("OK row should render");

    assert_eq!(
        ok_y,
        message_y + 2,
        "exactly one blank row should separate message and OK"
    );
    let spacer = row_string(message_y + 1);
    assert!(
        !spacer.contains("Repository") && !spacer.contains("OK"),
        "spacer row should be blank between message and OK: {spacer:?}"
    );
}
