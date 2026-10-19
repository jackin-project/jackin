// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

fn attached_mux(workdir: &std::path::Path) -> (Multiplexer, mpsc::UnboundedReceiver<Vec<u8>>) {
    let mut mux = crate::daemon::tests::single_pane_tab_mux();
    mux.session_supervisor.tabs.clear();
    mux.launch_env.workdir = workdir.to_owned();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.client_registry.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}
    (mux, rx)
}

async fn frames(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Result<Vec<ServerFrame>> {
    let mut frames = Vec::new();
    while let Ok(bytes) = rx.try_recv() {
        let (tag, mut payload) = bytes.split_first().context("empty host frame")?;
        frames.push(
            crate::protocol::attach::read_server_frame(&mut payload, *tag)
                .await?
                .context("missing host frame")?,
        );
    }
    Ok(frames)
}

#[tokio::test]
async fn complete_prepared_export_has_exact_chunks_offsets_digest_and_end() {
    let temp = tempfile::tempdir().unwrap();
    let payload = vec![0x5a; MAX_FILE_EXPORT_CHUNK_BYTES + 17];
    std::fs::write(temp.path().join("report.bin"), &payload).unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    mux.export_file_to_host("report.bin".to_owned(), true, false, None);
    assert!(
        rx.try_recv().is_err(),
        "worker cannot emit frames before MUX completion"
    );
    mux.finish_pending_file_export_for_test().await.unwrap();
    let mut output = frames(&mut rx).await.unwrap().into_iter();
    let Some(ServerFrame::FileExportStart(start)) = output.next() else {
        panic!("missing start");
    };
    assert_eq!(start.file_name, "report.bin");
    assert_eq!(start.size, u64::try_from(payload.len()).unwrap());
    assert!(start.reveal_after_export);
    assert!(!start.open_after_export);
    let mut reconstructed = Vec::new();
    for expected_size in [MAX_FILE_EXPORT_CHUNK_BYTES, 17] {
        let Some(ServerFrame::FileExportChunk(chunk)) = output.next() else {
            panic!("missing chunk");
        };
        assert_eq!(chunk.transfer_id, start.transfer_id);
        assert_eq!(chunk.offset, u64::try_from(reconstructed.len()).unwrap());
        assert_eq!(chunk.bytes.len(), expected_size);
        reconstructed.extend_from_slice(&chunk.bytes);
    }
    let Some(ServerFrame::FileExportEnd(end)) = output.next() else {
        panic!("missing end");
    };
    assert_eq!(end.transfer_id, start.transfer_id);
    assert_eq!(end.sha256, <[u8; 32]>::from(Sha256::digest(&payload)));
    assert_eq!(reconstructed, payload);
    assert!(output.next().is_none());
}

#[tokio::test]
async fn busy_export_retains_one_worker_and_emits_only_its_payload() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("first"), b"first").unwrap();
    std::fs::write(temp.path().join("second"), b"second").unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    mux.export_file_to_host("first".to_owned(), false, false, None);
    let task_id = mux.file_exports.pending.as_ref().unwrap().task.id();
    mux.export_file_to_host("second".to_owned(), false, false, None);
    assert_eq!(
        mux.file_exports.pending.as_ref().unwrap().task.id(),
        task_id
    );
    mux.finish_pending_file_export_for_test().await.unwrap();
    let output = frames(&mut rx).await.unwrap();
    assert_eq!(output.len(), 3);
    assert!(
        matches!(&output[0], ServerFrame::FileExportStart(start) if start.file_name == "first")
    );
    assert!(matches!(&output[1], ServerFrame::FileExportChunk(chunk) if chunk.bytes == b"first"));
    assert!(matches!(&output[2], ServerFrame::FileExportEnd(_)));
}

#[tokio::test]
async fn detach_and_reattach_same_channel_discards_old_export_without_notice() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("report"), b"old attachment").unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    mux.export_file_to_host("report".to_owned(), false, false, None);
    let old = mux.client_registry.client.attachment_identity().unwrap();
    let tx = mux.client_registry.client.take().unwrap();
    mux.client_registry.client.attach(tx);
    assert!(!mux.client_registry.client.matches_attachment(&old));
    mux.finish_pending_file_export_for_test().await.unwrap();
    rx.try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
    assert!(mux.clipboard.clipboard_image_notice.is_none());
}

#[tokio::test]
async fn replacement_client_never_receives_old_export_or_notice() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("report"), b"old attachment").unwrap();
    let (mut mux, mut old_rx) = attached_mux(temp.path());
    mux.export_file_to_host("report".to_owned(), false, false, None);
    let (tx, mut new_rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.finish_pending_file_export_for_test().await.unwrap();
    old_rx
        .try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
    new_rx
        .try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
    assert!(mux.clipboard.clipboard_image_notice.is_none());
}

#[tokio::test]
async fn mux_drop_signals_owned_export_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("report"), b"cancelled").unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    mux.export_file_to_host("report".to_owned(), false, false, None);
    let cancelled = Arc::clone(&mux.file_exports.pending.as_ref().unwrap().cancelled);
    drop(mux);
    assert!(cancelled.load(Ordering::Acquire));
    rx.try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
}

#[tokio::test]
async fn rejected_source_emits_no_partial_protocol_transfer() {
    let temp = tempfile::tempdir().unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    mux.export_file_to_host("missing".to_owned(), false, false, None);
    mux.finish_pending_file_export_for_test().await.unwrap();
    rx.try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
    assert!(
        mux.clipboard
            .clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.starts_with("File export rejected:"))
    );
}

#[tokio::test]
async fn failed_read_completion_emits_no_start_chunk_or_end() {
    let temp = tempfile::tempdir().unwrap();
    let (mut mux, mut rx) = attached_mux(temp.path());
    let attachment = mux.client_registry.client.attachment_identity().unwrap();
    let task = tokio::spawn(async {
        Err(PrepareError::ReadFailed(anyhow::anyhow!(
            "injected read failure"
        )))
    });
    mux.file_exports.pending = Some(PendingExport {
        task,
        attachment,
        session_id: None,
        intent: ExportIntent::Explicit,
        reveal_after_export: false,
        open_after_export: false,
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    mux.finish_pending_file_export_for_test().await.unwrap();
    rx.try_recv()
        .expect_err("rejected, cancelled, or stale export must not emit host frames");
    assert!(
        mux.clipboard
            .clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("injected read failure"))
    );
}
