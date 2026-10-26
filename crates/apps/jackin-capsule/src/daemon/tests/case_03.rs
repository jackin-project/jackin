// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::attach::{ServerFrame, decode_server};

#[tokio::test]
async fn drain_and_exit_delivers_shutdown_before_closing_attach_socket() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    crate::attach_protocol::drain_and_exit(&mut mux).await;
    let first = rx.try_recv().expect("shutdown frame emitted");
    assert!(first.len() >= 5);
    let tag = first[0];
    let len = u32::from_be_bytes(first[1..5].try_into().unwrap()) as usize;
    assert_eq!(first.len(), 5 + len);
    match decode_server(tag, first[5..].to_vec()).unwrap() {
        ServerFrame::Shutdown { .. } => {}
        frame => panic!("expected shutdown first, got {frame:?}"),
    }
    assert!(!mux.client_registry.client.is_attached());
    let _ = rx.try_recv().unwrap_err();
}
