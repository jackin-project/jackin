// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs::{self, FileTimes};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::time::{Duration, UNIX_EPOCH};

fn deadline() -> Instant {
    Instant::now()
        .checked_add(Duration::from_secs(5))
        .expect("test deadline")
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("owned export fixture");
    fs::write(root.path().join("file"), b"abc").expect("fixture bytes");
    root
}
fn hooked(
    requested: &str,
    root: &Path,
    cancelled: &AtomicBool,
    hook: &mut dyn FnMut(Stage) -> Result<()>,
) -> Result<PreparedExport, PrepareError> {
    prepare(
        requested,
        root,
        &Budget {
            cancelled,
            deadline: deadline(),
        },
        hook,
    )
}
fn read_failed(result: Result<PreparedExport, PrepareError>, message: &str) {
    let error = result.expect_err("no prepared payload after source failure");
    assert!(matches!(error, PrepareError::ReadFailed(_)), "{error:?}");
    assert!(error.to_string().contains(message), "{error}");
}

#[test]
fn exact_bytes_name_canonical_path_and_known_sha256() {
    let root = fixture();
    let prepared =
        prepare_export("file", root.path(), &AtomicBool::new(false), deadline()).unwrap();
    assert_eq!(
        prepared.source,
        root.path().join("file").canonicalize().unwrap()
    );
    assert_eq!(prepared.file_name, "file");
    assert_eq!(prepared.bytes, b"abc");
    assert_eq!(
        prepared.sha256,
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]
    );
}

#[test]
fn empty_and_exact_cap_succeed_but_cap_plus_one_is_rejected() {
    let root = fixture();
    let path = root.path().join("file");
    File::create(&path).unwrap().set_len(0).unwrap();
    let prepared =
        prepare_export("file", root.path(), &AtomicBool::new(false), deadline()).unwrap();
    assert!(prepared.bytes.is_empty());
    assert_eq!(prepared.sha256, <[u8; 32]>::from(Sha256::digest([])));
    File::create(&path)
        .unwrap()
        .set_len(MAX_EXPORT_FILE_BYTES)
        .unwrap();
    // This checks the byte boundary, not throughput under concurrent CI work.
    let byte_boundary_deadline = Instant::now()
        .checked_add(Duration::from_secs(30))
        .expect("byte-boundary test deadline");
    let prepared = prepare_export(
        "file",
        root.path(),
        &AtomicBool::new(false),
        byte_boundary_deadline,
    )
    .unwrap();
    assert_eq!(
        u64::try_from(prepared.bytes.len()).unwrap(),
        MAX_EXPORT_FILE_BYTES
    );
    assert!(prepared.bytes.iter().all(|byte| *byte == 0));
    drop(prepared);
    File::create(&path)
        .unwrap()
        .set_len(MAX_EXPORT_FILE_BYTES.checked_add(1).unwrap())
        .unwrap();
    assert!(
        matches!(prepare_export("file", root.path(), &AtomicBool::new(false), deadline()),
        Err(PrepareError::CandidateRejected(error)) if error.to_string().contains("export cap"))
    );
}

#[test]
fn initial_in_root_alias_is_allowed_and_outside_alias_is_rejected() {
    let root = fixture();
    let outside = fixture();
    symlink("file", root.path().join("alias")).unwrap();
    assert_eq!(
        prepare_export("alias", root.path(), &AtomicBool::new(false), deadline())
            .unwrap()
            .bytes,
        b"abc"
    );
    symlink(outside.path().join("file"), root.path().join("escape")).unwrap();
    assert!(matches!(
        prepare_export("escape", root.path(), &AtomicBool::new(false), deadline()),
        Err(PrepareError::CandidateRejected(_))
    ));
}

#[test]
fn final_replacement_after_stat_is_rejected_without_reading_replacement() {
    let root = fixture();
    let mut hook = |stage| {
        if stage == Stage::BeforeOpen {
            fs::write(root.path().join("replacement"), b"attacker bytes")?;
            fs::rename(root.path().join("replacement"), root.path().join("file"))?;
        }
        Ok(())
    };
    assert!(
        matches!(hooked("file", root.path(), &AtomicBool::new(false), &mut hook),
        Err(PrepareError::CandidateRejected(error)) if error.to_string().contains("changed during admission"))
    );
}

#[test]
fn final_symlink_and_fifo_swaps_fail_closed_without_blocking_open() {
    for fifo in [false, true] {
        let root = fixture();
        let outside = fixture();
        let mut hook = |stage| {
            if stage == Stage::BeforeOpen {
                fs::remove_file(root.path().join("file"))?;
                if fifo {
                    nix::unistd::mkfifo(&root.path().join("file"), Mode::S_IRUSR | Mode::S_IWUSR)?;
                } else {
                    symlink(outside.path().join("file"), root.path().join("file"))?;
                }
            }
            Ok(())
        };
        assert!(matches!(
            hooked("file", root.path(), &AtomicBool::new(false), &mut hook),
            Err(PrepareError::CandidateRejected(_))
        ));
    }
}

#[test]
fn initial_fifo_and_device_are_not_export_candidates() {
    let root = fixture();
    nix::unistd::mkfifo(&root.path().join("fifo"), Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
    assert!(matches!(
        prepare_export("fifo", root.path(), &AtomicBool::new(false), deadline()),
        Err(PrepareError::CandidateRejected(_))
    ));
    assert!(matches!(
        prepare_export(
            "null",
            Path::new("/dev"),
            &AtomicBool::new(false),
            deadline()
        ),
        Err(PrepareError::CandidateRejected(_))
    ));
}

#[test]
fn ancestor_symlink_swap_before_walk_rejects_outside_target() {
    let root = fixture();
    let outside = fixture();
    fs::create_dir(root.path().join("directory")).unwrap();
    fs::write(root.path().join("directory/file"), b"inside").unwrap();
    let mut hook = |stage| {
        if stage == Stage::Resolved {
            fs::rename(
                root.path().join("directory"),
                root.path().join("old-directory"),
            )?;
            symlink(outside.path(), root.path().join("directory"))?;
        }
        Ok(())
    };
    assert!(matches!(
        hooked(
            "directory/file",
            root.path(),
            &AtomicBool::new(false),
            &mut hook
        ),
        Err(PrepareError::CandidateRejected(_))
    ));
}

#[test]
fn growth_truncation_and_in_place_mutation_after_admission_have_no_payload() {
    for (bytes, expected) in [
        (b"abcd".as_slice(), "grew"),
        (b"a".as_slice(), "truncated"),
        (b"xyz".as_slice(), "mutated"),
    ] {
        let root = fixture();
        let mut hook = |stage| {
            if stage == Stage::Admitted {
                let mut file = File::create(root.path().join("file"))?;
                file.write_all(bytes)?;
                file.set_times(FileTimes::new().set_modified(UNIX_EPOCH))?;
            }
            Ok(())
        };
        read_failed(
            hooked("file", root.path(), &AtomicBool::new(false), &mut hook),
            expected,
        );
    }
}

#[test]
fn replacement_or_ancestor_link_after_read_rejects_pinned_payload() {
    for ancestor in [false, true] {
        let root = fixture();
        let outside = fixture();
        fs::create_dir(root.path().join("directory")).unwrap();
        fs::write(root.path().join("directory/file"), b"inside").unwrap();
        let mut hook = |stage| {
            if stage == Stage::BeforeFinal {
                if ancestor {
                    fs::rename(
                        root.path().join("directory"),
                        root.path().join("old-directory"),
                    )?;
                    symlink(outside.path(), root.path().join("directory"))?;
                } else {
                    fs::write(root.path().join("replacement"), b"outside")?;
                    fs::rename(
                        root.path().join("replacement"),
                        root.path().join("directory/file"),
                    )?;
                }
            }
            Ok(())
        };
        read_failed(
            hooked(
                "directory/file",
                root.path(),
                &AtomicBool::new(false),
                &mut hook,
            ),
            "binding changed",
        );
    }
}

#[test]
fn unrelated_sibling_churn_does_not_change_ancestor_identity() {
    let root = fixture();
    let mut hook = |stage| {
        if stage == Stage::Admitted {
            fs::write(root.path().join("sibling"), b"unrelated")?;
        }
        Ok(())
    };
    assert_eq!(
        hooked("file", root.path(), &AtomicBool::new(false), &mut hook)
            .unwrap()
            .bytes,
        b"abc"
    );
}

#[test]
fn cancellation_before_start_and_after_admission_is_a_read_failure() {
    let root = fixture();
    read_failed(
        prepare_export("missing", root.path(), &AtomicBool::new(true), deadline()),
        "cancelled",
    );
    let cancelled = AtomicBool::new(false);
    let mut hook = |stage| {
        if stage == Stage::Admitted {
            cancelled.store(true, Ordering::Release);
        }
        Ok(())
    };
    read_failed(
        hooked("file", root.path(), &cancelled, &mut hook),
        "cancelled",
    );
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    read_failed(
        prepare_export("file", root.path(), &AtomicBool::new(false), expired),
        "deadline",
    );
}

#[test]
fn read_stage_error_after_a_chunk_has_no_successful_prepared_payload() {
    let root = fixture();
    let mut hook = |stage| {
        if stage == Stage::AfterChunk {
            anyhow::bail!("injected read failure");
        }
        Ok(())
    };
    read_failed(
        hooked("file", root.path(), &AtomicBool::new(false), &mut hook),
        "injected read failure",
    );
}
