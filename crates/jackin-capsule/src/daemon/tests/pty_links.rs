// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Targets derive from fixture bytes and explicit displayed positions, rather
//! than production selection or hyperlink coordinate helpers.

use super::*;

fn history_link_session() -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (mut session, input_rx) = test_shell_session(20, 78);
    for index in 0..45 {
        session.feed_pty(
            format!("\x1b]8;;https://example.com/history/{index}\x07H{index:02}\x1b]8;;\x07\r\n")
                .as_bytes(),
        );
    }
    assert_eq!(session.shadow_grid.scrollback_len(), 26);
    session.feed_pty(b"\x1b[H\x1b[2K\x1b]8;;https://example.com/live\x07live_label\x1b]8;;\x07");
    assert_eq!(session.shadow_grid.scrollback_len(), 26);
    (session, input_rx)
}

async fn assert_host_open_frame(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>, target: &str) {
    let bytes = rx
        .try_recv()
        .expect("host-open frame from displayed OSC 8 label");
    let mut payload = &bytes[1..];
    assert_eq!(
        read_server_frame(&mut payload, bytes[0])
            .await
            .unwrap()
            .unwrap(),
        ServerFrame::HostOpenUrl(target.to_owned()),
    );
    assert!(
        rx.try_recv().is_err(),
        "one gesture must emit one host-open frame"
    );
}

#[tokio::test]
async fn modified_click_osc8_live_label_with_history() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = history_link_session();
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let mut rx = attach_drained_client(&mut mux);
    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 2,
            button: 8,
        },
    );
    assert_host_open_frame(&mut rx, "https://example.com/live").await;
    assert!(input_rx.try_recv().is_err());
}

#[tokio::test]
async fn modified_click_osc8_small_history_never_opens_neighbor_row() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for _ in 0..20 {
        session.feed_pty(b"history\r\n");
    }
    assert_eq!(session.shadow_grid.scrollback_len(), 1);
    session.feed_pty(b"\x1b[H\x1b[2K\x1b]8;;https://example.com/clicked\x07clicked\x1b]8;;\x07\x1b[2;1H\x1b]8;;https://example.com/neighbor\x07neighbor\x1b]8;;\x07");
    assert_eq!(session.shadow_grid.scrollback_len(), 1);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let mut rx = attach_drained_client(&mut mux);
    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 2,
            button: 8,
        },
    );
    assert_host_open_frame(&mut rx, "https://example.com/clicked").await;
    assert!(input_rx.try_recv().is_err());
}

#[tokio::test]
async fn modified_click_osc8_scrolled_history_and_live_prefix() {
    for (viewport_row, expected) in [
        (0, "https://example.com/history/23"),
        (3, "https://example.com/live"),
    ] {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = history_link_session();
        assert!(session.set_scrollback_offset(3));
        mux.session_supervisor.sessions.insert(1, session);
        drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
        let mut rx = attach_drained_client(&mut mux);
        let inner = mux.visible_panes()[0].inner;
        apply_action_frame(
            &mut mux,
            Action::OpenVisibleUrlAt {
                row: inner.row + viewport_row,
                col: inner.col + 1,
                button: 8,
            },
        );
        assert_host_open_frame(&mut rx, expected).await;
        assert!(input_rx.try_recv().is_err());
    }
}

#[tokio::test]
async fn cursor_osc8_live_label_with_history() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = history_link_session();
    session.feed_pty(b"\x1b[1;3H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let mut rx = attach_drained_client(&mut mux);
    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);
    assert_host_open_frame(&mut rx, "https://example.com/live").await;
    assert!(input_rx.try_recv().is_err());
}

#[tokio::test]
async fn modified_click_osc8_alternate_screen_with_primary_history() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = history_link_session();
    session.feed_pty(
        b"\x1b[?1049h\x1b[H\x1b]8;;https://example.com/alternate\x07alt_label\x1b]8;;\x07",
    );
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let mut rx = attach_drained_client(&mut mux);
    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 2,
            button: 8,
        },
    );
    assert_host_open_frame(&mut rx, "https://example.com/alternate").await;
    assert!(input_rx.try_recv().is_err());
}

#[tokio::test]
async fn modified_click_unsafe_osc8_with_history_rejects() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = history_link_session();
    session.feed_pty(b"\x1b[H\x1b]8;;file:///tmp/private\x07unsafe_label\x1b]8;;\x07");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let mut rx = attach_drained_client(&mut mux);
    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 2,
            button: 8,
        },
    );
    assert!(rx.try_recv().is_err());
    assert!(input_rx.try_recv().is_err());
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[test]
fn modified_hover_osc8_live_label_with_history() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = history_link_session();
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 2,
            button: 43,
        },
    )
    .expect("hover must repaint the OSC 8 target");
    assert_eq!(
        mux.render.link_hover_url.as_deref(),
        Some("https://example.com/live")
    );
    assert!(input_rx.try_recv().is_err());
}
