// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::current_dir;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use super::macos_wait::classify_tokio_sigchld_eligibility;
#[cfg(target_os = "macos")]
use super::{SigchldDisposition, TokioSigchldEligibility};

#[expect(
    clippy::disallowed_methods,
    reason = "synchronous test fixture opens descriptors for process-directory validation"
)]
fn open_test_descriptor(path: impl AsRef<std::path::Path>) -> File {
    File::open(path).expect("open process-directory test descriptor")
}

#[cfg(target_os = "macos")]
#[test]
fn tokio_sigchld_default_flags_are_symbolic_subsets() {
    for flags in [
        0,
        libc::SA_RESTART,
        libc::SA_SIGINFO,
        libc::SA_RESTART | libc::SA_SIGINFO,
    ] {
        assert_eq!(
            classify_tokio_sigchld_eligibility(SigchldDisposition::Default, flags, 0),
            TokioSigchldEligibility::Eligible,
            "default action flags {flags:#x} should be accepted",
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn tokio_sigchld_rejects_ignore_and_caught() {
    let unsupported_flags = libc::SA_NOCLDWAIT | (1 << 30);
    let nonempty_action_mask: libc::sigset_t = 1;

    assert_eq!(
        classify_tokio_sigchld_eligibility(
            SigchldDisposition::Ignore,
            unsupported_flags,
            nonempty_action_mask,
        ),
        TokioSigchldEligibility::Ignore,
    );
    assert_eq!(
        classify_tokio_sigchld_eligibility(
            SigchldDisposition::OpaqueCaught,
            unsupported_flags,
            nonempty_action_mask,
        ),
        TokioSigchldEligibility::ForeignCaught,
    );
}

#[cfg(target_os = "macos")]
#[test]
fn tokio_sigchld_rejects_childwait_and_unknown_flags() {
    assert_eq!(
        classify_tokio_sigchld_eligibility(
            SigchldDisposition::Default,
            libc::SA_NOCLDWAIT | (1 << 30),
            0,
        ),
        TokioSigchldEligibility::NoChildWait,
    );
    assert_eq!(
        classify_tokio_sigchld_eligibility(SigchldDisposition::Default, libc::SA_NOCLDSTOP, 0,),
        TokioSigchldEligibility::UnsupportedFlags,
    );
    assert_eq!(
        classify_tokio_sigchld_eligibility(SigchldDisposition::Default, 1 << 30, 0,),
        TokioSigchldEligibility::UnsupportedFlags,
    );
}

#[cfg(target_os = "macos")]
#[test]
fn tokio_sigchld_rejects_nonempty_action_mask() {
    let nonempty_action_mask: libc::sigset_t = 1;

    assert_eq!(
        classify_tokio_sigchld_eligibility(
            SigchldDisposition::Default,
            libc::SA_RESTART,
            nonempty_action_mask,
        ),
        TokioSigchldEligibility::NonEmptyActionMask,
    );
}

#[test]
fn descriptor_zero_survives_child_standard_input_replacement() {
    let temporary = tempfile::tempdir().unwrap();
    let status = Command::new("sh")
        .args([
            "-c",
            "exec 0<&-; exec \"$1\" --exact tests::descriptor_zero_child --nocapture",
            "fixture",
        ])
        .arg(std::env::current_exe().unwrap())
        .env("JACKIN_PINNED_DIRECTORY_FIXTURE", temporary.path())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn descriptor_zero_child() {
    let Some(directory) = std::env::var_os("JACKIN_PINNED_DIRECTORY_FIXTURE") else {
        return;
    };
    nix::unistd::close(0).unwrap();
    let descriptor = Arc::new(open_test_descriptor(&directory));
    assert_eq!(
        descriptor.as_raw_fd(),
        0,
        "fixture must exercise descriptor zero"
    );
    let mut command = Command::new("pwd");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    current_dir(&mut command, descriptor).unwrap();
    let mut child = command.spawn().unwrap();
    let mut stdout = Vec::new();
    std::io::Read::read_to_end(&mut child.stdout.take().unwrap(), &mut stdout).unwrap();
    assert!(child.wait().unwrap().success());
    let actual = std::path::PathBuf::from(String::from_utf8(stdout).unwrap().trim());
    assert_eq!(
        actual.canonicalize().unwrap(),
        std::path::PathBuf::from(directory).canonicalize().unwrap()
    );
}

#[test]
fn descriptor_cwd_rejects_a_preset_pathname_cwd() {
    let temporary = tempfile::tempdir().unwrap();
    let mut command = Command::new("true");
    command.current_dir(temporary.path());
    let descriptor = Arc::new(open_test_descriptor(temporary.path()));

    let error = current_dir(&mut command, descriptor)
        .expect_err("pathname and descriptor working directories conflict");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "pathname cwd and descriptor cwd are mutually exclusive"
    );
}

#[test]
fn descriptor_cwd_rejects_a_regular_file_descriptor() {
    let temporary = tempfile::tempdir().unwrap();
    let file_path = temporary.path().join("not-a-directory");
    File::create(&file_path).unwrap();
    let descriptor = Arc::new(open_test_descriptor(file_path));

    let error = current_dir(&mut Command::new("true"), descriptor)
        .expect_err("working directory descriptor must identify a directory");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "pinned working directory descriptor is not a directory"
    );
}
