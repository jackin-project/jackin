// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn missing_capture_budgets_are_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let request = ExecRequest::new("/bin/echo", ["hello"]);
    let idle = AtomicBool::new(false);
    let (_stdout_guard, _stderr_guard, stdout, stderr) = spool_pair(&dir)?;
    let error = exec_spooled_sync_cancellable(&request, stdout, stderr, &idle).unwrap_err();
    let failure = error.downcast_ref::<SpoolError>().unwrap();
    assert_eq!(failure.kind, SpoolFailureKind::Request);
    Ok(())
}

#[test]
fn non_capture_stream_mode_is_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let request = ExecRequest::new("/bin/echo", ["hello"])
        .output_limits(1 << 20, 1 << 20)
        .stdout_mode(crate::StdioMode::Null);
    let idle = AtomicBool::new(false);
    let (_stdout_guard, _stderr_guard, stdout, stderr) = spool_pair(&dir)?;
    let error = exec_spooled_sync_cancellable(&request, stdout, stderr, &idle).unwrap_err();
    let failure = error.downcast_ref::<SpoolError>().unwrap();
    assert_eq!(failure.kind, SpoolFailureKind::Request);
    Ok(())
}
