// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

use super::capture_limited;

#[test]
fn bounded_capture_preserves_complete_output_at_the_limit() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '1234'; printf 'abcd' >&2"]);
    let output = capture_limited(&mut command, 4, 4).expect("capture at exact limits");
    assert_eq!(output.stdout, b"1234");
    assert_eq!(output.stderr, b"abcd");
}

#[test]
fn bounded_capture_drains_and_rejects_overflow_without_returning_partial_output() {
    for (script, stdout_limit, stderr_limit) in [
        ("head -c 131072 /dev/zero", 4, 4),
        ("head -c 131072 /dev/zero >&2", 4, 4),
    ] {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        let error = capture_limited(&mut command, stdout_limit, stderr_limit)
            .expect_err("overflow must fail closed");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeded 4 byte limit"));
    }
}
