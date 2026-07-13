// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `util`.
use super::*;
use std::io::Write;

#[test]
fn returns_none_when_path_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    assert_eq!(read_text_bounded("test", &missing, 1024), None);
}

#[test]
fn returns_full_contents_below_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("small.txt");
    std::fs::write(&p, b"hello").unwrap();
    assert_eq!(
        read_text_bounded("test", &p, 1024).as_deref(),
        Some("hello")
    );
}

#[test]
fn truncates_at_cap_when_file_larger() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("big.txt");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(&vec![b'a'; 4096]).unwrap();
    let result = read_text_bounded("test", &p, 64).expect("read succeeds");
    assert_eq!(result.len(), 64, "must respect the cap and truncate");
    assert!(result.chars().all(|c| c == 'a'));
}

#[test]
fn returns_none_on_invalid_utf8() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("binary.bin");
    std::fs::write(&p, [0xff, 0xfe, 0xfd]).unwrap();
    assert_eq!(read_text_bounded("test", &p, 1024), None);
}
