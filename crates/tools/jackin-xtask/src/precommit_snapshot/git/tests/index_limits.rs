// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    capture::index_inputs_with_limit,
    snapshot::{admit_staged_index_entries, copy_and_verify_index_with_limit, copy_index_bytes},
};
use super::*;
use crate::precommit_snapshot::process::{SetupControls, is_clean_cancellation};
use std::{
    io::{Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

struct CancelAfterFirstRead {
    bytes: Cursor<Vec<u8>>,
    cancellation: Arc<AtomicBool>,
    reads: Arc<AtomicUsize>,
}

impl Read for CancelAfterFirstRead {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.bytes.read(buffer)?;
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            self.cancellation.store(true, Ordering::SeqCst);
        }
        Ok(count)
    }
}

fn cancelling_reader(bytes: Vec<u8>) -> (CancelAfterFirstRead, SetupControls, Arc<AtomicUsize>) {
    let cancellation = Arc::new(AtomicBool::new(false));
    let controls = SetupControls::new(Arc::clone(&cancellation), Duration::from_secs(30));
    let reads = Arc::new(AtomicUsize::new(0));
    (
        CancelAfterFirstRead {
            bytes: Cursor::new(bytes),
            cancellation,
            reads: Arc::clone(&reads),
        },
        controls,
        reads,
    )
}

#[test]
fn index_admission_applies_the_per_file_limit_to_active_and_shared_inputs() {
    let fixture = fixture();
    let root = fixture.path();
    let active = LiveRepository::capture_at(root).expect("capture active index");
    let error = index_inputs_with_limit(&active.index, None, &active.controls, 1)
        .expect_err("oversized active index must fail before hashing");
    assert!(format!("{error:#}").contains("live index input exceeds its byte limit"));

    git(root, &["update-index", "--split-index"]);
    let split = LiveRepository::capture_at(root).expect("capture split index");
    let shared = split.shared_index.as_ref().expect("shared index path");
    let error = index_inputs_with_limit(shared, None, &split.controls, 1)
        .expect_err("oversized shared index must fail before hashing");
    assert!(format!("{error:#}").contains("live index input exceeds its byte limit"));
}

#[test]
fn private_index_copy_rejects_limit_before_creating_destination() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let input = live
        .index_inputs
        .iter()
        .find(|input| input.admitted_path() == live.index)
        .expect("admitted active index");
    let destination = fixture.path().join("limited-index-copy");

    let error = copy_and_verify_index_with_limit(
        input,
        &destination,
        "active Git index",
        &live.controls,
        input.admitted_length() - 1,
    )
    .expect_err("copy limit below admitted length must fail");
    assert!(format!("{error:#}").contains("active Git index exceeds its byte limit"));
    assert!(!destination.exists());
}

#[test]
fn index_copy_does_not_write_bytes_beyond_the_admitted_limit() {
    let mut source = Cursor::new(b"grown-index".to_vec());
    let controls = SetupControls::for_test(Duration::from_secs(30));
    let mut destination = Vec::new();

    let error = copy_index_bytes(
        &mut source,
        &mut destination,
        10,
        10,
        &controls,
        "active Git index",
    )
    .expect_err("growth beyond admitted bytes must fail");
    assert!(format!("{error:#}").contains("active Git index exceeds its byte limit"));
    assert!(destination.is_empty());
}

#[test]
fn index_copy_observes_cancellation_before_writing_a_read_chunk() {
    let (mut source, controls, _) = cancelling_reader(vec![b'x'; 64 * 1024]);
    let mut destination = Vec::new();
    let error = copy_index_bytes(
        &mut source,
        &mut destination,
        64 * 1024,
        64 * 1024,
        &controls,
        "active Git index",
    )
    .expect_err("cancellation after read must stop before write");
    assert!(is_clean_cancellation(&error));
    assert!(destination.is_empty());
}

#[test]
fn staged_index_parser_checks_cancellation_between_inventory_chunks() {
    let root = tempfile::tempdir().expect("snapshot root");
    let record = b"100644 0000000000000000000000000000000000000000\tfile\0";
    let bytes = record.repeat(2_000);
    let (source, controls, reads) = cancelling_reader(bytes);
    let error = admit_staged_index_entries(root.path(), source, &controls)
        .expect_err("cancellation must stop staged inventory parsing");
    assert!(is_clean_cancellation(&error));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn staged_index_parser_checks_cancellation_before_symlink_resolution() {
    let root = tempfile::tempdir().expect("snapshot root");
    symlink("../escape", root.path().join("link")).expect("create staged symlink");
    let bytes = b"120000 0000000000000000000000000000000000000000\tlink\0".to_vec();
    let (source, controls, _) = cancelling_reader(bytes);
    let error = admit_staged_index_entries(root.path(), source, &controls)
        .expect_err("cancellation must be checked before resolving staged symlink");
    assert!(is_clean_cancellation(&error));
}
