//! Tests for `run`.
use super::*;

#[test]
fn forced_select_message_commits_current_index() {
    let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into()]);
    picker.select_index(1);

    let result = update_forced_select(
        &mut picker,
        SelectLoopMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(1));
}

#[test]
fn forced_select_message_ignores_cancel() {
    let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into()]);

    let result = update_forced_select(
        &mut picker,
        SelectLoopMessage::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
    );

    assert_eq!(result, None);
}

#[test]
fn select_prompt_message_commits_option_value() {
    let options = vec!["alpha".into(), "beta".into()];
    let mut picker = SelectListState::new(options.clone());
    picker.select_index(1);

    let result = update_select_prompt(
        &mut picker,
        &options,
        false,
        SelectPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .expect("enter commits")
    .expect("commit succeeds");

    assert_eq!(result, PromptResult::Value("beta".into()));
}

#[test]
fn select_prompt_message_commits_skip_row_when_skippable() {
    let options = vec!["alpha".into(), "beta".into()];
    let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into(), "(skip)".into()]);
    picker.select_index(2);

    let result = update_select_prompt(
        &mut picker,
        &options,
        true,
        SelectPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .expect("enter commits")
    .expect("skip succeeds");

    assert_eq!(result, PromptResult::Skipped);
}

#[test]
fn text_prompt_message_commits_value() {
    let mut input = TextInputState::new("name", "demo");

    let result = update_text_prompt(
        &mut input,
        false,
        TextPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .expect("enter commits")
    .expect("commit succeeds");

    assert_eq!(result, PromptResult::Value("demo".into()));
}

#[test]
fn text_prompt_message_commits_empty_as_skip_when_skippable() {
    let mut input = TextInputState::new_allow_empty("name", "");

    let result = update_text_prompt(
        &mut input,
        true,
        TextPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .expect("enter commits")
    .expect("skip succeeds");

    assert_eq!(result, PromptResult::Skipped);
}

#[test]
fn confirm_prompt_message_commits_confirmation() {
    let mut state = ConfirmState::new("continue?").with_focus_yes();

    let result = update_confirm_prompt(
        &mut state,
        ConfirmPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(true));
}

#[test]
fn confirm_prompt_message_cancel_returns_false() {
    let mut state = ConfirmState::new("continue?");

    let result = update_confirm_prompt(
        &mut state,
        ConfirmPromptMessage::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(false));
}

#[test]
fn prompt_context_lines_maps_semantic_styles() {
    let lines = prompt_context_lines(&[
        PromptContextLine::Emphasis("important".into()),
        PromptContextLine::Blank,
        PromptContextLine::Path("/tmp/worktree".into()),
        PromptContextLine::Muted("choose".into()),
        PromptContextLine::Plain("plain".into()),
    ]);

    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0].spans[0].content, "important");
    assert_eq!(lines[2].spans[0].content, "/tmp/worktree");
    assert_eq!(lines[3].spans[0].content, "choose");
    assert_eq!(lines[4].spans[0].content, "plain");
}

#[test]
fn error_prompt_message_acknowledges_enter() {
    let mut state = ErrorPopupState::new("Failed", "nope");

    let result = update_error_prompt(
        &mut state,
        ErrorPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(()));
}

#[test]
fn error_prompt_message_ignores_navigation() {
    let mut state = ErrorPopupState::new("Failed", "nope");

    let result = update_error_prompt(
        &mut state,
        ErrorPromptMessage::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
    );

    assert_eq!(result, None);
}

#[test]
fn rich_dialog_requirement_message_is_tui_owned() {
    assert_eq!(
        rich_launch_dialog_required_message("launch choice"),
        "launch choice requires the rich launch dialog"
    );
}
