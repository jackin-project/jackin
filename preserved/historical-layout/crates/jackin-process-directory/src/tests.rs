// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::current_dir;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::sync::Arc;

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
#[expect(
    clippy::disallowed_methods,
    reason = "isolated synchronous subprocess fixture exercises descriptor zero"
)]
fn descriptor_zero_child() {
    let Some(directory) = std::env::var_os("JACKIN_PINNED_DIRECTORY_FIXTURE") else {
        return;
    };
    nix::unistd::close(0).unwrap();
    let descriptor = Arc::new(File::open(&directory).unwrap());
    assert_eq!(
        descriptor.as_raw_fd(),
        0,
        "fixture must exercise descriptor zero"
    );
    let mut command = Command::new("pwd");
    command.stdin(Stdio::null());
    current_dir(&mut command, descriptor).unwrap();
    let output = command.output().unwrap();
    assert!(output.status.success());
    let actual = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(
        actual.canonicalize().unwrap(),
        std::path::PathBuf::from(directory).canonicalize().unwrap()
    );
}
