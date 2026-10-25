// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn captures_child_stdout_into_spool() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let request = ExecRequest::new("/bin/echo", ["hello"]).output_limits(1 << 20, 1 << 20);
    let idle = AtomicBool::new(false);
    let (_stdout_guard, _stderr_guard, stdout, stderr) = spool_pair(&dir)?;
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &idle)?;
    assert!(result.success);
    assert_eq!(result.code, Some(0));
    let mut spool = result.stdout;
    use std::io::{Read as _, Seek as _};
    spool.rewind()?;
    let mut content = String::new();
    spool.read_to_string(&mut content)?;
    assert_eq!(content, "hello\n");
    Ok(())
}

#[test]
fn failing_exit_reports_code_without_success() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let request =
        ExecRequest::new("/usr/bin/false", Vec::<String>::new()).output_limits(1 << 20, 1 << 20);
    let idle = AtomicBool::new(false);
    let (_stdout_guard, _stderr_guard, stdout, stderr) = spool_pair(&dir)?;
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &idle)?;
    assert!(!result.success);
    assert_eq!(result.code, Some(1));
    Ok(())
}
