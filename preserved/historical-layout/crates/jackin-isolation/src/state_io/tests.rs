// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::path::PathBuf;

fn fixture() -> (tempfile::TempDir, PathBuf, StateDirectory) {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    let directory = StateDirectory::open(&state, true).unwrap().unwrap();
    directory.write_file("isolation.json", b"original").unwrap();
    (temp, state, directory)
}

#[test]
fn old_temporary_symlink_cannot_redirect_state_write() {
    let (temp, state, directory) = fixture();
    let canary = temp.path().join("outside");
    std::fs::write(&canary, b"canary").unwrap();
    std::os::unix::fs::symlink(&canary, state.join(".jackin/isolation.json.tmp")).unwrap();
    directory.write_file("isolation.json", b"updated").unwrap();
    assert_eq!(std::fs::read(&canary).unwrap(), b"canary");
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"updated"
    );
}

#[test]
fn ancestor_swap_refuses_commit_and_preserves_both_namespaces() {
    let (temp, state, directory) = fixture();
    let source = directory.read_file("isolation.json").unwrap().unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("isolation.json"), b"canary").unwrap();
    let swap = || {
        std::fs::rename(state.join(".jackin"), state.join(".jackin-original")).unwrap();
        std::os::unix::fs::symlink(&outside, state.join(".jackin")).unwrap();
    };
    assert!(
        directory
            .commit(
                "isolation.json",
                b"updated",
                Some(&source),
                &[],
                CommitHooks {
                    before_validation: Some(&swap),
                    ..CommitHooks::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(state.join(".jackin-original/isolation.json")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(outside.join("isolation.json")).unwrap(),
        b"canary"
    );
}

#[test]
fn replacement_after_validation_is_atomically_restored() {
    let (_temp, state, directory) = fixture();
    let source = directory.read_file("isolation.json").unwrap().unwrap();
    let replace = || {
        std::fs::rename(
            state.join(".jackin/isolation.json"),
            state.join("original-record"),
        )
        .unwrap();
        std::fs::write(state.join(".jackin/isolation.json"), b"new-unrelated-state").unwrap();
    };
    let error = directory
        .commit(
            "isolation.json",
            b"updated",
            Some(&source),
            &[],
            CommitHooks {
                before_install: Some(&replace),
                ..CommitHooks::default()
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("retained recovery artifacts"));
    assert_eq!(
        std::fs::read(state.join("original-record")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"new-unrelated-state"
    );
}

#[test]
fn changed_manifest_after_install_rolls_back_migration() {
    let (_temp, state, directory) = fixture();
    directory
        .write_file("instance.json", b"manifest-original")
        .unwrap();
    let source = directory.read_file("isolation.json").unwrap().unwrap();
    let manifest = directory.read_file("instance.json").unwrap().unwrap();
    let replace = || {
        std::fs::rename(
            state.join(".jackin/instance.json"),
            state.join("original-manifest"),
        )
        .unwrap();
        std::fs::write(state.join(".jackin/instance.json"), b"manifest-replacement").unwrap();
    };
    assert!(
        directory
            .commit(
                "isolation.json",
                b"updated",
                Some(&source),
                &[("instance.json", &manifest)],
                CommitHooks {
                    after_install: Some(&replace),
                    ..CommitHooks::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(state.join(".jackin/instance.json")).unwrap(),
        b"manifest-replacement"
    );
    assert_eq!(
        std::fs::read(state.join("original-manifest")).unwrap(),
        b"manifest-original"
    );
}

#[test]
fn dangling_state_file_alias_is_error() {
    let (_temp, state, directory) = fixture();
    std::fs::remove_file(state.join(".jackin/isolation.json")).unwrap();
    std::os::unix::fs::symlink(state.join("missing"), state.join(".jackin/isolation.json"))
        .unwrap();
    assert!(directory.read_file("isolation.json").is_err());
}

#[test]
fn newer_destination_after_install_is_retained_with_original_recovery() {
    let (_temp, state, directory) = fixture();
    let source = directory.read_file("isolation.json").unwrap().unwrap();
    let replace = || {
        std::fs::rename(
            state.join(".jackin/isolation.json"),
            state.join("interrupted-candidate"),
        )
        .unwrap();
        std::fs::write(state.join(".jackin/isolation.json"), b"newer-state").unwrap();
    };
    assert!(
        directory
            .commit(
                "isolation.json",
                b"updated",
                Some(&source),
                &[],
                CommitHooks {
                    after_install: Some(&replace),
                    ..CommitHooks::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"newer-state"
    );
    let recovery = std::fs::read_dir(state.join(".jackin"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".isolation-txn-")
        })
        .unwrap();
    assert_eq!(
        std::fs::read(recovery.join("candidate")).unwrap(),
        b"original"
    );
}

#[test]
fn competing_state_transaction_returns_actionable_busy_error() {
    let (_temp, state, _directory) = fixture();
    let error = StateDirectory::open(&state, false)
        .err()
        .expect("independent open must not bypass the held state lock");
    assert!(error.to_string().contains("state busy"));
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"original"
    );
}

#[test]
fn staged_candidate_alias_refuses_install_and_preserves_source_and_canary() {
    let (temp, state, directory) = fixture();
    let source = directory.read_file("isolation.json").unwrap().unwrap();
    let canary = temp.path().join("outside");
    std::fs::write(&canary, b"canary").unwrap();
    let inject = || {
        let transaction = std::fs::read_dir(state.join(".jackin"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".isolation-txn-")
            })
            .unwrap();
        std::fs::remove_file(transaction.join("candidate")).unwrap();
        std::os::unix::fs::symlink(&canary, transaction.join("candidate")).unwrap();
    };
    assert!(
        directory
            .commit(
                "isolation.json",
                b"updated",
                Some(&source),
                &[],
                CommitHooks {
                    before_install: Some(&inject),
                    ..CommitHooks::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(state.join(".jackin/isolation.json")).unwrap(),
        b"original"
    );
    assert_eq!(std::fs::read(&canary).unwrap(), b"canary");
}
