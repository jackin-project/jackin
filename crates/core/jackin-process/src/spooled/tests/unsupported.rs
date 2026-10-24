// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(
    unix,
    not(any(
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc"))
    ))
))]

use super::super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn strict_spool_rejects_platform_without_selective_reaping() {
    let stdout = tempfile::tempfile().expect("create stdout spool");
    let stderr = tempfile::tempfile().expect("create stderr spool");
    let request = crate::ExecRequest::new("/bin/true", None::<&str>).output_limits(1, 1);
    let error = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect_err("unsupported platform must reject before spawn");
    let error = error
        .downcast::<SpoolError>()
        .expect("strict spool returns a typed error");
    assert_eq!(error.kind, SpoolFailureKind::Unsupported);
    let (stdout, stderr, recovery) = error.into_parts();
    assert!(stdout.is_some());
    assert!(stderr.is_some());
    assert!(recovery.is_none());
}
