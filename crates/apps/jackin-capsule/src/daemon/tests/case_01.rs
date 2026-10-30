// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::attach::AttachCapabilities;

fn contains(reply: &[u8], pattern: &[u8]) -> bool {
    !pattern.is_empty() && reply.windows(pattern.len()).any(|window| window == pattern)
}

fn osc_replies(mux: &mut super::super::Multiplexer, id: u64) -> Vec<Vec<u8>> {
    let session = mux
        .session_supervisor
        .sessions
        .get_mut(id)
        .expect("spawned session");
    session.shadow_grid.process(b"\x1b]10;?\x07");
    session.shadow_grid.process(b"\x1b]11;?\x07");
    session
        .shadow_grid
        .drain_passthrough()
        .into_iter()
        .filter_map(|event| match event {
            termpane::PassthroughEvent::Reply(bytes) => Some(bytes),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn reattach_updates_capabilities_without_resetting_model_palette() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let id = super::spawn_sleep_session(&mut mux);
    mux.client_registry.attached_terminal.default_fg = Some((10, 20, 30));
    mux.client_registry.attached_terminal.default_bg = Some((40, 50, 60));
    mux.client_registry.attached_capabilities = AttachCapabilities {
        pointer_shapes: true,
        ..AttachCapabilities::default()
    };
    mux.apply_client_colors_to_sessions();
    let first = osc_replies(&mut mux, id);
    assert!(
        first
            .iter()
            .any(|reply| contains(reply, b"rgb:0a0a/1414/1e1e")),
        "first client foreground not reported"
    );
    assert!(
        first
            .iter()
            .any(|reply| contains(reply, b"rgb:2828/3232/3c3c")),
        "first client background not reported"
    );
    mux.client_registry.attached_terminal.default_fg = Some((1, 2, 3));
    mux.client_registry.attached_terminal.default_bg = None;
    mux.client_registry.attached_capabilities = AttachCapabilities::default();
    mux.apply_client_colors_to_sessions();
    assert!(!mux.client_registry.attached_capabilities.pointer_shapes);
    let second = osc_replies(&mut mux, id);
    assert!(
        second
            .iter()
            .any(|reply| contains(reply, b"rgb:0101/0202/0303")),
        "reattached foreground not applied"
    );
    assert!(
        second
            .iter()
            .any(|reply| contains(reply, b"rgb:2828/3232/3c3c")),
        "unreported background was reset instead of kept"
    );
    super::terminate_session(&mut mux, id);
}

#[test]
fn session_terminal_carries_the_attached_client_palette() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    mux.client_registry.attached_terminal.default_fg = Some((7, 8, 9));
    mux.client_registry.attached_terminal.default_bg = Some((1, 2, 3));
    let terminal = mux.session_terminal(20, 60);
    assert_eq!(terminal.default_fg, Some((7, 8, 9)));
    assert_eq!(terminal.default_bg, Some((1, 2, 3)));
    assert_eq!((terminal.rows, terminal.cols), (20, 60));
}
