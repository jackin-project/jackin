#[cfg(test)]
use super::*;

#[test]
fn stages_png_with_private_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"\x89PNG\r\n\x1a\npayload".to_vec(),
    };

    let path = stage_clipboard_image_at(temp.path(), &image).unwrap();
    assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
    assert_eq!(std::fs::read(&path).unwrap(), image.bytes);
    assert_eq!(
        std::fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn cleanup_removes_staged_files_but_leaves_non_files() {
    let temp = tempfile::tempdir().unwrap();
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"\x89PNG\r\n\x1a\npayload".to_vec(),
    };
    let path = stage_clipboard_image_at(temp.path(), &image).unwrap();
    let nested = temp.path().join("nested");
    std::fs::create_dir(&nested).unwrap();

    let removed = cleanup_clipboard_run_dir_at(temp.path()).unwrap();

    assert_eq!(removed, 1);
    assert!(!path.exists());
    assert!(nested.exists());
}

#[test]
fn rejects_mismatched_magic() {
    let temp = tempfile::tempdir().unwrap();
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"not a png".to_vec(),
    };

    let err = stage_clipboard_image_at(temp.path(), &image).unwrap_err();
    assert!(format!("{err:#}").contains("magic bytes"));
}

#[test]
fn accepts_browser_tiff_magic() {
    let temp = tempfile::tempdir().unwrap();
    let image = ClipboardImage {
        format: ClipboardImageFormat::Tiff,
        bytes: b"MM\0*payload".to_vec(),
    };

    let path = stage_clipboard_image_at(temp.path(), &image).unwrap();
    assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("tiff"));
}

#[test]
fn chunked_transfer_reassembles_and_validates_digest() {
    let bytes = b"\x89PNG\r\n\x1a\nchunked".to_vec();
    let digest: [u8; FILE_EXPORT_DIGEST_BYTES] = Sha256::digest(&bytes).into();
    let mut transfers = ClipboardImageTransfers::default();

    transfers
        .start(ClipboardImageStart {
            transfer_id: 7,
            format: ClipboardImageFormat::Png,
            size: bytes.len() as u64,
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 7,
            offset: 0,
            bytes: bytes[..8].to_vec(),
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 7,
            offset: 8,
            bytes: bytes[8..].to_vec(),
        })
        .unwrap();

    let image = transfers
        .end(ClipboardImageEnd {
            transfer_id: 7,
            sha256: digest,
        })
        .unwrap();
    assert_eq!(image.format, ClipboardImageFormat::Png);
    assert_eq!(image.bytes, bytes);
}

#[test]
fn chunked_transfer_rejects_oversize_start() {
    let mut transfers = ClipboardImageTransfers::default();

    let err = transfers
        .start(ClipboardImageStart {
            transfer_id: 10,
            format: ClipboardImageFormat::Png,
            size: MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64 + 1,
        })
        .unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("exceeds cap"), "{message}");
    assert!(
        message.contains(&MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES.to_string()),
        "{message}"
    );
}

#[test]
fn chunked_transfer_rejects_digest_mismatch() {
    let mut transfers = ClipboardImageTransfers::default();
    transfers
        .start(ClipboardImageStart {
            transfer_id: 8,
            format: ClipboardImageFormat::Png,
            size: 11,
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 8,
            offset: 0,
            bytes: b"\x89PNG\r\n\x1a\nbad".to_vec(),
        })
        .unwrap();

    let err = transfers
        .end(ClipboardImageEnd {
            transfer_id: 8,
            sha256: [0; FILE_EXPORT_DIGEST_BYTES],
        })
        .unwrap_err();
    assert!(format!("{err:#}").contains("SHA-256 mismatch"));
}

#[test]
fn rejected_chunk_cancels_transfer_so_retry_can_reuse_id() {
    let bytes = b"\x89PNG\r\n\x1a\nretry".to_vec();
    let digest: [u8; FILE_EXPORT_DIGEST_BYTES] = Sha256::digest(&bytes).into();
    let mut transfers = ClipboardImageTransfers::default();

    transfers
        .start(ClipboardImageStart {
            transfer_id: 9,
            format: ClipboardImageFormat::Png,
            size: bytes.len() as u64,
        })
        .unwrap();

    let err = transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 9,
            offset: 1,
            bytes: bytes.clone(),
        })
        .unwrap_err();
    assert!(format!("{err:#}").contains("offset"));

    transfers
        .start(ClipboardImageStart {
            transfer_id: 9,
            format: ClipboardImageFormat::Png,
            size: bytes.len() as u64,
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 9,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();
    let image = transfers
        .end(ClipboardImageEnd {
            transfer_id: 9,
            sha256: digest,
        })
        .unwrap();

    assert_eq!(image.bytes, bytes);
}

#[test]
fn chunked_transfer_idle_cleanup_removes_stale_buffer() {
    let mut transfers = ClipboardImageTransfers::default();
    transfers
        .start(ClipboardImageStart {
            transfer_id: 11,
            format: ClipboardImageFormat::Png,
            size: 11,
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 11,
            offset: 0,
            bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    transfers.active.get_mut(&11).unwrap().last_activity =
        Instant::now().checked_sub(Duration::from_secs(10)).unwrap();

    assert_eq!(transfers.abort_idle_before(Instant::now()), 1);
    let err = transfers
        .end(ClipboardImageEnd {
            transfer_id: 11,
            sha256: [0; FILE_EXPORT_DIGEST_BYTES],
        })
        .unwrap_err();
    assert!(format!("{err:#}").contains("has no active start"));
}

#[test]
fn chunked_transfer_idle_cleanup_keeps_fresh_buffer() {
    let bytes = b"\x89PNG\r\n\x1a\nfresh".to_vec();
    let digest: [u8; FILE_EXPORT_DIGEST_BYTES] = Sha256::digest(&bytes).into();
    let mut transfers = ClipboardImageTransfers::default();
    transfers
        .start(ClipboardImageStart {
            transfer_id: 12,
            format: ClipboardImageFormat::Png,
            size: bytes.len() as u64,
        })
        .unwrap();
    transfers
        .chunk(ClipboardImageChunk {
            transfer_id: 12,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();

    assert_eq!(
        transfers
            .abort_idle_before(Instant::now().checked_sub(Duration::from_secs(10)).unwrap()),
        0
    );
    let image = transfers
        .end(ClipboardImageEnd {
            transfer_id: 12,
            sha256: digest,
        })
        .unwrap();
    assert_eq!(image.bytes, bytes);
}
}
