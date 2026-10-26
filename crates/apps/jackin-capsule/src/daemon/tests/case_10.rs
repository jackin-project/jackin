// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::attach::{ServerFrame, decode_server};
use sha2::Digest;

fn split_frame(bytes: &[u8]) -> (u8, Vec<u8>) {
    assert!(bytes.len() > 5);
    let len = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 5 + len);
    (bytes[0], bytes[5..].to_vec())
}

async fn poll_until_frames(
    mux: &mut super::super::Multiplexer,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    count: usize,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        mux.poll_file_export().await;
        if rx.len() >= count || std::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

async fn poll_settled(mux: &mut super::super::Multiplexer) {
    for _ in 0..40 {
        mux.poll_file_export().await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn modified_click_visible_file_path_sends_file_export_frames() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"export me").unwrap();
    let mut mux = super::test_mux(dir.path());
    let id = super::spawn_sleep_session(&mut mux);
    mux.session_supervisor
        .tabs
        .push(crate::tui::layout::Tab::new_single("t", id, "codename-e"));
    {
        let session = mux
            .session_supervisor
            .sessions
            .get_mut(id)
            .expect("spawned session");
        session.shadow_grid.process(b"notes.txt");
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    assert!(mux.export_visible_file_at(3, 5));
    poll_until_frames(&mut mux, &mut rx, 3).await;
    let mut frames = Vec::new();
    while let Ok(bytes) = rx.try_recv() {
        frames.push(bytes);
    }
    assert_eq!(frames.len(), 3);
    let mut transfer = None;
    let mut bytes = Vec::new();
    for frame in &frames {
        let (tag, payload) = split_frame(frame);
        match decode_server(tag, payload).unwrap() {
            ServerFrame::FileExportStart(start) => {
                assert_eq!(start.file_name, "notes.txt");
                assert_eq!(start.size, 9);
                transfer = Some(start.transfer_id);
            }
            ServerFrame::FileExportChunk(chunk) => {
                assert_eq!(chunk.transfer_id, transfer.unwrap());
                assert_eq!(chunk.offset, 0);
                bytes.extend_from_slice(&chunk.bytes);
            }
            ServerFrame::FileExportEnd(end) => {
                assert_eq!(end.transfer_id, transfer.unwrap());
                let digest: [u8; 32] = sha2::Sha256::digest(b"export me").into();
                assert_eq!(end.sha256, digest);
            }
            frame => panic!("unexpected frame {frame:?}"),
        }
    }
    assert_eq!(bytes, b"export me");
    mux.export_file_to_host("../escape.txt".to_owned(), false, false, None);
    poll_settled(&mut mux).await;
    assert!(
        mux.clipboard
            .clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("rejected")),
        "escape rejected"
    );
    let _ = rx.try_recv().unwrap_err();
    super::terminate_session(&mut mux, id);
}
