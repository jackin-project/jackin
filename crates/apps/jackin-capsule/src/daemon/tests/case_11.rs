// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::layout::Tab;

const ALT_MODIFIED_PRESS: u8 = 8;
const HOVER_BUTTON: u8 = 3 | 8 | 32;

#[tokio::test]
async fn modified_click_in_mouse_enabled_pane_forwards_to_pty() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let id = super::spawn_sleep_session(&mut mux);
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("t", id, "codename-m"));
    {
        let session = mux
            .session_supervisor
            .sessions
            .get_mut(id)
            .expect("spawned session");
        session.shadow_grid.process(b"\x1b[?1003h\x1b[?1006h");
        assert_eq!(
            session.mouse_protocol_mode(),
            termpane::MouseProtocolMode::AnyMotion
        );
        assert_eq!(
            session.mouse_protocol_encoding(),
            termpane::MouseProtocolEncoding::Sgr
        );
    }
    let inner = mux.active_focused_inner_rect().expect("focused pane");
    assert!(mux.forward_mouse_to_focused_pane_with_kind(
        inner.col + 2,
        inner.row + 2,
        ALT_MODIFIED_PRESS,
        true
    ));
    super::terminate_session(&mut mux, id);
}

#[tokio::test]
async fn modified_url_hover_renders_visible_target_without_forwarding_to_pty() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let id = super::spawn_sleep_session(&mut mux);
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("t", id, "codename-u"));
    {
        let session = mux
            .session_supervisor
            .sessions
            .get_mut(id)
            .expect("spawned session");
        session.shadow_grid.process(b"https://example.com/docs");
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.update_hover_for_mouse(3, 5, HOVER_BUTTON);
    assert_eq!(
        mux.render.link_hover_url.as_deref(),
        Some("https://example.com/docs")
    );
    let _ = rx.try_recv().unwrap_err();
    super::terminate_session(&mut mux, id);
}
