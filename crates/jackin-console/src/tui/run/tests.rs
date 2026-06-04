//! Tests for `run`.
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn quit_intercept_opens_off_main_for_bare_q() {
    let state = QuitInterceptState {
        on_main_screen: false,
        consumes_letter_input: false,
    };

    assert!(should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        state,
    ));
    assert!(should_open_quit_confirm(
        key(KeyCode::Char('Q'), KeyModifiers::SHIFT),
        state,
    ));
}

#[test]
fn quit_intercept_ignores_main_text_input_and_modified_keys() {
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        QuitInterceptState {
            on_main_screen: true,
            consumes_letter_input: false,
        },
    ));
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: true,
        },
    ));
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::CONTROL),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: false,
        },
    ));
}

#[test]
fn letter_input_state_detects_text_and_filter_modals() {
    assert!(consumes_letter_input(LetterInputState {
        editor_modal: Some(LetterInputModalKind::TextInput),
        ..LetterInputState::default()
    }));
    assert!(consumes_letter_input(LetterInputState {
        list_modal: Some(LetterInputModalKind::FilterPicker),
        ..LetterInputState::default()
    }));
    assert!(!consumes_letter_input(LetterInputState {
        settings_mount_modal: Some(LetterInputModalKind::Other),
        ..LetterInputState::default()
    }));
    assert!(!consumes_letter_input(LetterInputState::default()));
}

#[test]
fn token_generate_status_message_names_target_scope() {
    assert_eq!(
        token_generate_scope_label(TokenGenerateScopeLabel::Workspace("proj")),
        "workspace \"proj\""
    );
    assert_eq!(
        token_generate_scope_label(TokenGenerateScopeLabel::WorkspaceRole {
            workspace: "proj",
            role: "ops",
        }),
        "workspace \"proj\" role \"ops\""
    );
    assert_eq!(
        token_generate_status_message(TokenGenerateScopeLabel::Global),
        "\nGenerating Claude OAuth token for global config -- complete the browser sign-in, then paste the code below.\n"
    );
}

#[test]
fn debug_run_id_label_uses_empty_fallback() {
    assert_eq!(debug_run_id_label(Some("run-1")), "run-1");
    assert_eq!(debug_run_id_label(None), "");
}
