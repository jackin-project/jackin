//! Tests for `confirm_dialog`.
use super::*;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

#[test]
fn y_commits_true() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn uppercase_y_commits_true() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('Y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn n_commits_false() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('n'))),
        ModalOutcome::Commit(false)
    ));
}

#[test]
fn esc_cancels() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn arrow_is_noop() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Down)),
        ModalOutcome::Continue
    ));
}

#[test]
fn default_focus_is_no() {
    let s = ConfirmState::new("Delete?");
    assert_eq!(s.focus, ConfirmFocus::No);
}

#[test]
fn tab_cycles_focus() {
    let mut s = ConfirmState::new("Delete?");
    assert_eq!(s.focus, ConfirmFocus::No);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmFocus::Yes);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmFocus::No);
}

#[test]
fn enter_commits_focused_option() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(false)
    ));

    let mut s = ConfirmState::new("Delete?");
    s.handle_key(key(KeyCode::Tab));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn y_still_works_regardless_of_focus() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn details_prompt_renders_readable_source_details() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let s = ConfirmState::details(
        "Review source",
        "Use this source?",
        vec![
            ("Name".into(), "primary".into()),
            ("Location".into(), "https://example.com/source.git".into()),
        ],
        vec![
            "External content may run commands.".into(),
            "Review the source before continuing.".into(),
        ],
    );
    let area = Rect::new(0, 0, 100, required_height(&s));
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_confirm_dialog(f, area, &s)).unwrap();

    let buf = term.backend().buffer();
    let mut rendered = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            rendered.push_str(buf[(x, y)].symbol());
        }
        rendered.push('\n');
    }

    assert!(rendered.contains("Review source"));
    assert!(rendered.contains("Name: primary"));
    assert!(rendered.contains("Location: https://example.com/source.git"));
    assert!(rendered.contains("External content may run commands."));
    assert!(rendered.contains("Review the source before continuing."));
}
