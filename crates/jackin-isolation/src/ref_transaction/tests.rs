// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::os::unix::fs::symlink;

const OLD: &str = "1111111111111111111111111111111111111111";
const NEW: &str = "2222222222222222222222222222222222222222";
const OTHER: &str = "3333333333333333333333333333333333333333";
const NAME: &str = "refs/heads/jackin/scratch";

fn fixture() -> (tempfile::TempDir, OwnedFd) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("refs/heads/jackin")).expect("refs");
    let fd = nix::fcntl::open(
        dir.path(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .expect("pin");
    (dir, fd)
}

#[test]
fn deletes_exact_loose_ref_and_scratch_log_preserving_head_and_other_logs() {
    let (dir, fd) = fixture();
    std::fs::write(dir.path().join(NAME), format!("{OLD}\n")).expect("ref");
    std::fs::write(dir.path().join("HEAD"), "ref: refs/heads/main\n").expect("head");
    std::fs::create_dir_all(dir.path().join("logs/refs/heads/jackin")).expect("logs");
    std::fs::write(dir.path().join(format!("logs/{NAME}")), "scratch log\n").expect("log");
    std::fs::write(dir.path().join("logs/refs/heads/main"), "main log\n").expect("log");
    prepare(fd.as_fd(), NAME, OLD)
        .expect("prepare")
        .commit()
        .expect("commit");
    assert!(!dir.path().join(NAME).exists());
    assert!(!dir.path().join(format!("logs/{NAME}")).exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("HEAD")).expect("head"),
        "ref: refs/heads/main\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("logs/refs/heads/main")).expect("log"),
        "main log\n"
    );
    assert!(!dir.path().join("packed-refs.lock").exists());
}

#[test]
fn packed_target_and_loose_shadow_delete_without_resurrecting_old_ref() {
    for shadow in [false, true] {
        let (dir, fd) = fixture();
        let packed =
            format!("# pack-refs with: peeled\n{OLD} {NAME}\n^{OTHER}\n{OTHER} refs/tags/keep\n");
        std::fs::write(dir.path().join("packed-refs"), &packed).expect("packed");
        if shadow {
            std::fs::write(dir.path().join(NAME), format!("{NEW}\n")).expect("ref");
        }
        prepare(fd.as_fd(), NAME, if shadow { NEW } else { OLD })
            .expect("prepare")
            .commit()
            .expect("commit");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("packed-refs")).expect("packed"),
            format!("# pack-refs with: peeled\n{OTHER} refs/tags/keep\n")
        );
        assert!(!dir.path().join(NAME).exists());
    }
}

#[test]
fn refuses_symbolic_loose_ref_even_with_matching_packed_shadow() {
    let (dir, fd) = fixture();
    std::fs::write(dir.path().join(NAME), "ref: refs/heads/main\n").expect("ref");
    std::fs::write(dir.path().join("packed-refs"), format!("{OLD} {NAME}\n")).expect("packed");
    assert!(prepare(fd.as_fd(), NAME, OLD).is_err());
    assert!(dir.path().join(NAME).exists());
}

#[test]
fn refuses_symlinked_descendant_and_packed_metadata_without_touching_foreign_file() {
    for packed in [false, true] {
        let (dir, fd) = fixture();
        let foreign = tempfile::tempdir().expect("foreign");
        std::fs::write(foreign.path().join("scratch"), format!("{OLD}\n")).expect("foreign ref");
        if packed {
            symlink(
                foreign.path().join("scratch"),
                dir.path().join("packed-refs"),
            )
            .expect("link");
        } else {
            std::fs::remove_dir(dir.path().join("refs/heads/jackin")).expect("remove");
            symlink(foreign.path(), dir.path().join("refs/heads/jackin")).expect("link");
        }
        assert!(prepare(fd.as_fd(), NAME, OLD).is_err());
        assert_eq!(
            std::fs::read_to_string(foreign.path().join("scratch")).expect("foreign"),
            format!("{OLD}\n")
        );
    }
}

#[test]
fn refuses_existing_or_symlinked_lock_and_preserves_it() {
    let (dir, fd) = fixture();
    let foreign = tempfile::NamedTempFile::new().expect("foreign");
    symlink(foreign.path(), dir.path().join(format!("{NAME}.lock"))).expect("lock");
    assert!(prepare(fd.as_fd(), NAME, &"0".repeat(40)).is_err());
    assert!(
        dir.path()
            .join(format!("{NAME}.lock"))
            .symlink_metadata()
            .expect("lock")
            .file_type()
            .is_symlink()
    );
}

#[test]
fn newer_ref_after_prepare_is_never_deleted() {
    let (dir, fd) = fixture();
    std::fs::write(dir.path().join(NAME), format!("{OLD}\n")).expect("ref");
    let transaction = prepare(fd.as_fd(), NAME, OLD).expect("prepare");
    std::fs::write(dir.path().join(NAME), format!("{NEW}\n")).expect("new ref");
    assert!(transaction.commit().is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(NAME)).expect("ref"),
        format!("{NEW}\n")
    );
}

#[test]
fn descendant_directory_swap_after_prepare_retains_both_old_and_foreign_ref() {
    let (dir, fd) = fixture();
    std::fs::write(dir.path().join(NAME), format!("{OLD}\n")).expect("ref");
    let transaction = prepare(fd.as_fd(), NAME, OLD).expect("prepare");
    let foreign = tempfile::tempdir().expect("foreign");
    std::fs::write(foreign.path().join("scratch"), format!("{NEW}\n")).expect("foreign ref");
    std::fs::rename(
        dir.path().join("refs/heads/jackin"),
        dir.path().join("saved-jackin"),
    )
    .expect("swap");
    symlink(foreign.path(), dir.path().join("refs/heads/jackin")).expect("redirect");
    assert!(transaction.commit().is_err());
    assert_eq!(
        std::fs::read_to_string(foreign.path().join("scratch")).expect("foreign"),
        format!("{NEW}\n")
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("saved-jackin/scratch")).expect("old"),
        format!("{OLD}\n")
    );
}

#[test]
fn duplicate_or_mixed_width_packed_records_fail_before_deletion() {
    for packed in [
        format!("{OLD} {NAME}\n{NEW} {NAME}\n"),
        format!("{OLD} {NAME}\n{} refs/heads/main\n", "a".repeat(64)),
    ] {
        let (dir, fd) = fixture();
        std::fs::write(dir.path().join("packed-refs"), &packed).expect("packed");
        assert!(prepare(fd.as_fd(), NAME, OLD).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("packed-refs")).expect("packed"),
            packed
        );
    }
}

#[test]
fn sha256_missing_ref_with_stale_reflog_preserves_unrelated_packed_refs() {
    let (dir, fd) = fixture();
    let packed = format!("{} refs/heads/main\n", "a".repeat(64));
    std::fs::write(dir.path().join("packed-refs"), &packed).expect("packed");
    std::fs::create_dir_all(dir.path().join("logs/refs/heads/jackin")).expect("logs");
    std::fs::write(dir.path().join(format!("logs/{NAME}")), "stale log\n").expect("log");
    prepare(fd.as_fd(), NAME, &"0".repeat(64))
        .expect("prepare")
        .commit()
        .expect("commit");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("packed-refs")).expect("packed"),
        packed
    );
    assert!(!dir.path().join(format!("logs/{NAME}")).exists());
}

#[test]
fn arbitrary_nonutf8_unrelated_packed_ref_name_is_preserved() {
    let (dir, fd) = fixture();
    std::fs::write(dir.path().join(NAME), format!("{OLD}\n")).expect("ref");
    let mut packed = format!("{OTHER} refs/tags/").into_bytes();
    packed.extend_from_slice(&[0xff, b'\n']);
    std::fs::write(dir.path().join("packed-refs"), &packed).expect("packed");
    prepare(fd.as_fd(), NAME, OLD)
        .expect("prepare")
        .commit()
        .expect("commit");
    assert_eq!(
        std::fs::read(dir.path().join("packed-refs")).expect("packed"),
        packed
    );
}
