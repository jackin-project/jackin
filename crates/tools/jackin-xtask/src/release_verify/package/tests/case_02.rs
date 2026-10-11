// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn verifies_native_version_probe_output_exactly() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let matching = format!("#!/bin/sh\nprintf 'jackin {VERSION}\\n'\n").into_bytes();
    verify_runnable_version(&matching, "jackin", VERSION).unwrap();

    let mismatching = format!("#!/bin/sh\nprintf 'jackin {VERSION}-wrong\\n'\n").into_bytes();
    let error = verify_runnable_version(&mismatching, "jackin", VERSION)
        .expect_err("a binary with a wrong --version response must fail closed");
    assert!(error.to_string().contains("output does not equal"));
}

#[test]
fn rejects_cross_arch_binary_metadata_mismatch() {
    let mut bytes = vec![0_u8; 20];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[18..20].copy_from_slice(&62_u16.to_le_bytes());

    let error = verify_binary_metadata(&bytes, "aarch64-unknown-linux-gnu")
        .expect_err("x86_64 metadata must not pass the arm64 target check");
    assert!(error.to_string().contains("does not match target"));
}

#[test]
fn validates_source_bound_preview_version() {
    validate_preview_version(
        "0.6.4-preview.123+0123456",
        "0123456789abcdef0123456789abcdef01234567",
    )
    .unwrap();
    assert!(
        validate_preview_version(
            "0.6.4-preview.123+fedcba9",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .is_err()
    );
}

#[test]
fn source_checkout_accepts_admitted_old_sha_after_origin_main_advances() {
    let directory = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "--quiet"]);
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "test"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();
    git(directory.path(), &["add", "tracked"]);
    git(directory.path(), &["commit", "--quiet", "-m", "seed"]);
    git(
        directory.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/jackin-project/jackin.git",
        ],
    );
    git(
        directory.path(),
        &[
            "config",
            &format!("url.file://{}.insteadOf", remote.path().display()),
            "https://github.com/jackin-project/jackin.git",
        ],
    );
    let commit = git(directory.path(), &["rev-parse", "HEAD"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );
    git(
        directory.path(),
        &["checkout", "--quiet", "--detach", &commit],
    );

    verify_source_checkout(directory.path(), &source_manifest(commit.clone())).unwrap();

    fs::write(directory.path().join("untracked"), "must fail\n").unwrap();
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("untracked source changes must fail closed");
    assert!(error.to_string().contains("not clean"));
    fs::remove_file(directory.path().join("untracked")).unwrap();

    fs::write(directory.path().join("tracked"), "hidden modification\n").unwrap();
    git(
        directory.path(),
        &["update-index", "--assume-unchanged", "tracked"],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("assume-unchanged tracked changes must fail closed");
    assert!(error.to_string().contains("assume-unchanged"));
    git(
        directory.path(),
        &["update-index", "--no-assume-unchanged", "tracked"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();

    fs::write(directory.path().join("tracked"), "hidden skip-worktree\n").unwrap();
    git(
        directory.path(),
        &["update-index", "--skip-worktree", "tracked"],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("skip-worktree tracked changes must fail closed");
    assert!(error.to_string().contains("skip-worktree"));
    git(
        directory.path(),
        &["update-index", "--no-skip-worktree", "tracked"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();

    git(
        directory.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "advance"],
    );
    let advanced = git(directory.path(), &["rev-parse", "HEAD"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );
    git(
        directory.path(),
        &["checkout", "--quiet", "--detach", &commit],
    );

    // A queued preview remains bound to its admitted source commit after main
    // advances. The producer's identity check must not substitute the latest
    // branch tip for the event SHA.
    verify_source_checkout(directory.path(), &source_manifest(commit.clone())).unwrap();
    let error = verify_source_checkout(directory.path(), &source_manifest(advanced))
        .expect_err("a manifest for another commit must still fail closed");
    assert!(
        error
            .to_string()
            .contains("does not match source checkout HEAD")
    );

    git(
        directory.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "http://github.com/jackin-project/jackin.git",
        ],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("HTTP GitHub remotes must fail closed");
    assert!(error.to_string().contains("GitHub repository URL"));
}
