// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn exit_dirty_down_arrow_recomposes_a_changed_frame() {
    let mut mux = test_mux(30, 100);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    // Paint the modal once so rendered == frame generation.
    let first = mux.compose_pending_frame();

    // Down arrow should invalidate and produce a non-empty, *changed* frame.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert!(
        mux.has_pending_render(),
        "down arrow on the modal must mark a pending render"
    );
    let second = mux.compose_pending_frame();
    assert!(!second.is_empty(), "down arrow must recompose a frame");
    assert_ne!(
        first, second,
        "the recomposed frame must differ once the selection moved"
    );
}

#[test]
fn exit_dirty_marker_moves_on_screen_with_zero_panes() {
    let (rows, cols) = (44u16, 157u16);
    let mut mux = test_mux(rows, cols);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed \u{b7} 3 unpushed".to_owned()],
        Arc::from([]),
    ));
    mux.invalidate(FullRedrawReason::DialogChange);
    let mut grid = DamageGrid::new(rows, cols, 0);

    grid.process(&mux.compose_pending_frame());
    let before = marker_row_on_screen(&grid, rows, cols);

    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42])); // down
    grid.process(&mux.compose_pending_frame());
    let after = marker_row_on_screen(&grid, rows, cols);

    assert!(
        before.is_some(),
        "marker must be visible on screen initially"
    );
    assert!(
        after > before,
        "down arrow must move the rendered \u{25b8} marker: before={before:?} after={after:?}"
    );
}

#[test]
fn exit_dirty_marker_moves_after_session_exits_realistic() {
    // Reproduce the real path: a live agent pane, the session exits, the
    // dirty-exit modal opens with zero panes, then the operator presses down.
    // The client is a persistent VirtualClient (its grid carries the agent
    // screen forward), so this exercises the exact diff baseline the operator's
    // terminal sees — unlike a modal opened on a blank mux.
    let (rows, cols) = (44u16, 157u16);
    let mut mux = single_pane_tab_mux_with_size(rows, cols);
    let (session, _rx) = test_session(rows, cols);
    mux.session_supervisor.sessions.insert(1, session);
    let mut client = VirtualClient::new(rows, cols);

    // Agent paints something; the client mirrors it.
    feed_and_compose(&mut mux, &mut client, 1, b"agent output here\r\n");

    // The session exits — the daemon removes it (zero panes now).
    mux.remove_exited_session(1);

    // handle_last_session_exit opens the modal and invalidates.
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed \u{b7} 3 unpushed".to_owned()],
        Arc::from([]),
    ));
    mux.invalidate(FullRedrawReason::DialogChange);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    let before = marker_row_on_screen(&client.grid, rows, cols);

    // Operator presses down.
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::Data(vec![0x1b, 0x5b, 0x42]),
    );
    let after = marker_row_on_screen(&client.grid, rows, cols);

    assert!(
        before.is_some(),
        "modal marker must be visible after session exit"
    );
    assert!(
        after > before,
        "down arrow must move the rendered marker on the operator's screen: before={before:?} after={after:?}"
    );
}

#[tokio::test]
async fn last_session_exit_does_not_repush_modal_while_dialog_open() {
    // Regression: the event loop calls handle_last_session_exit on every client
    // frame while no sessions are live. With the dirty-exit modal already open,
    // re-entering must NOT push a second modal (which reset the selection to 0
    // every keypress, capping navigation at row 1).
    let mut mux = test_mux(44, 157);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    let depth_before = mux.control.dialog_stack.len();

    let exited = handle_last_session_exit(&mut mux, None).await;

    assert!(!exited, "must keep the loop alive while the modal is open");
    assert_eq!(
        mux.control.dialog_stack.len(),
        depth_before,
        "must not re-push the modal while a dialog is already open"
    );
}

#[test]
fn exit_dirty_down_arrow_reaches_last_row() {
    // The selection must advance all the way to the final row (Discard), not cap
    // at row 1 — guards against an off-by-one or re-push regression.
    let mut mux = test_mux(44, 157);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    for _ in 0..5 {
        mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42])); // down
    }
    match mux.dialog_top() {
        Some(Dialog::ExitDirty { selected, .. }) => {
            assert_eq!(
                *selected, 3,
                "five downs must land on the last row (Discard)"
            );
        }
        other => panic!("expected ExitDirty, got {other:?}"),
    }
}

#[test]
fn build_exit_inspect_rows_groups_repos_with_header_and_file_rows() {
    use crate::exit_assess::DirtyRepo;
    use crate::tui::components::dialog::InspectRow;
    use jackin_core::ChangedFile;

    let repos = vec![
        DirtyRepo {
            path: "/workspace/alpha".to_owned(),
            changed: vec![
                ChangedFile {
                    status: 'M',
                    path: "src/main.rs".to_owned(),
                },
                ChangedFile {
                    status: '?',
                    path: "new.rs".to_owned(),
                },
            ],
            unpushed: 0,
        },
        DirtyRepo {
            path: "/workspace/beta".to_owned(),
            changed: vec![],
            unpushed: 1,
        },
    ];
    let rows = build_exit_inspect_rows(&repos);
    // First entry must be a Repo header.
    assert!(matches!(rows.first(), Some(InspectRow::Repo(_))));
    // Two repos → exactly two Repo headers.
    let repo_count = rows
        .iter()
        .filter(|r| matches!(r, InspectRow::Repo(_)))
        .count();
    assert_eq!(repo_count, 2, "one header per repo");
    // alpha has two changed files → two File rows follow its header.
    let file_count = rows
        .iter()
        .filter(|r| matches!(r, InspectRow::File(_)))
        .count();
    assert_eq!(file_count, 2, "only changed files produce File rows");
    // Repo labels are derived from the final path component.
    if let Some(InspectRow::Repo(label)) = rows.first() {
        assert_eq!(label, "alpha");
    }
    // File rows are formatted as "<status> <path>".
    let file_rows: Vec<_> = rows
        .iter()
        .filter_map(|r| {
            if let InspectRow::File(s) = r {
                Some(s.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(file_rows, ["M src/main.rs", "? new.rs"]);
}

#[tokio::test(start_paused = true)]
async fn detach_attached_task_sends_shutdown_and_aborts_reader() {
    use crate::attach_protocol::detach_attached_task;
    use jackin_protocol::attach::TAG_SHUTDOWN;
    use std::time::Duration;
    use tokio::sync::mpsc;

    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    mux.client_registry.client.attach(tx);
    let parked = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    mux.client_registry.attached_task = Some(parked);
    detach_attached_task(&mut mux, "takeover").await;
    let frame = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting shutdown")
        .expect("channel closed without shutdown");
    assert_eq!(frame[0], TAG_SHUTDOWN);
    assert!(
        mux.client_registry.attached_task.is_none()
            || mux
                .client_registry
                .attached_task
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished)
    );
}

#[tokio::test]
async fn client_writer_attach_displaces_prior_sender() {
    use jackin_protocol::attach::{ServerFrame, TAG_BELL};
    use std::time::Duration;
    use tokio::sync::mpsc;

    let mut mux = single_pane_tab_mux();
    let (tx_a, mut rx_a) = mpsc::unbounded_channel::<Vec<u8>>();
    let (tx_b, mut rx_b) = mpsc::unbounded_channel::<Vec<u8>>();
    mux.client_registry.client.attach(tx_a);
    mux.client_registry.client.attach(tx_b);
    mux.client_registry
        .client
        .send_protocol_frame(ServerFrame::Bell);
    assert!(
        rx_a.try_recv().is_err(),
        "prior client must be displaced (A must not receive)"
    );
    let got = tokio::time::timeout(Duration::from_secs(1), rx_b.recv())
        .await
        .expect("timeout waiting for B")
        .expect("B closed");
    assert_eq!(got[0], TAG_BELL);
}

#[tokio::test]
async fn input_frames_apply_to_post_takeover_mux() {
    use crate::attach_protocol::detach_attached_task;
    use crate::daemon::control::handle_client_frame;
    use jackin_protocol::attach::ClientFrame;
    use std::time::Duration;
    use tokio::sync::mpsc;

    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session(24, 80);
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Test", 1, "test");

    let (tx_old, _rx_old) = mpsc::unbounded_channel::<Vec<u8>>();
    mux.client_registry.client.attach(tx_old);
    detach_attached_task(&mut mux, "takeover").await;
    let (tx_new, _rx_new) = mpsc::unbounded_channel::<Vec<u8>>();
    mux.client_registry.client.attach(tx_new);

    handle_client_frame(&mut mux, ClientFrame::Input(b"x".to_vec()));
    let got = tokio::time::timeout(Duration::from_secs(1), input_rx.recv())
        .await
        .expect("timeout waiting for session input")
        .expect("session input channel closed");
    assert_eq!(got, b"x");
}

#[test]
fn remove_exited_session_retires_codename_inv_d8() {
    // INV-D8: removing an exited session retires its codename so labels update.
    let mut mux = single_pane_tab_mux();
    let (session, _rx) = test_session(24, 80);
    mux.session_supervisor.sessions.insert(1, session);
    // Seed live set the way spawn_session would after pick_next_codename.
    mux.session_supervisor
        .codename_live
        .insert("test".to_owned());
    assert!(
        mux.session_supervisor.codename_live.contains("test"),
        "precondition: codename live before exit"
    );
    assert!(
        !mux.session_supervisor.codename_retired.contains("test"),
        "precondition: not yet retired"
    );

    mux.remove_exited_session(1);

    assert!(
        !mux.session_supervisor.codename_live.contains("test"),
        "exited session codename must leave live set"
    );
    assert!(
        mux.session_supervisor.codename_retired.contains("test"),
        "exited session codename must enter retired set"
    );
    assert!(
        mux.session_supervisor.sessions.is_empty(),
        "session map must drop the exited id"
    );
    assert!(
        mux.session_supervisor.tabs.is_empty(),
        "sole tab owning the session must be removed"
    );
}

#[tokio::test]
async fn last_session_exit_defers_when_dialog_already_open_inv_d19() {
    // INV-D19: when a dirty-exit (or any) dialog is open, last-session exit
    // handling must not re-enter and drain.
    let mut mux = single_pane_tab_mux();
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["repo   1 changed".to_owned()],
        Arc::from([]),
    ));
    assert!(mux.dialog_open(), "precondition: dialog open");

    let should_exit = handle_last_session_exit(&mut mux, Some("test-exit".into())).await;
    assert!(
        !should_exit,
        "handle_last_session_exit must return false while dialog is open"
    );
    assert!(
        mux.dialog_open(),
        "dialog must remain open after deferred last-session exit"
    );
}

#[test]
fn session_send_writes_the_payload_verbatim_into_the_addressed_pty() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "review the diff\r".to_owned(),
        },
    );

    assert!(matches!(
        reply,
        ServerMsg::SessionSent {
            session: 1,
            bytes: 16
        }
    ));
    // Verbatim: the daemon appends no newline, no bracketed-paste wrapper.
    // The submit key is the caller's to include.
    assert_eq!(
        input_rx.try_recv().expect("payload reached the PTY writer"),
        b"review the diff\r".to_vec()
    );
    assert!(
        matches!(input_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "exactly one write"
    );
}
