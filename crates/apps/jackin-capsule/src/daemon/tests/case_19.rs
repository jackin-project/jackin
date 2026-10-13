// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn modified_click_accepts_mailto_osc8_target() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=mail;mailto:operator@example.com\x07email\x1b]8;;\x07 and https://example.com/visible",
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
        .expect("host-open-url frame should allow mailto OSC 8 target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("mailto:operator@example.com".to_owned())
    );
}

#[tokio::test]
async fn modified_click_accepts_visible_mailto_token() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"contact mailto:operator@example.com now\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("contact mailto:opera".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );

    input_rx
        .try_recv()
        .expect_err("modified-click should stay host-open path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should allow visible mailto target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("mailto:operator@example.com".to_owned())
    );
}

#[tokio::test]
async fn modified_click_rejects_unsafe_visible_url_without_forwarding() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"local file:///tmp/report.html now\r\n");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + u16::try_from("local file:///tmp/re".len()).unwrap_or(u16::MAX),
            button: 8,
        },
    );

    input_rx
        .try_recv()
        .expect_err("unsafe host-open gesture should not forward mouse bytes");
    rx.try_recv()
        .expect_err("unsafe host-open gesture should not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn modified_click_rejects_unsafe_osc8_url_without_forwarding() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=file;file:///tmp/report.html\x07local_file\x1b]8;;\x07 and https://example.com/visible",
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
        .expect_err("unsafe OSC8 host-open gesture should not forward mouse bytes");
    rx.try_recv()
        .expect_err("unsafe OSC8 host-open gesture should not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn open_link_under_cursor_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/jackin-preflight-url now\x1b[1;15H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx
        .try_recv()
        .expect_err("open-link command must not forward bytes to the pane");
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

#[test]
fn modified_url_hover_renders_visible_target_without_forwarding_to_pty() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 7,
            // SGR passive motion + Alt. Ghostty reported this shape during
            // Phase 0 preflight for Option-hover in a mouse-disabled pane.
            button: 43,
        },
    )
    .expect("hovering a link should repaint the notice");

    input_rx
        .try_recv()
        .expect_err("modified hover must not write bytes into a mouse-disabled pane");
    assert_eq!(
        mux.render.link_hover_url.as_deref(),
        Some("https://example.com/visible")
    );
    let frame = String::from_utf8_lossy(&frame);
    assert!(
        frame.contains("Open link: https://example.com/visible"),
        "hover notice missing from frame: {frame:?}"
    );
}

#[test]
fn modified_url_hover_prefers_osc8_target() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07https://example.com/visible\x1b]8;;\x07",
    );
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    drop(handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 1,
            button: 51,
        },
    ));

    assert_eq!(
        mux.render.link_hover_url.as_deref(),
        Some("https://example.com/osc8")
    );
}

#[test]
fn unmodified_url_hover_clears_existing_link_notice() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.session_supervisor.sessions.insert(1, session);
    mux.render.link_hover_url = Some("https://example.com/visible".to_owned());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    drop(handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 7,
            button: SGR_NO_BUTTON_MOTION,
        },
    ));

    assert_eq!(mux.render.link_hover_url, None);
}

#[tokio::test]
async fn open_link_under_cursor_palette_prefers_osc8_target_over_visible_text() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07https://example.com/visible\x1b]8;;\x07\x1b[1;2H",
    );
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx
        .try_recv()
        .expect_err("open-link must stay attach path");
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

#[tokio::test]
async fn open_link_under_cursor_palette_rejects_unsafe_visible_url() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hopen file:///tmp/report.html now\x1b[1;12H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx
        .try_recv()
        .expect_err("unsafe open-link command must not forward bytes to the pane");
    rx.try_recv()
        .expect_err("unsafe open-link command must not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn open_link_under_cursor_palette_action_reports_missing_url() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hplain text only\x1b[1;3H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    rx.try_recv()
        .expect_err("missing URL must not emit a host-open frame");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("No host-open link under focused cursor")
    );
}

#[tokio::test]
async fn export_file_under_cursor_palette_action_sends_file_export_frames() {
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
    session.feed_pty(b"\x1b[1;1Hsee report.txt now\x1b[1;7H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::ExportFileUnderCursorAndReveal);
    mux.client_registry.client.flush_out_of_band();

    input_rx
        .try_recv()
        .expect_err("export-under-cursor command must not forward bytes to the pane");
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
    assert_eq!(start.size, "hello export".len() as u64);
    assert!(start.reveal_after_export);
    assert!(!start.open_after_export);
    assert!(
        mux.clipboard
            .clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("File export and reveal queued: report.txt"))
    );
}
