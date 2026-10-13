// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn normalize_size_substitutes_zero_and_clamps_minimums() {
    assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
    assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
    assert_eq!(normalize_size(40, 120), (40, 120));
}

#[tokio::test]
async fn clipboard_image_writer_keeps_small_images_single_frame() {
    let (mut client, mut server) = duplex(4096);
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"\x89PNG\r\n\x1a\nsmall".to_vec(),
    };
    let mut operations = HashMap::new();

    write_clipboard_image_frames(&mut client, &mut operations, image.clone())
        .await
        .unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(request) = frame else {
        panic!("expected contextual clipboard image");
    };
    assert_eq!(
        request.operation,
        AttachControlOperation::ClipboardImage(image)
    );
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn clipboard_image_writer_chunks_large_images_with_digest() {
    let mut bytes = vec![b'x'; MAX_CONTEXTUAL_CLIPBOARD_IMAGE_BYTES + 1];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    let capacity = bytes.len() + 4096;
    let (mut client, mut server) = duplex(capacity);
    let expected_digest: [u8; 32] = Sha256::digest(&bytes).into();
    let mut operations = HashMap::new();

    write_clipboard_image_frames(
        &mut client,
        &mut operations,
        ClipboardImage {
            format: ClipboardImageFormat::Png,
            bytes: bytes.clone(),
        },
    )
    .await
    .unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let start = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(request) = start else {
        panic!("expected contextual chunked image start");
    };
    let AttachControlOperation::ClipboardImageStart(start) = request.operation else {
        panic!("expected chunked image start");
    };
    assert_eq!(start.format, ClipboardImageFormat::Png);
    assert_eq!(start.size, bytes.len() as u64);

    let mut received = Vec::new();
    loop {
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        match frame {
            ClientFrame::AttachControl(AttachControlRequest {
                operation: AttachControlOperation::ClipboardImageChunk(chunk),
                ..
            }) => {
                assert_eq!(chunk.transfer_id, start.transfer_id);
                assert_eq!(chunk.offset, received.len() as u64);
                assert!(chunk.bytes.len() <= MAX_CLIPBOARD_IMAGE_CHUNK_BYTES);
                received.extend(chunk.bytes);
            }
            ClientFrame::AttachControl(AttachControlRequest {
                operation: AttachControlOperation::ClipboardImageEnd(end),
                ..
            }) => {
                assert_eq!(end.transfer_id, start.transfer_id);
                assert_eq!(end.sha256, expected_digest);
                break;
            }
            other => panic!("unexpected frame {other:?}"),
        }
    }

    assert_eq!(received, bytes);
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn explicit_clipboard_image_request_returns_probe_error_to_capsule() {
    let (mut client, mut server) = duplex(4096);
    let mut operations = HashMap::new();

    write_clipboard_image_request_result(
        &mut client,
        &mut operations,
        Err(anyhow::anyhow!(
            "Linux host clipboard image reader needs WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"
        )),
        "host clipboard does not contain a readable image",
        "host clipboard image probe failed",
    )
    .await;
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(AttachControlRequest {
        operation: AttachControlOperation::ClipboardImageError(error),
        ..
    }) = frame
    else {
        panic!("expected ClipboardImageError");
    };

    assert_eq!(error.reason_code(), "backend-unavailable");
    assert!(error.message().contains("xclip/wl-paste"));
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn explicit_clipboard_path_request_mentions_file_url_support() {
    let (mut client, mut server) = duplex(4096);
    let mut operations = HashMap::new();

    write_clipboard_image_request_result(
        &mut client,
        &mut operations,
        Ok(None),
        "host clipboard text is not an absolute readable image path or file:// image URL",
        "host clipboard image path probe failed",
    )
    .await;
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(AttachControlRequest {
        operation: AttachControlOperation::ClipboardImageError(error),
        ..
    }) = frame
    else {
        panic!("expected ClipboardImageError");
    };

    assert_eq!(error.reason_code(), "io");
    assert!(error.message().contains("host I/O failed"));
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[test]
fn host_file_export_finalizes_after_digest_match() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"export me";
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 99,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: bytes.len() as u64,
                reveal_after_export: true,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 99,
            offset: 0,
            bytes: bytes.to_vec(),
        })
        .unwrap();
    let completed = exports
        .end(FileExportEnd {
            transfer_id: 99,
            sha256,
        })
        .unwrap();

    assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), bytes);
    assert_eq!(completed.final_path, root.path().join("report.txt"));
    assert_eq!(completed.bytes, bytes.len() as u64);
    assert!(completed.reveal_after_export);
}

#[test]
fn host_file_export_rejects_digest_mismatch_and_removes_temp() {
    let root = tempfile::tempdir().unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 100,
                source_path: "/workspace/report.txt".into(),
                file_name: "../bad:name.txt".into(),
                size: 3,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 100,
            offset: 0,
            bytes: b"bad".to_vec(),
        })
        .unwrap();
    let err = exports
        .end(FileExportEnd {
            transfer_id: 100,
            sha256: [0; 32],
        })
        .expect_err("digest mismatch should reject export");

    assert!(format!("{err:#}").contains("SHA-256 mismatch"));
    assert!(!root.path().join("__bad_name.txt").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn host_file_export_drop_removes_interrupted_temp_file() {
    let root = tempfile::tempdir().unwrap();
    {
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 102,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: 9,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 102,
                offset: 0,
                bytes: b"partial".to_vec(),
            })
            .unwrap();

        assert!(root.path().join("report.txt.part").exists());
        assert!(!root.path().join("report.txt").exists());
    }

    assert!(!root.path().join("report.txt.part").exists());
    assert!(!root.path().join("report.txt").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn host_file_export_abort_removes_temp_and_rejects_end() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"export me";
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 103,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: bytes.len() as u64,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 103,
            offset: 0,
            bytes: b"export".to_vec(),
        })
        .unwrap();

    let err = exports
        .chunk(FileExportChunk {
            transfer_id: 103,
            offset: 0,
            bytes: b"bad-offset".to_vec(),
        })
        .expect_err("bad offset should reject export chunk");
    assert!(format!("{err:#}").contains("did not match expected"));

    exports.abort(103);
    assert!(!root.path().join("report.txt.part").exists());
    assert!(!root.path().join("report.txt").exists());
    let err = exports
        .end(FileExportEnd {
            transfer_id: 103,
            sha256,
        })
        .expect_err("aborted transfer should not finalize");
    assert!(format!("{err:#}").contains("has no active start"));
}

#[test]
fn host_file_export_idle_cleanup_removes_stale_temp_file() {
    let root = tempfile::tempdir().unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 104,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: 9,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 104,
            offset: 0,
            bytes: b"partial".to_vec(),
        })
        .unwrap();
    exports.active.get_mut(&104).unwrap().last_activity =
        Instant::now().checked_sub(Duration::from_secs(10)).unwrap();

    assert!(root.path().join("report.txt.part").exists());
    assert_eq!(exports.abort_idle_before(Instant::now()), 1);
    assert!(!root.path().join("report.txt.part").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());

    let err = exports
        .end(FileExportEnd {
            transfer_id: 104,
            sha256: [0; 32],
        })
        .expect_err("stale transfer cleanup should remove active export");
    assert!(format!("{err:#}").contains("has no active start"));
}
