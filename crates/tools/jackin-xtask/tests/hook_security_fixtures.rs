// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use anyhow::{Context, Result, ensure};
use jackin_process::{ExecRequest, exec_sync};
use std::{ffi::OsStr, path::Path, time::Duration};

#[test]
fn canonical_hook_security_fixtures_run_with_prebuilt_native_binaries() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .context("resolving security fixture repository")?;
    let temporary = tempfile::Builder::new()
        .prefix("jackin-hook-security-")
        .tempdir_in(&root)
        .context("creating private security fixture temporary parent")?;
    let request = ExecRequest::new("sh", ["scripts/test-pre-push-security"])
        .cwd(&root)
        .envs([
            (
                OsStr::new("JACKIN_PRECOMMIT_OWNER_TEST_BIN"),
                OsStr::new(env!("CARGO_BIN_EXE_jackin-precommit-owner")),
            ),
            (OsStr::new("TMPDIR"), temporary.path().as_os_str()),
        ])
        .timeout(Duration::from_secs(120))
        .output_limits(1024 * 1024, 1024 * 1024);
    assert_fixture(&request, "pre-push pinned launcher fixture passed")?;

    let request = ExecRequest::new(
        env!("CARGO_BIN_EXE_jackin-xtask"),
        ["security-audit-fixtures"],
    )
    .cwd(&root)
    .timeout(Duration::from_secs(120))
    .output_limits(1024 * 1024, 1024 * 1024);
    assert_fixture(&request, "pinned security-audit synthetic fixtures passed")
}

fn assert_fixture(request: &ExecRequest, marker: &str) -> Result<()> {
    let output = exec_sync(request).context("running genuine hook security fixture")?;
    ensure!(
        output.success && !output.timed_out,
        "hook security fixture failed (status {:?}): {}{}",
        output.code,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains(marker),
        "hook security fixture omitted its completion marker: {marker}"
    );
    Ok(())
}
