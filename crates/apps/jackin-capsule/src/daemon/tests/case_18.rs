// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn double_click_selects_word_and_copies_once() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let inner = mux.visible_panes()[0].inner;
    // Cell (0, 6) sits inside "/model" (content columns 4..=9).
    let row = inner.row;
    let col = inner.col + 6;

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(
        mux.clipboard.selection.is_none(),
        "first press must stay a plain click"
    );
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));

    let sel = mux
        .clipboard
        .selection
        .expect("double-click selects the word");
    assert_eq!(
        (sel.anchor_row, sel.anchor_col, sel.end_row, sel.end_col),
        (0, 4, 0, 9),
        "selection must cover exactly /model"
    );
    assert!(
        mux.clipboard.selection_copied,
        "word selection copies immediately"
    );
    mux.client_registry.client.flush_out_of_band();
    let clipboard = rx.try_recv().expect("word selection writes OSC 52");
    let needle = crate::tui::view::encode_osc52_clipboard_write("/model");
    assert!(
        clipboard
            .windows(needle.len())
            .any(|w| w == needle.as_slice()),
        "clipboard write must carry the bare word: {:?}",
        String::from_utf8_lossy(&clipboard)
    );

    // The release that ends the double-click must not copy again or drop
    // the highlight.
    drop(apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col,
            button: 0,
        },
    ));
    assert!(
        mux.clipboard.selection.is_some(),
        "word selection stays highlighted after release"
    );
    mux.client_registry.client.flush_out_of_band();
    rx.try_recv()
        .expect_err("release after a word click must not write the clipboard twice");
}

#[test]
fn double_click_window_requires_same_cell_within_500ms() {
    use std::time::{Duration, Instant};

    use super::mouse_input::{PanePress, is_double_click};

    let base = Instant::now();
    let press = |session_id, content_row, col, at| PanePress {
        session_id,
        content_row,
        col,
        at,
    };
    let first = press(1, 4, 7, base);
    let quick = press(1, 4, 7, base + Duration::from_millis(100));
    let slow = press(1, 4, 7, base + Duration::from_millis(900));
    let other_col = press(1, 4, 8, base + Duration::from_millis(100));
    let other_row = press(1, 5, 7, base + Duration::from_millis(100));
    let other_session = press(2, 4, 7, base + Duration::from_millis(100));

    assert!(is_double_click(&first, &quick));
    assert!(!is_double_click(&first, &slow), "outside the 500 ms window");
    assert!(!is_double_click(&first, &other_col));
    assert!(!is_double_click(&first, &other_row));
    assert!(!is_double_click(&first, &other_session));
}

#[tokio::test]
async fn open_host_url_dialog_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.apply_dialog_action(DialogAction::OpenHostUrl(
        "https://github.com/jackin-project/jackin/pull/565".to_owned(),
    ));

    let bytes = rx.try_recv().expect("host-open-url frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://github.com/jackin-project/jackin/pull/565".to_owned())
    );
}

#[test]
fn open_host_url_dialog_action_honors_operator_opt_out() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.open_host_url_from_dialog(
        "https://github.com/jackin-project/jackin/pull/565".to_owned(),
        false,
    );

    rx.try_recv()
        .expect_err("disabled host URL opening must not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link opening disabled by JACKIN_OPEN_LINKS")
    );
}

#[test]
fn open_host_url_dialog_action_rejects_unsupported_scheme() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.open_host_url_from_dialog("file:///Users/operator/private.txt".to_owned(), true);

    rx.try_recv()
        .expect_err("unsupported host URL schemes must not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[test]
fn host_url_open_policy_honors_operator_opt_out_values() {
    assert!(mouse_input::host_url_opening_allowed_for(None));
    assert!(mouse_input::host_url_opening_allowed_for(Some("allow")));
    for value in ["deny", "off", "no"] {
        assert!(
            !mouse_input::host_url_opening_allowed_for(Some(value)),
            "{value} should disable host URL opening"
        );
    }
}

#[tokio::test]
async fn modified_click_visible_url_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"visit https://example.com/jackin-preflight-url now\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("visit https://exa".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );

    input_rx.try_recv().expect_err(
        "host-open URL gesture should not forward mouse bytes to a mouse-disabled pane",
    );
    let bytes = rx.try_recv().expect("host-open-url frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/jackin-preflight-url".to_owned())
    );
}

#[tokio::test]
async fn modified_click_in_mouse_enabled_pane_forwards_to_pty() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1000h\x1b[?1006hvisit https://example.com/rich-tui now\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("visit https://exa".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );

    let forwarded = input_rx
        .try_recv()
        .expect("modified click should forward to mouse-enabled pane");
    assert!(
        forwarded.starts_with(b"\x1b[<8;"),
        "unexpected forwarded mouse bytes: {forwarded:02x?}"
    );
    rx.try_recv()
        .expect_err("mouse-enabled pane should not emit a host-open-url frame");
}

#[tokio::test]
async fn modified_click_visible_file_path_sends_file_export_frames() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("workspace");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();

    let mut mux = single_pane_tab_mux();
    mux.launch_env.workdir = workdir.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.client_registry.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"artifact report.txt ready\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("artifact report".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );
    mux.client_registry.client.flush_out_of_band();

    input_rx
        .try_recv()
        .expect_err("modified file export must not forward mouse bytes to the pane");
    let bytes = rx.try_recv().expect("file-export-start frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode file-export-start frame")
        .expect("file-export-start frame");
    let ServerFrame::FileExportStart(start) = frame else {
        panic!("expected FileExportStart");
    };
    assert_eq!(
        start.source_path,
        workdir
            .join("report.txt")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(start.file_name, "report.txt");
    assert!(!start.reveal_after_export);
    assert!(!start.open_after_export);
}

#[test]
fn modified_click_plain_word_without_file_falls_through_quietly() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"plain words only\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("plain wo".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );
    mux.client_registry.client.flush_out_of_band();

    rx.try_recv()
        .expect_err("plain modified-click must not emit host frames");
    assert_eq!(mux.clipboard.clipboard_image_notice.as_deref(), None);
}

#[tokio::test]
async fn modified_click_prefers_osc8_target_over_visible_text() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07osc8_link\x1b]8;;\x07 and https://example.com/visible",
    );
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 1,
            button: 8,
        },
    );

    input_rx
        .try_recv()
        .expect_err("modified-click should stay host-open path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should prefer OSC 8 target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/osc8".to_owned())
    );
}
