// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `run`.
use super::*;

#[test]
fn forced_select_message_commits_current_index() {
    let mut picker = PromptPicker::new(vec!["alpha".into(), "beta".into()]);
    picker.select_index(1);

    let result = update_forced_select(
        &mut picker,
        SelectLoopMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(1));
}

#[test]
fn forced_select_message_ignores_cancel() {
    let mut picker = PromptPicker::new(vec!["alpha".into(), "beta".into()]);

    let result = update_forced_select(
        &mut picker,
        SelectLoopMessage::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
    );

    assert_eq!(result, None);
}

#[test]
fn select_prompt_message_commits_option_value() {
    let options = vec!["alpha".into(), "beta".into()];
    let mut picker = PromptPicker::new(options.clone());
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
    let mut picker = PromptPicker::new(vec!["alpha".into(), "beta".into(), "(skip)".into()]);
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
    let mut input = PromptText::new("name", "demo");

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
    let mut input = PromptText::new_allow_empty("name", "");

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
    let mut state = PromptConfirm::new("continue?").with_focus_yes();

    let result = update_confirm_prompt(
        &mut state,
        ConfirmPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(true));
}

#[test]
fn confirm_prompt_message_cancel_returns_false() {
    let mut state = PromptConfirm::new("continue?");

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
    let mut state = PromptError::new("Failed", "nope");

    let result = update_error_prompt(
        &mut state,
        ErrorPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );

    assert_eq!(result, Some(()));
}

#[test]
fn error_prompt_message_ignores_navigation() {
    let mut state = PromptError::new("Failed", "nope");

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

#[test]
fn handoff_destroys_backend_before_release_and_delayed_drop_cannot_write() {
    use std::sync::{Arc, Mutex};

    struct BackendDrop(Arc<Mutex<Vec<&'static str>>>);
    impl Drop for BackendDrop {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("backend cursor restore");
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = OwnedTerminal(Some(BackendDrop(Arc::clone(&events))));
    let released = Arc::clone(&events);
    let mut ownership = Some(jackin_core::TerminalOwnershipGuard::new(move || {
        released.lock().unwrap().push("scope release");
    }));
    release_renderer_resources(
        || events.lock().unwrap().push("input joined"),
        &mut terminal.0,
        &mut ownership,
    );
    events.lock().unwrap().push("interactive attach");
    release_renderer_resources(|| {}, &mut terminal.0, &mut ownership);
    drop(terminal);
    assert_eq!(
        *events.lock().unwrap(),
        [
            "input joined",
            "backend cursor restore",
            "scope release",
            "interactive attach",
        ]
    );
}


#[test]
fn unexpected_render_owner_loss_cancels_launch_but_normal_stop_does_not() {
    use std::sync::{Arc, atomic::AtomicBool};
    let cancelled = tokio_util::sync::CancellationToken::new();
    drop(super::owned_render_task(
        cancelled.clone(), Arc::new(AtomicBool::new(false)), std::future::pending(),
    ));
    assert!(cancelled.is_cancelled());

    let completed = tokio_util::sync::CancellationToken::new();
    drop(super::owned_render_task(
        completed.clone(), Arc::new(AtomicBool::new(true)), std::future::pending(),
    ));
    assert!(!completed.is_cancelled());
}

#[tokio::test]
async fn render_owner_abort_before_first_poll_cancels_launch() {
    use std::sync::{Arc, atomic::AtomicBool};
    let cancel = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(super::owned_render_task(
        cancel.clone(), Arc::new(AtomicBool::new(false)), std::future::pending(),
    ));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(cancel.is_cancelled());
}

#[tokio::test]
async fn render_owner_panic_cancels_launch() {
    use std::sync::{Arc, atomic::AtomicBool};
    let cancel = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(super::owned_render_task(
        cancel.clone(), Arc::new(AtomicBool::new(false)), async { panic!("render owner failed"); },
    ));
    assert!(task.await.unwrap_err().is_panic());
    assert!(cancel.is_cancelled());
}

#[tokio::test]
async fn poisoned_owned_renderer_terminates_pending_failure_wait_without_ack() {
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use crate::tui::subscriptions::lock_view;
    let view = Arc::new(Mutex::new(crate::initial_view()));
    crate::update_launch_view(&mut lock_view(&view), crate::LaunchMessage::StageFailed(crate::LaunchFailure {
        title: "Launch failed".to_owned(),
        summary: "fixture stage failed".to_owned(),
        detail: None,
        next_step: None,
        stage: crate::LaunchStage::Network,
    }));
    lock_view(&view).failure_ack = false;
    let renderer = Arc::new(Mutex::new(()));
    let poison = Arc::clone(&renderer);
    assert!(std::thread::spawn(move || {
        let _renderer = poison.lock().unwrap();
        panic!("poison owned renderer");
    }).join().is_err());
    let cancel = tokio_util::sync::CancellationToken::new();
    let owner_cancel = cancel.clone();
    let mut waiting = std::pin::pin!(crate::progress::wait_for_failure_acknowledgement(&view, &cancel));
    {
        use std::future::Future as _;
        assert!(matches!(waiting.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())), std::task::Poll::Pending));
    }
    super::owned_render_task(
        cancel.clone(), Arc::new(AtomicBool::new(false)), async move {
            assert!(matches!(super::try_render_owner(&renderer, &owner_cancel), Err(())));
            assert!(owner_cancel.is_cancelled(), "poison must terminate, not skip frames");
        },
    ).await;
    assert!(cancel.is_cancelled());
    assert!(!lock_view(&view).failure_ack);
    assert_eq!(lock_view(&view).failure.as_ref().unwrap().summary, "fixture stage failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), waiting).await.unwrap();
}

#[tokio::test]
async fn owned_renderer_io_failure_terminates_without_manufacturing_ack() {
    use std::io::Write as _;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use crate::tui::subscriptions::lock_view;
    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "fixture terminal disconnected"))
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let view = Arc::new(Mutex::new(crate::initial_view()));
    crate::update_launch_view(&mut lock_view(&view), crate::LaunchMessage::StageFailed(crate::LaunchFailure {
        title: "Launch failed".to_owned(),
        summary: "fixture stage failed".to_owned(),
        detail: None,
        next_step: None,
        stage: crate::LaunchStage::Network,
    }));
    let cancel = tokio_util::sync::CancellationToken::new();
    let owner_cancel = cancel.clone();
    let mut waiting = std::pin::pin!(crate::progress::wait_for_failure_acknowledgement(&view, &cancel));
    {
        use std::future::Future as _;
        assert!(matches!(waiting.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())), std::task::Poll::Pending));
    }
    super::owned_render_task(
        cancel.clone(), Arc::new(AtomicBool::new(false)), async move {
            let renderer = Mutex::new(FailingWriter);
            let mut renderer = super::try_render_owner(&renderer, &owner_cancel).unwrap().unwrap();
            let draw_result = renderer.write_all(b"failure popup");
            assert_eq!(draw_result.as_ref().unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
            assert!(!super::render_frame_succeeded(&draw_result, &owner_cancel));
            assert!(owner_cancel.is_cancelled(), "draw failure must terminate owner");
        },
    ).await;
    assert!(cancel.is_cancelled());
    assert!(!lock_view(&view).failure_ack);
    assert_eq!(lock_view(&view).failure.as_ref().unwrap().summary, "fixture stage failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), waiting).await.unwrap();
}

#[test]
fn busy_owned_renderer_and_successful_draw_preserve_live_ack_producer() {
    let renderer = std::sync::Mutex::new(());
    let guard = renderer.lock().unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    assert!(super::try_render_owner(&renderer, &cancel).unwrap().is_none());
    assert!(!cancel.is_cancelled());
    drop(guard);
    assert!(super::try_render_owner(&renderer, &cancel).unwrap().is_some());
    assert!(super::render_frame_succeeded(&Ok::<(), std::io::Error>(()), &cancel));
    assert!(!cancel.is_cancelled());
}
