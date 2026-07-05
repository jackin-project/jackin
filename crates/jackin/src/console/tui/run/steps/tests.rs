use super::*;

fn fresh_state() -> ConsoleState {
    let config = AppConfig::default();
    let cwd = std::env::temp_dir();
    jackin_console::tui::console::new_console_state(&config, &cwd).expect("console state")
}

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

#[test]
fn quit_confirm_key_n_dismisses_and_continues() {
    let mut state = fresh_state();
    state.open_quit_confirm();

    let step = handle_quit_key_step(&mut state, key(crossterm::event::KeyCode::Char('n')));

    assert!(matches!(step, Some(ConsoleLoopStep::Continue)));
    assert!(!state.quit_confirm_open());
}

#[test]
fn quit_confirm_key_y_exits() {
    let mut state = fresh_state();
    state.open_quit_confirm();

    let step = handle_quit_key_step(&mut state, key(crossterm::event::KeyCode::Char('y')));

    assert!(matches!(step, Some(ConsoleLoopStep::Exit(None))));
}
