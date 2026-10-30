// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::ClipboardImageError;

#[test]
fn clipboard_image_error_from_message_classifies_known_shapes() {
    let shaped: Vec<(ClipboardImageError, &str)> = vec![
        (ClipboardImageError::Empty, "empty"),
        (ClipboardImageError::TooLarge, "oversize"),
        (ClipboardImageError::UnsupportedFormat, "signature-mismatch"),
        (ClipboardImageError::DigestMismatch, "digest-mismatch"),
        (ClipboardImageError::ChunkSequence, "offset-mismatch"),
        (ClipboardImageError::MissingTransfer, "missing-transfer"),
        (ClipboardImageError::DuplicateTransfer, "duplicate-transfer"),
        (
            ClipboardImageError::BackendUnavailable,
            "backend-unavailable",
        ),
        (ClipboardImageError::Io, "io"),
        (ClipboardImageError::Other("boom".to_owned()), "other"),
    ];
    for (error, reason) in &shaped {
        assert_eq!(error.reason_code(), *reason);
        assert!(!error.message().is_empty());
    }
    assert_eq!(
        ClipboardImageError::from_message("clipboard image transfer is empty".to_owned()),
        ClipboardImageError::Empty
    );
    assert_eq!(
        ClipboardImageError::from_message("payload too large for transfer".to_owned()),
        ClipboardImageError::TooLarge
    );
    assert_eq!(
        ClipboardImageError::from_message("magic bytes not an image".to_owned()),
        ClipboardImageError::UnsupportedFormat
    );
    assert_eq!(
        ClipboardImageError::from_message("transfer SHA-256 digest differs".to_owned()),
        ClipboardImageError::DigestMismatch
    );
    assert_eq!(
        ClipboardImageError::from_message("chunk offset did not match expected".to_owned()),
        ClipboardImageError::ChunkSequence
    );
    assert_eq!(
        ClipboardImageError::from_message("transfer has no active start".to_owned()),
        ClipboardImageError::MissingTransfer
    );
    assert_eq!(
        ClipboardImageError::from_message("transfer already active".to_owned()),
        ClipboardImageError::DuplicateTransfer
    );
    assert_eq!(
        ClipboardImageError::from_message("no display; try xclip".to_owned()),
        ClipboardImageError::BackendUnavailable
    );
    assert_eq!(
        ClipboardImageError::from_message("failed reading clipboard bytes".to_owned()),
        ClipboardImageError::Io
    );
    for error in [
        ClipboardImageError::Empty,
        ClipboardImageError::TooLarge,
        ClipboardImageError::UnsupportedFormat,
        ClipboardImageError::DigestMismatch,
        ClipboardImageError::ChunkSequence,
        ClipboardImageError::MissingTransfer,
        ClipboardImageError::DuplicateTransfer,
        ClipboardImageError::BackendUnavailable,
        ClipboardImageError::Io,
    ] {
        assert_eq!(
            ClipboardImageError::from_message(error.message().to_owned()),
            error
        );
    }
    let unknown = "frobnicator disengaged at dawn".to_owned();
    assert_eq!(
        ClipboardImageError::from_message(unknown.clone()),
        ClipboardImageError::Other(unknown.clone())
    );
    assert_eq!(
        ClipboardImageError::Other(unknown.clone()).reason_code(),
        "other"
    );
    assert_eq!(
        ClipboardImageError::Other(unknown.clone()).message(),
        unknown.as_str()
    );
}
